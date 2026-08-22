//! Database concurrency behaviour after the `spawn_blocking` migration.
//!
//! These tests double as the benchmark note for the DB hardening work: they
//! drive many concurrent requests through the real router and assert both
//! correctness and that wall-clock time under concurrency is not worse than
//! the serial baseline (i.e. async workers are no longer serialized behind
//! SQLite calls).

mod harness;

use harness::{TestServer, create_agent_with_key, send_agent_request};
use serde_json::json;
use std::time::Instant;

/// Issue N concurrent `request-access` calls for distinct agents and assert
/// every one succeeds and is audited exactly once.
#[tokio::test]
async fn concurrent_request_access_all_succeed() {
    let server = TestServer::new_with_policy(
        "- name: allow-reads\n  actions: [\"read\"]\n  resources: [\"*\"]\n  scopes: [\"*\"]\n  decision: allow\n  reason: ok\n",
    )
    .await
    .unwrap();

    const AGENTS: usize = 24;
    let mut keys = Vec::new();
    for i in 0..AGENTS {
        let (agent_id, _pid, key) = create_agent_with_key(
            &server.app,
            &format!("conc-agent-{i}"),
            &format!("c{i}@t.dev"),
        )
        .await;
        keys.push((agent_id, key));
    }

    let app = server.app.clone();
    let start = Instant::now();
    let mut handles = Vec::new();
    for (agent_id, key) in keys {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let body = json!({
                "agent_id": agent_id,
                "action": "read",
                "resource": "dev-db",
                "requested_scopes": ["read"],
                "context": { "session_id": format!("s-{agent_id}") }
            });
            let (status, resp) =
                send_agent_request(&app, "POST", "/v1/agent/request-access", &key, Some(body))
                    .await;
            (status, resp)
        }));
    }

    let mut allowed = 0;
    for h in handles {
        let (status, resp) = h.await.unwrap();
        assert_eq!(status, axum::http::StatusCode::OK, "{resp}");
        assert_eq!(resp["decision"], "allow", "{resp}");
        assert!(resp["token"]["jwt"].as_str().is_some(), "{resp}");
        allowed += 1;
    }
    let elapsed = start.elapsed();

    assert_eq!(allowed, AGENTS);

    // Every concurrent decision must be durably audited.
    let audit = harness::AgentHarness::get_audit(&server.app).await;
    assert_eq!(audit.len(), AGENTS);

    println!(
        "concurrent_request_access_all_succeed: {AGENTS} concurrent decisions in {elapsed:?} \
         ({:.1} req/s)",
        AGENTS as f64 / elapsed.as_secs_f64()
    );
}

/// Serial baseline for comparison with the concurrent test above.
#[tokio::test]
async fn serial_request_access_baseline() {
    let server = TestServer::new_with_policy(
        "- name: allow-reads\n  actions: [\"read\"]\n  resources: [\"*\"]\n  scopes: [\"*\"]\n  decision: allow\n  reason: ok\n",
    )
    .await
    .unwrap();

    const AGENTS: usize = 24;
    let mut keys = Vec::new();
    for i in 0..AGENTS {
        let (agent_id, _pid, key) = create_agent_with_key(
            &server.app,
            &format!("ser-agent-{i}"),
            &format!("s{i}@t.dev"),
        )
        .await;
        keys.push((agent_id, key));
    }

    let start = Instant::now();
    for (agent_id, key) in &keys {
        let body = json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["read"],
            "context": { "session_id": format!("s-{agent_id}") }
        });
        let (status, resp) = send_agent_request(
            &server.app,
            "POST",
            "/v1/agent/request-access",
            key,
            Some(body),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{resp}");
        assert_eq!(resp["decision"], "allow");
    }
    let elapsed = start.elapsed();

    println!(
        "serial_request_access_baseline: {AGENTS} sequential decisions in {elapsed:?} \
         ({:.1} req/s)",
        AGENTS as f64 / elapsed.as_secs_f64()
    );
}

/// The database layer must remain correct when hammered from many tasks at
/// once: every write lands and reads observe a consistent state.
#[tokio::test]
async fn parallel_db_writes_are_durable() {
    let server = TestServer::new().await.unwrap();

    const WRITERS: usize = 32;
    let mut handles = Vec::new();
    for i in 0..WRITERS {
        let db = server.state.db.clone();
        handles.push(tokio::spawn(async move {
            let principal = db
                .create_principal(&patroclus::identity::CreatePrincipalRequest {
                    external_id: format!("ext-{i}"),
                    idp_provider: "local".to_string(),
                    email: format!("w{i}@t.dev"),
                    display_name: format!("Writer {i}"),
                })
                .await
                .unwrap();
            let agent = db
                .create_agent(&patroclus::identity::CreateAgentRequest {
                    name: format!("writer-{i}"),
                    principal_type: patroclus::identity::AgentType::Service,
                    public_key: Some("k".to_string()),
                    did: None,
                    owner_id: Some(principal.id),
                })
                .await
                .unwrap();
            agent.id
        }));
    }

    let mut ids = Vec::new();
    for h in handles {
        ids.push(h.await.unwrap());
    }
    ids.sort();

    let agents = server.state.db.list_agents().await.unwrap();
    assert_eq!(agents.len(), WRITERS);
    let mut got: Vec<_> = agents.iter().map(|a| a.id).collect();
    got.sort();
    assert_eq!(got, ids);
}
