//! Prometheus `/metrics` endpoint (issue #10).
//!
//! Asserts that authorization decisions increment the decision counter with
//! the right outcome label, that token issuance is counted, that the latency
//! histogram observes requests, and that gauges reflect live state.

mod harness;

use axum::http::StatusCode;
use harness::{AgentHarness, TestServer, create_agent_with_key, send_agent_request, send_request};
use serde_json::json;

const ALLOW_POLICY: &str = r#"
- name: allow-reads
  actions: ["read"]
  resources: ["*"]
  scopes: ["*"]
  decision: allow
  reason: Read access permitted by policy
"#;

const DENY_POLICY: &str = r#"
- name: deny-everything
  decision: deny
  reason: Denied by test policy
"#;

async fn get_metrics(server: &TestServer) -> String {
    let (status, body) = send_request(&server.app, "GET", "/metrics", None).await;
    assert_eq!(status, StatusCode::OK, "metrics endpoint failed: {body}");
    body["raw"].as_str().unwrap_or_default().to_string()
}

fn counter_value(metrics: &str, outcome: &str, action: &str) -> f64 {
    // The exporter renders labels in alphabetical order.
    let needle = format!("action=\"{action}\",outcome=\"{outcome}\"");
    metrics
        .lines()
        .filter(|l| l.starts_with("patroclus_authz_decisions_total"))
        .find_map(|l| {
            l.split_whitespace()
                .last()
                .filter(|_| l.contains(&needle))
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0)
}

#[tokio::test]
async fn metrics_endpoint_serves_prometheus_text() {
    let server = TestServer::new().await.unwrap();

    // Prometheus omits families with no samples; touch each family so the
    // endpoint must render all five documented metric groups.
    server.state.metrics.record_decision("allow", "probe");
    server
        .state
        .metrics
        .request_duration
        .with_label_values(&["GET", "/metrics"])
        .observe(0.001);
    server.state.metrics.active_sessions.set(0);
    server.state.metrics.approval_queue_depth.set(0);
    server.state.metrics.tokens_issued.inc_by(0);

    let body = get_metrics(&server).await;
    assert!(body.contains("patroclus_authz_decisions_total"));
    assert!(body.contains("patroclus_request_duration_seconds"));
    assert!(body.contains("patroclus_active_sessions"));
    assert!(body.contains("patroclus_approval_queue_depth"));
    assert!(body.contains("patroclus_tokens_issued_total"));
}

#[tokio::test]
async fn allow_decision_increments_counter() {
    let server = TestServer::new_with_policy(ALLOW_POLICY).await.unwrap();
    let (agent_id, _pid, key) =
        create_agent_with_key(&server.app, "metrics-allow", "ma@t.dev").await;

    let before = counter_value(&get_metrics(&server).await, "allow", "read");
    assert_eq!(before, 0.0);

    let body = json!({
        "agent_id": agent_id,
        "action": "read",
        "resource": "dev-db",
        "requested_scopes": ["read"],
        "context": { "session_id": "metrics-allow-s1" }
    });
    let (status, resp) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &key,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["decision"], "allow");

    let after = counter_value(&get_metrics(&server).await, "allow", "read");
    assert_eq!(after, 1.0, "allow/read counter must increment by one");
}

#[tokio::test]
async fn deny_decision_increments_counter() {
    let server = TestServer::new_with_policy(DENY_POLICY).await.unwrap();
    let (agent_id, _pid, key) =
        create_agent_with_key(&server.app, "metrics-deny", "md@t.dev").await;

    let before = counter_value(&get_metrics(&server).await, "deny", "write");
    assert_eq!(before, 0.0);

    let body = json!({
        "agent_id": agent_id,
        "action": "write",
        "resource": "prod-db",
        "requested_scopes": ["write"],
        "context": { "session_id": "metrics-deny-s1" }
    });
    let (status, resp) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &key,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["decision"], "deny");

    let after = counter_value(&get_metrics(&server).await, "deny", "write");
    assert_eq!(after, 1.0, "deny/write counter must increment by one");

    // Denials must not touch the allow counter.
    assert_eq!(
        counter_value(&get_metrics(&server).await, "allow", "write"),
        0.0
    );
}

#[tokio::test]
async fn token_issuance_and_gauges_are_observed() {
    let server = TestServer::new_with_policy(ALLOW_POLICY).await.unwrap();
    let (agent_id, _pid, key) =
        create_agent_with_key(&server.app, "metrics-tokens", "mt@t.dev").await;

    async fn tokens_issued(server: &TestServer) -> f64 {
        let metrics = get_metrics(server).await;
        metrics
            .lines()
            .find(|l| l.starts_with("patroclus_tokens_issued_total"))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    assert_eq!(tokens_issued(&server).await, 0.0);

    let body = json!({
        "agent_id": agent_id,
        "action": "read",
        "resource": "dev-db",
        "requested_scopes": ["read"],
        "context": { "session_id": "metrics-tok-s1" }
    });
    let (status, resp) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &key,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert!(resp["token"]["jwt"].as_str().is_some(), "{resp}");

    assert_eq!(tokens_issued(&server).await, 1.0);

    // The session created by the request shows up in the gauge.
    let metrics = get_metrics(&server).await;
    let sessions: f64 = metrics
        .lines()
        .find(|l| l.starts_with("patroclus_active_sessions"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    assert!(sessions >= 1.0, "at least one active session expected");

    // The request itself was observed by the latency histogram.
    assert!(
        metrics.contains("patroclus_request_duration_seconds_count"),
        "histogram must observe requests:\n{metrics}"
    );
}

#[tokio::test]
async fn approval_queue_depth_reflects_pending_requests() {
    let server = TestServer::new_with_policy(
        "- name: needs-approval\n  actions: [\"deploy\"]\n  resources: [\"*\"]\n  scopes: [\"*\"]\n  decision: require_approval\n  reason: needs a human\n",
    )
    .await
    .unwrap();
    let (agent_id, _pid, key) =
        create_agent_with_key(&server.app, "metrics-approval", "mp@t.dev").await;

    async fn queue_depth(server: &TestServer) -> f64 {
        let metrics = get_metrics(server).await;
        metrics
            .lines()
            .find(|l| l.starts_with("patroclus_approval_queue_depth"))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    assert_eq!(queue_depth(&server).await, 0.0);

    let body = json!({
        "agent_id": agent_id,
        "action": "deploy",
        "resource": "prod-cluster",
        "requested_scopes": ["deploy"],
        "context": { "session_id": "metrics-appr-s1" }
    });
    let (status, resp) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &key,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["decision"], "require_approval");

    assert_eq!(queue_depth(&server).await, 1.0);

    // Approving drains the queue.
    let request_id = resp["approval"]["request_id"].as_str().unwrap();
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/approvals/{request_id}/approve"),
        Some(json!({ "approver_id": agent_id, "reason": "ok" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(queue_depth(&server).await, 0.0);
}

#[tokio::test]
async fn decisions_are_counted_across_many_requests() {
    let server = TestServer::new_with_policy(ALLOW_POLICY).await.unwrap();
    let (agent_id, _pid, key) =
        create_agent_with_key(&server.app, "metrics-multi", "mm@t.dev").await;

    for i in 0..5 {
        let body = json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["read"],
            "context": { "session_id": format!("metrics-multi-s{i}") }
        });
        let (status, resp) = send_agent_request(
            &server.app,
            "POST",
            "/v1/agent/request-access",
            &key,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{resp}");
    }

    let after = counter_value(&get_metrics(&server).await, "allow", "read");
    assert_eq!(after, 5.0);

    // Sanity: the audit trail matches the decision count.
    let audit = AgentHarness::get_audit(&server.app).await;
    assert_eq!(audit.len(), 5);
}
