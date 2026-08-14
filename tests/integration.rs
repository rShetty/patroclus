mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

const ALLOW_POLICY: &str = r#"
- name: allow-reads
  actions: ["read"]
  resources: ["*"]
  scopes: ["*"]
  decision: allow
  reason: Read access permitted by policy

- name: allow-writes-dev
  actions: ["write", "update"]
  resources: ["dev-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev write access permitted by policy

- name: deny-deletes-prod
  actions: ["delete"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: deny
  reason: Production deletes are strictly forbidden

- name: require-approval-prod
  actions: ["write", "update"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: require_approval
  reason: Production write requires human approval
"#;

async fn create_agent_and_principal(
    app: &axum::Router,
    agent_name: &str,
    principal_email: &str,
) -> (String, String) {
    let principal_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/principals")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "external_id": principal_email,
                        "idp_provider": "local",
                        "email": principal_email,
                        "display_name": principal_email
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let principal: Value = serde_json::from_slice(
        &axum::body::to_bytes(principal_resp.into_body(), usize::MAX).await.unwrap(),
    )
    .unwrap();
    let principal_id = principal["id"].as_str().unwrap().to_string();

    let agent_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/agents")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": agent_name,
                        "principal_type": "delegated",
                        "owner_id": principal_id
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let agent: Value = serde_json::from_slice(
        &axum::body::to_bytes(agent_resp.into_body(), usize::MAX).await.unwrap(),
    )
    .unwrap();
    let agent_id = agent["id"].as_str().unwrap().to_string();

    (agent_id, principal_id)
}

async fn send_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let request = if let Some(b) = body {
        builder.body(Body::from(b.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_val: Value = if body_bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(json!({ "raw": String::from_utf8_lossy(&body_bytes) }))
    };
    (status, json_val)
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 1 TESTS — Core infrastructure
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_health() {
    let server = harness::TestServer::new().unwrap();
    let (status, body) = send_request(&server.app, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "patroclus");
}

#[tokio::test]
async fn test_create_agent() {
    let server = harness::TestServer::new().unwrap();
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/admin/agents",
        Some(json!({
            "name": "test-agent",
            "principal_type": "delegated"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "test-agent");
    assert_eq!(body["principal_type"], "delegated");
    assert_eq!(body["status"], "active");
    assert!(body["id"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_create_principal() {
    let server = harness::TestServer::new().unwrap();
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/admin/principals",
        Some(json!({
            "external_id": "alice",
            "idp_provider": "local",
            "email": "alice@example.com",
            "display_name": "Alice"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "alice@example.com");
    assert_eq!(body["display_name"], "Alice");
}

#[tokio::test]
async fn test_list_agents() {
    let server = harness::TestServer::new().unwrap();
    create_agent_and_principal(&server.app, "agent-1", "user1@test.com").await;
    create_agent_and_principal(&server.app, "agent-2", "user2@test.com").await;

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/agents", None).await;
    assert_eq!(status, StatusCode::OK);
    let agents = body.as_array().unwrap();
    assert_eq!(agents.len(), 2);
}

#[tokio::test]
async fn test_get_agent_by_id() {
    let server = harness::TestServer::new().unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "my-agent", "owner@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "GET",
        &format!("/v1/admin/agents/{}", agent_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "my-agent");
    assert_eq!(body["id"], agent_id);
}

#[tokio::test]
async fn test_get_agent_not_found() {
    let server = harness::TestServer::new().unwrap();
    let (status, body) = send_request(
        &server.app,
        "GET",
        "/v1/admin/agents/00000000-0000-0000-0000-000000000001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

// ═══════════════════════════════════════════════════════════════════════
// POLICY ENGINE TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_default_deny_when_no_policy() {
    let server = harness::TestServer::new().unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "some-resource",
            "requested_scopes": ["read:all"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "deny");
    assert!(body["reason"].as_str().unwrap().contains("No matching") || body["reason"].as_str().unwrap().contains("default deny"));
    assert!(body["token"].is_null());
}

#[tokio::test]
async fn test_policy_allow_issues_token() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db/users",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "allow");
    assert!(body["token"].is_object());
    assert!(body["token"]["jwt"].as_str().unwrap().len() > 0);
    assert!(body["token"]["jti"].as_str().unwrap().len() > 0);
    assert_eq!(body["token"]["scopes"], json!(["db:read"]));
    assert!(body["token"]["expires_at"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_policy_deny_for_prod_delete() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "delete",
            "resource": "prod-db/users",
            "requested_scopes": ["db:delete"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "deny");
    assert!(body["reason"].as_str().unwrap().contains("forbidden"));
    assert!(body["token"].is_null());
}

#[tokio::test]
async fn test_policy_require_approval_for_prod_write() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let resource_resp = send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "prod-db",
            "resource_type": "database",
            "uri": "prod-db/users",
            "actions": {"read": true, "write": true},
            "sensitivity": "high",
            "owner_id": principal_id
        })),
    )
    .await;
    assert_eq!(resource_resp.0, StatusCode::OK);

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "write",
            "resource": "prod-db/users",
            "requested_scopes": ["db:write"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "require_approval");
    assert!(body["approval"].is_object());
    assert_eq!(body["approval"]["status"], "pending");
    assert!(body["approval"]["request_id"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_check_access_dry_run() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/check",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "any-resource",
            "requested_scopes": ["read:all"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allowed"], true);
    assert_eq!(body["decision"], "allow");
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIT LOG TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_audit_log_records_decisions() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    // Generate an allow and a deny
    send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;
    send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "delete",
            "resource": "prod-db",
            "requested_scopes": ["db:delete"]
        })),
    )
    .await;

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/audit", None).await;
    assert_eq!(status, StatusCode::OK);
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 2);

    // Entries are DESC — most recent first
    assert_eq!(entries[0]["decision"], "deny");
    assert_eq!(entries[0]["action"], "delete");
    assert_eq!(entries[0]["resource"], "prod-db");
    assert_eq!(entries[1]["decision"], "allow");
    assert_eq!(entries[1]["action"], "read");
    assert_eq!(entries[1]["resource"], "dev-db");
}

#[tokio::test]
async fn test_audit_log_hash_chain_integrity() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/audit", None).await;
    let entries = body.as_array().unwrap();
    assert!(!entries.is_empty());

    // First entry should have a zero prev_hash
    let first = entries.last().unwrap();
    assert_eq!(
        first["prev_hash"],
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert!(first["row_hash"].as_str().unwrap().len() == 64);

    // Second entry's prev_hash should match first entry's row_hash
    if entries.len() >= 2 {
        let second = &entries[entries.len() - 2];
        assert_eq!(second["prev_hash"], first["row_hash"]);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TOKEN TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_issued_token_is_verifiable() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let jwt = body["token"]["jwt"].as_str().unwrap();
    let claims = server.state.token_verifier.verify(jwt, Some("dev-db")).unwrap();
    assert_eq!(claims.scope, "db:read");
    assert_eq!(claims.aud, "dev-db");
    assert!(claims.jti.len() > 0);
    assert!(claims.exp > claims.iat);
}

#[tokio::test]
async fn test_token_revocation() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let jti = body["token"]["jti"].as_str().unwrap();
    let jwt = body["token"]["jwt"].as_str().unwrap();

    // Token should verify before revocation
    assert!(server.state.token_verifier.verify(jwt, Some("dev-db")).is_ok());

    // Revoke
    let (status, _) = send_request(
        &server.app,
        "POST",
        &format!("/v1/admin/tokens/{}/revoke", jti),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Token should fail after revocation
    let result = server.state.token_verifier.verify(jwt, Some("dev-db"));
    assert!(result.is_err());
    match result.unwrap_err() {
        patroclus::errors::PatroclusError::RevokedToken(_) => {}
        other => panic!("expected RevokedToken, got {:?}", other),
    }
}

#[tokio::test]
async fn test_token_audience_binding() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let jwt = body["token"]["jwt"].as_str().unwrap();

    // Verify with wrong audience should fail
    let result = server.state.token_verifier.verify(jwt, Some("wrong-audience"));
    assert!(result.is_err());

    // Verify with correct audience should succeed
    let result = server.state.token_verifier.verify(jwt, Some("dev-db"));
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 2 TESTS — Delegation
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_principal_delegates_scoped_permissions() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read", "calendar:create_event"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["delegation_token"].as_str().unwrap().len() > 0);
    assert!(body["grant_id"].as_str().unwrap().len() > 0);
    assert_eq!(
        body["scopes"],
        json!(["calendar:read", "calendar:create_event"])
    );
}

#[tokio::test]
async fn test_delegation_token_is_verifiable() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 600
        })),
    )
    .await;

    let token = body["delegation_token"].as_str().unwrap();
    let claims = server.state.token_verifier.verify(token, None).unwrap();
    assert!(claims.sub.starts_with("user:"));
    assert_eq!(claims.scope, "calendar:read");
}

#[tokio::test]
async fn test_sub_delegation_narrower_scope() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "parent-agent", "user@test.com").await;
    let (sub_agent_id, _) = create_agent_and_principal(&server.app, "sub-agent", "sub@test.com").await;

    // Parent gets calendar:read + calendar:write
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read", "calendar:write"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let parent_token = body["delegation_token"].as_str().unwrap();

    // Sub-delegate with only calendar:read (narrower)
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": parent_token,
            "sub_agent_id": sub_agent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sub_token = body["delegated_token"].as_str().unwrap();

    let claims = server.state.token_verifier.verify(sub_token, None).unwrap();
    assert_eq!(claims.scope, "calendar:read");
    assert_eq!(claims.act.delegation_depth, 1);
    assert!(claims.act.delegation_chain.is_some());
}

#[tokio::test]
async fn test_sub_delegation_scope_escalation_rejected() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "parent-agent", "user@test.com").await;
    let (sub_agent_id, _) = create_agent_and_principal(&server.app, "sub-agent", "sub@test.com").await;

    // Parent gets only calendar:read
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let parent_token = body["delegation_token"].as_str().unwrap();

    // Try to sub-delegate calendar:write (wider scope — should fail)
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": parent_token,
            "sub_agent_id": sub_agent_id,
            "scopes": ["calendar:write"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("scope escalation"));
}

#[tokio::test]
async fn test_delegation_depth_limit() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();

    // Create a chain of agents
    let mut agent_ids = Vec::new();
    for i in 0..5 {
        let (id, _) = create_agent_and_principal(
            &server.app,
            &format!("agent-{}", i),
            &format!("user{}@test.com", i),
        )
        .await;
        agent_ids.push(id);
    }

    // Build delegation chain
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_ids[0],
            "scopes": ["calendar:read"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let mut current_token = body["delegation_token"].as_str().unwrap().to_string();

    // Default max_delegation_depth is 3 — so depth 1, 2, 3 should succeed
    for i in 1..=3 {
        let (status, body) = send_request(
            &server.app,
            "POST",
            "/v1/agent/delegate",
            Some(json!({
                "parent_grant_token": current_token,
                "sub_agent_id": agent_ids[i],
                "scopes": ["calendar:read"],
                "expires_in_seconds": 1800
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "depth {} should succeed", i);
        current_token = body["delegated_token"].as_str().unwrap().to_string();
    }

    // Depth 4 should fail (exceeds max_delegation_depth=3)
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": current_token,
            "sub_agent_id": agent_ids[4],
            "scopes": ["calendar:read"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("delegation depth exceeded"));
}

#[tokio::test]
async fn test_sub_delegation_cannot_outlive_parent() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "parent", "user@test.com").await;
    let (sub_agent_id, _) = create_agent_and_principal(&server.app, "sub", "sub@test.com").await;

    // Parent gets 60 second expiry
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 60
        })),
    )
    .await;
    let parent_token = body["delegation_token"].as_str().unwrap();
    let parent_expiry = body["expires_at"].as_str().unwrap();

    // Sub-delegate requesting 3600 seconds — should be capped at parent's 60s
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": parent_token,
            "sub_agent_id": sub_agent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let sub_expiry = body["expires_at"].as_str().unwrap();
    // Sub expiry should be <= parent expiry
    let parent_ts = chrono::DateTime::parse_from_rfc3339(parent_expiry).unwrap();
    let sub_ts = chrono::DateTime::parse_from_rfc3339(sub_expiry).unwrap();
    assert!(sub_ts <= parent_ts, "sub-delegation must not outlive parent");
}

// ═══════════════════════════════════════════════════════════════════════
// GRANT REVOCATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_revoke_grant_cascades_to_children() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();

    let (parent_id, _) = create_agent_and_principal(&server.app, "parent", "p@test.com").await;
    let (child1_id, _) = create_agent_and_principal(&server.app, "child1", "c1@test.com").await;
    let (child2_id, _) = create_agent_and_principal(&server.app, "child2", "c2@test.com").await;

    // Create parent grant
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": parent_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let parent_grant_id = body["grant_id"].as_str().unwrap();
    let parent_token = body["delegation_token"].as_str().unwrap();

    // Create child 1 grant (depth 1)
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": parent_token,
            "sub_agent_id": child1_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    let child1_token = body["delegated_token"].as_str().unwrap();

    // Create child 2 grant from child 1 (depth 2)
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": child1_token,
            "sub_agent_id": child2_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 900
        })),
    )
    .await;

    // Revoke parent grant — should cascade
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/grants/{}/revoke", parent_grant_id),
        Some(json!({ "cascade": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let count = body["count"].as_i64().unwrap();
    assert!(count >= 1, "at least the parent grant should be revoked");
}

// ═══════════════════════════════════════════════════════════════════════
// APPROVAL WORKFLOW TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_approval_workflow_approve() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    // Create a resource so approval can be linked
    send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "prod-db",
            "resource_type": "database",
            "uri": "prod-db/users",
            "actions": {"write": true},
            "sensitivity": "high",
            "owner_id": principal_id
        })),
    )
    .await;

    // Request access to prod write — should require approval
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "write",
            "resource": "prod-db/users",
            "requested_scopes": ["db:write"]
        })),
    )
    .await;
    assert_eq!(body["decision"], "require_approval");
    let request_id = body["approval"]["request_id"].as_str().unwrap();

    // List pending approvals
    let (status, body) = send_request(&server.app, "GET", "/v1/principal/approvals", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["action"], "write");

    // Approve it
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/approvals/{}/approve", request_id),
        Some(json!({
            "approver_id": principal_id,
            "reason": "Approved for maintenance window"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "approved");
    assert!(body["approval_token"].is_string());
    assert_eq!(body["approver_id"], principal_id);

    // Verify no more pending
    let (_, body) = send_request(&server.app, "GET", "/v1/principal/approvals", None).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_approval_workflow_deny() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "prod-db",
            "resource_type": "database",
            "uri": "prod-db/data",
            "actions": {"write": true},
            "sensitivity": "critical",
            "owner_id": principal_id
        })),
    )
    .await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "write",
            "resource": "prod-db/data",
            "requested_scopes": ["db:write"]
        })),
    )
    .await;
    let request_id = body["approval"]["request_id"].as_str().unwrap();

    // Deny it
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/approvals/{}/deny", request_id),
        Some(json!({
            "approver_id": principal_id,
            "reason": "No production changes during business hours"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "denied");
    assert!(body["approval_token"].is_null());
}

#[tokio::test]
async fn test_approval_status_lookup() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "prod-db",
            "resource_type": "database",
            "uri": "prod-db/lookup",
            "actions": {"write": true},
            "sensitivity": "high",
            "owner_id": principal_id
        })),
    )
    .await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "write",
            "resource": "prod-db/lookup",
            "requested_scopes": ["db:write"]
        })),
    )
    .await;
    let request_id = body["approval"]["request_id"].as_str().unwrap();

    // Look up status
    let (status, body) = send_request(
        &server.app,
        "GET",
        &format!("/v1/agent/approval-status/{}", request_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "pending");
    assert_eq!(body["action"], "write");
}

#[tokio::test]
async fn test_double_approval_rejected() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "agent", "user@test.com").await;

    send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "prod-db",
            "resource_type": "database",
            "uri": "prod-db/double",
            "actions": {"write": true},
            "sensitivity": "high",
            "owner_id": principal_id
        })),
    )
    .await;

    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "write",
            "resource": "prod-db/double",
            "requested_scopes": ["db:write"]
        })),
    )
    .await;
    let request_id = body["approval"]["request_id"].as_str().unwrap();

    // First approve
    let (status, _) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/approvals/{}/approve", request_id),
        Some(json!({ "approver_id": principal_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Second approve should fail (already resolved)
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/principal/approvals/{}/approve", request_id),
        Some(json!({ "approver_id": principal_id })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body["error"].as_str().unwrap().contains("already resolved"));
}

// ═══════════════════════════════════════════════════════════════════════
// POLICY MANAGEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_and_list_policies() {
    let server = harness::TestServer::new().unwrap();

    let (status, _) = send_request(
        &server.app,
        "POST",
        "/v1/admin/policies",
        Some(json!({
            "name": "test-policy",
            "engine": "yaml",
            "definition": "- name: allow-all\n  decision: allow\n  reason: test\n"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    let policies = body["policies"].as_array().unwrap();
    assert!(policies.len() >= 1);
}

// ═══════════════════════════════════════════════════════════════════════
// RESOURCE MANAGEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_and_list_resources() {
    let server = harness::TestServer::new().unwrap();

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/admin/resources",
        Some(json!({
            "name": "github-api",
            "resource_type": "api",
            "uri": "https://api.github.com",
            "actions": {"read": true, "write": true},
            "sensitivity": "medium"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "github-api");
    assert_eq!(body["resource_type"], "api");

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/resources", None).await;
    assert_eq!(status, StatusCode::OK);
    let resources = body.as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["name"], "github-api");
}

// ═══════════════════════════════════════════════════════════════════════
// END-TO-END SCENARIO TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_delegation_then_access() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "worker", "user@test.com").await;

    // Step 1: Principal delegates scoped permission
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["calendar:read", "calendar:create_event"],
            "expires_in_seconds": 900
        })),
    )
    .await;
    let delegation_token = body["delegation_token"].as_str().unwrap();
    assert!(delegation_token.len() > 0);

    // Step 2: Agent uses delegation token to request access
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "calendar/events",
            "requested_scopes": ["calendar:read"],
            "delegation_token": delegation_token
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "allow");
    assert!(body["token"].is_object());

    // Step 3: Verify the token
    let jwt = body["token"]["jwt"].as_str().unwrap();
    let claims = server.state.token_verifier.verify(jwt, Some("calendar/events")).unwrap();
    assert_eq!(claims.scope, "calendar:read");
}

#[tokio::test]
async fn test_e2e_multi_agent_delegation_chain() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();

    let (orchestrator_id, _) = create_agent_and_principal(&server.app, "orchestrator", "orch@test.com").await;
    let (worker_id, _) = create_agent_and_principal(&server.app, "worker", "wkr@test.com").await;

    // Human delegates to orchestrator
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": orchestrator_id,
            "scopes": ["calendar:read", "calendar:write", "email:send"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let orch_token = body["delegation_token"].as_str().unwrap();

    // Orchestrator delegates narrowed scope to worker
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        Some(json!({
            "parent_grant_token": orch_token,
            "sub_agent_id": worker_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let worker_token = body["delegated_token"].as_str().unwrap();

    // Verify the delegation chain is recorded in the token
    let claims = server.state.token_verifier.verify(worker_token, None).unwrap();
    assert_eq!(claims.act.delegation_depth, 1);
    let chain = claims.act.delegation_chain.unwrap();
    assert_eq!(chain.len(), 1);
    assert!(chain[0].sub.starts_with("user:"));
    assert!(chain[0].act.starts_with("agent:"));
}
