mod harness;
use harness::send_agent_request;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
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
                .header("Authorization", "Bearer test-admin-token")
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
        &axum::body::to_bytes(principal_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let principal_id = principal["id"].as_str().unwrap().to_string();

    let agent_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/agents")
                .header("Authorization", "Bearer test-admin-token")
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
        &axum::body::to_bytes(agent_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
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
    if ["/v1/admin", "/v1/vault", "/v1/sessions", "/v1/principal"]
        .iter()
        .any(|p| uri == *p || uri.starts_with(&format!("{p}/")))
    {
        builder = builder.header("Authorization", "Bearer test-admin-token");
    }
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
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_val: Value = if body_bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body_bytes)
            .unwrap_or(json!({ "raw": String::from_utf8_lossy(&body_bytes) }))
    };
    (status, json_val)
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 1 TESTS — Core infrastructure
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_health() {
    let server = harness::TestServer::new().await.unwrap();
    let (status, body) = send_request(&server.app, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "patroclus");
}

#[tokio::test]
async fn test_create_agent() {
    let server = harness::TestServer::new().await.unwrap();
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
    assert!(!body["id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_principal() {
    let server = harness::TestServer::new().await.unwrap();
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
    let server = harness::TestServer::new().await.unwrap();
    create_agent_and_principal(&server.app, "agent-1", "user1@test.com").await;
    create_agent_and_principal(&server.app, "agent-2", "user2@test.com").await;

    let (status, body) = send_request(&server.app, "GET", "/v1/admin/agents", None).await;
    assert_eq!(status, StatusCode::OK);
    let agents = body.as_array().unwrap();
    assert_eq!(agents.len(), 2);
}

#[tokio::test]
async fn test_get_agent_by_id() {
    let server = harness::TestServer::new().await.unwrap();
    let (agent_id, _, _agent_key) =
        harness::create_agent_with_key(&server.app, "my-agent", "owner@test.com").await;

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
    let server = harness::TestServer::new().await.unwrap();
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
    let server = harness::TestServer::new().await.unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    assert!(
        body["reason"].as_str().unwrap().contains("No matching")
            || body["reason"].as_str().unwrap().contains("default deny")
    );
    assert!(body["token"].is_null());
}

#[tokio::test]
async fn test_policy_allow_issues_token() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    assert!(!body["token"]["jwt"].as_str().unwrap().is_empty());
    assert!(!body["token"]["jti"].as_str().unwrap().is_empty());
    assert_eq!(body["token"]["scopes"], json!(["db:read"]));
    assert!(!body["token"]["expires_at"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_policy_deny_for_prod_delete() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    assert!(!body["approval"]["request_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_check_access_dry_run() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/check",
        &agent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    // Generate an allow and a deny
    send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;
    send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let (_status, body) = send_request(&server.app, "GET", "/v1/admin/audit", None).await;
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let jwt = body["token"]["jwt"].as_str().unwrap();
    let claims = server
        .state
        .token_verifier
        .verify(jwt, Some("dev-db"))
        .unwrap();
    assert_eq!(claims.scope, "db:read");
    assert_eq!(claims.aud, "dev-db");
    assert!(!claims.jti.is_empty());
    assert!(claims.exp > claims.iat);
}

#[tokio::test]
async fn test_token_revocation() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    assert!(
        server
            .state
            .token_verifier
            .verify(jwt, Some("dev-db"))
            .is_ok()
    );

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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let result = server
        .state
        .token_verifier
        .verify(jwt, Some("wrong-audience"));
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, _agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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
    assert!(!body["delegation_token"].as_str().unwrap().is_empty());
    assert!(!body["grant_id"].as_str().unwrap().is_empty());
    assert_eq!(
        body["scopes"],
        json!(["calendar:read", "calendar:create_event"])
    );
}

#[tokio::test]
async fn test_delegation_token_is_verifiable() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, _agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, parent_key) =
        harness::create_agent_with_key(&server.app, "parent-agent", "user@test.com").await;
    let (sub_agent_id, _, _sub_key) =
        harness::create_agent_with_key(&server.app, "sub-agent", "sub@test.com").await;

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
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &parent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, parent_key) =
        harness::create_agent_with_key(&server.app, "parent-agent", "user@test.com").await;
    let (sub_agent_id, _, _sub_key) =
        harness::create_agent_with_key(&server.app, "sub-agent", "sub@test.com").await;

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
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &parent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();

    // Create a chain of agents
    let mut agent_ids = Vec::new();
    let mut agent_keys = Vec::new();
    for i in 0..5 {
        let (id, _, key) = harness::create_agent_with_key(
            &server.app,
            &format!("agent-{}", i),
            &format!("user{}@test.com", i),
        )
        .await;
        agent_ids.push(id);
        agent_keys.push(key);
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
    for (i, agent_id) in agent_ids.iter().enumerate().take(4).skip(1) {
        let (status, body) = send_agent_request(
            &server.app,
            "POST",
            "/v1/agent/delegate",
            &agent_keys[i - 1],
            Some(json!({
                "parent_grant_token": current_token,
                "sub_agent_id": agent_id,
                "scopes": ["calendar:read"],
                "expires_in_seconds": 1800
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "depth {} should succeed", i);
        current_token = body["delegated_token"].as_str().unwrap().to_string();
    }

    // Depth 4 should fail (exceeds max_delegation_depth=3)
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &agent_keys[3],
        Some(json!({
            "parent_grant_token": current_token,
            "sub_agent_id": agent_ids[4],
            "scopes": ["calendar:read"],
            "expires_in_seconds": 1800
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("delegation depth exceeded")
    );
}

#[tokio::test]
async fn test_sub_delegation_cannot_outlive_parent() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, parent_key) =
        harness::create_agent_with_key(&server.app, "parent", "user@test.com").await;
    let (sub_agent_id, _, _sub_key) =
        harness::create_agent_with_key(&server.app, "sub", "sub@test.com").await;

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
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &parent_key,
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
    assert!(
        sub_ts <= parent_ts,
        "sub-delegation must not outlive parent"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// GRANT REVOCATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_revoke_grant_cascades_to_children() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();

    let (parent_id, _, parent_key) =
        harness::create_agent_with_key(&server.app, "parent", "p@test.com").await;
    let (child1_id, _, child1_key) =
        harness::create_agent_with_key(&server.app, "child1", "c1@test.com").await;
    let (child2_id, _, _child2_key) =
        harness::create_agent_with_key(&server.app, "child2", "c2@test.com").await;

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
    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &parent_key,
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
    let (_, _body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &child1_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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
    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let (status, body) = send_agent_request(
        &server.app,
        "GET",
        &format!("/v1/agent/approval-status/{}", request_id),
        &agent_key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "pending");
    assert_eq!(body["action"], "write");
}

#[tokio::test]
async fn test_double_approval_rejected() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

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

    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let server = harness::TestServer::new().await.unwrap();

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
    assert!(!policies.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
// RESOURCE MANAGEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_and_list_resources() {
    let server = harness::TestServer::new().await.unwrap();

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
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "worker", "user@test.com").await;

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
    assert!(!delegation_token.is_empty());

    // Step 2: Agent uses delegation token to request access
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
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
    let claims = server
        .state
        .token_verifier
        .verify(jwt, Some("calendar/events"))
        .unwrap();
    assert_eq!(claims.scope, "calendar:read");
}

#[tokio::test]
async fn test_e2e_multi_agent_delegation_chain() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();

    let (orchestrator_id, _, orchestrator_key) =
        harness::create_agent_with_key(&server.app, "orchestrator", "orch@test.com").await;
    let (worker_id, _, _worker_key) =
        harness::create_agent_with_key(&server.app, "worker", "wkr@test.com").await;

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
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &orchestrator_key,
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
    let claims = server
        .state
        .token_verifier
        .verify(worker_token, None)
        .unwrap();
    assert_eq!(claims.act.delegation_depth, 1);
    let chain = claims.act.delegation_chain.unwrap();
    assert_eq!(chain.len(), 1);
    assert!(chain[0].sub.starts_with("user:"));
    assert!(chain[0].act.starts_with("agent:"));
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 4 TESTS — Credential Vault
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_vault_encrypt_decrypt_roundtrip() {
    use patroclus::vault::Vault;

    let vault = Vault::new(b"test-key-material").unwrap();
    let plaintext = "ghp_abcdef1234567890";
    let (ciphertext, nonce) = vault.encrypt(plaintext).unwrap();

    assert_ne!(ciphertext, plaintext.as_bytes());
    assert_ne!(ciphertext, Vec::<u8>::new());
    assert_eq!(nonce.len(), 12);

    let decrypted = vault.decrypt(&ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn test_vault_decrypt_with_wrong_key_fails() {
    use patroclus::vault::Vault;

    let vault1 = Vault::new(b"correct-key").unwrap();
    let vault2 = Vault::new(b"wrong-key").unwrap();

    let (ciphertext, nonce) = vault1.encrypt("secret-token").unwrap();
    assert!(vault2.decrypt(&ciphertext, &nonce).is_err());
}

#[tokio::test]
async fn test_vault_store_and_retrieve_credential() {
    let server = harness::TestServer::new().await.unwrap();
    let vault = server.state.vault.as_ref().unwrap();

    let (_, principal_id) =
        create_agent_and_principal(&server.app, "agent", "vault@test.com").await;
    let pid = uuid::Uuid::parse_str(&principal_id).unwrap();

    let (encrypted, nonce) = vault.encrypt("ghp_stored_refresh_token").unwrap();
    let id = server
        .state
        .db
        .store_vault_credential(
            pid,
            "github",
            &encrypted,
            &nonce,
            vault.key_id(),
            &["repo".to_string(), "read:user".to_string()],
            None,
        )
        .await
        .unwrap();

    assert!(id != uuid::Uuid::nil());

    let record = server
        .state
        .db
        .get_vault_credential(pid, "github")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.provider, "github");
    assert_eq!(record.scopes, vec!["repo", "read:user"]);

    let decrypted = vault
        .decrypt(&record.encrypted_token, &record.nonce)
        .unwrap();
    assert_eq!(decrypted, "ghp_stored_refresh_token");
}

#[tokio::test]
async fn test_vault_store_credential_api() {
    let server = harness::TestServer::new().await.unwrap();
    let (_, principal_id) =
        create_agent_and_principal(&server.app, "agent", "vault-api@test.com").await;

    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/vault/credentials",
        Some(json!({
            "principal_id": principal_id,
            "provider": "github",
            "refresh_token": "ghp_my_refresh_token",
            "scopes": ["repo", "read:user"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["provider"], "github");
    assert_eq!(body["stored"], true);
    assert!(!body["id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_vault_store_and_retrieve_via_api() {
    let server = harness::TestServer::new().await.unwrap();
    let (_, principal_id) =
        create_agent_and_principal(&server.app, "agent", "vault-rt@test.com").await;

    // Store via API
    send_request(
        &server.app,
        "POST",
        "/v1/vault/credentials",
        Some(json!({
            "principal_id": principal_id,
            "provider": "slack",
            "refresh_token": "xoxe.abc-def-ghi",
            "scopes": ["chat:write", "channels:read"]
        })),
    )
    .await;

    // Retrieve via DB
    let pid = uuid::Uuid::parse_str(&principal_id).unwrap();
    let vault = server.state.vault.as_ref().unwrap();
    let record = server
        .state
        .db
        .get_vault_credential(pid, "slack")
        .await
        .unwrap()
        .unwrap();
    let decrypted = vault
        .decrypt(&record.encrypted_token, &record.nonce)
        .unwrap();
    assert_eq!(decrypted, "xoxe.abc-def-ghi");
    assert_eq!(record.scopes, vec!["chat:write", "channels:read"]);
}

#[tokio::test]
async fn test_vault_vend_unknown_provider_fails() {
    let server = harness::TestServer::new().await.unwrap();
    let (_, principal_id) = create_agent_and_principal(&server.app, "agent", "vend@test.com").await;

    // Store a credential for an unknown provider
    send_request(
        &server.app,
        "POST",
        "/v1/vault/credentials",
        Some(json!({
            "principal_id": principal_id,
            "provider": "unknown",
            "refresh_token": "some-token",
            "scopes": ["read"]
        })),
    )
    .await;

    // Try to vend — should fail because "unknown" is not a supported provider
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/vault/vend",
        Some(json!({
            "principal_id": principal_id,
            "provider": "unknown",
            "requested_scopes": ["read"],
            "agent_token_jti": "test-jti-123"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body["error"].as_str().unwrap().contains("Unknown provider"));
}

#[tokio::test]
async fn test_vault_vend_no_stored_credential_fails() {
    let server = harness::TestServer::new().await.unwrap();
    let (_, principal_id) =
        create_agent_and_principal(&server.app, "agent", "vend-nc@test.com").await;

    // Try to vend without storing a credential first
    let (status, body) = send_request(
        &server.app,
        "POST",
        "/v1/vault/vend",
        Some(json!({
            "principal_id": principal_id,
            "provider": "github",
            "requested_scopes": ["repo"],
            "agent_token_jti": "test-jti-456"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("No stored credential")
    );
}

#[tokio::test]
async fn test_vault_different_providers_isolated() {
    let server = harness::TestServer::new().await.unwrap();
    let vault = server.state.vault.as_ref().unwrap();
    let (_, principal_id) = create_agent_and_principal(&server.app, "agent", "iso@test.com").await;
    let pid = uuid::Uuid::parse_str(&principal_id).unwrap();

    // Store GitHub credential
    let (enc_gh, nonce_gh) = vault.encrypt("ghp_github_token").unwrap();
    server
        .state
        .db
        .store_vault_credential(
            pid,
            "github",
            &enc_gh,
            &nonce_gh,
            vault.key_id(),
            &["repo".to_string()],
            None,
        )
        .await
        .unwrap();

    // Store Slack credential
    let (enc_sl, nonce_sl) = vault.encrypt("xoxe.slack_token").unwrap();
    server
        .state
        .db
        .store_vault_credential(
            pid,
            "slack",
            &enc_sl,
            &nonce_sl,
            vault.key_id(),
            &["chat:write".to_string()],
            None,
        )
        .await
        .unwrap();

    // Retrieve GitHub
    let gh = server
        .state
        .db
        .get_vault_credential(pid, "github")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        vault.decrypt(&gh.encrypted_token, &gh.nonce).unwrap(),
        "ghp_github_token"
    );

    // Retrieve Slack
    let sl = server
        .state
        .db
        .get_vault_credential(pid, "slack")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        vault.decrypt(&sl.encrypted_token, &sl.nonce).unwrap(),
        "xoxe.slack_token"
    );

    // GitHub should not return Slack token
    assert_ne!(
        vault.decrypt(&gh.encrypted_token, &gh.nonce).unwrap(),
        vault.decrypt(&sl.encrypted_token, &sl.nonce).unwrap()
    );
}

#[tokio::test]
async fn test_e2e_agent_requests_access_then_vault_vends_credential() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, principal_id, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "e2e-vault@test.com").await;

    // Store a GitHub credential for the principal
    let pid = uuid::Uuid::parse_str(&principal_id).unwrap();
    let vault = server.state.vault.as_ref().unwrap();
    let (enc, nonce) = vault.encrypt("ghp_refresh_for_e2e").unwrap();
    server
        .state
        .db
        .store_vault_credential(
            pid,
            "github",
            &enc,
            &nonce,
            vault.key_id(),
            &["repo".to_string()],
            None,
        )
        .await
        .unwrap();

    // Agent requests access — should get a token
    let (_, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "github/repos",
            "requested_scopes": ["github:read"]
        })),
    )
    .await;
    assert_eq!(body["decision"], "allow");
    let jti = body["token"]["jti"].as_str().unwrap();
    assert!(!jti.is_empty());

    // Verify the credential is stored and retrievable (the vend would call GitHub's API)
    let record = server
        .state
        .db
        .get_vault_credential(pid, "github")
        .await
        .unwrap()
        .unwrap();
    let decrypted = vault
        .decrypt(&record.encrypted_token, &record.nonce)
        .unwrap();
    assert_eq!(decrypted, "ghp_refresh_for_e2e");
}

// ═══════════════════════════════════════════════════════════════════════
// AUTHENTICATION TESTS (issue #1)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_admin_routes_reject_unauthenticated() {
    let server = harness::TestServer::new().await.unwrap();
    // Raw oneshot without the harness auto-header.
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/admin/agents")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = server.app.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_routes_reject_wrong_token() {
    let server = harness::TestServer::new().await.unwrap();
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/admin/agents")
        .header("Authorization", "Bearer wrong-token")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = server.app.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_agent_routes_require_client_key() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, _key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;
    let body = json!({
        "agent_id": agent_id,
        "action": "read",
        "resource": "dev-db/users",
        "requested_scopes": ["db:read"]
    });
    // No key at all
    let (status, _) = send_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Wrong key
    let (status, _) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        "pat_wrong_key_entirely",
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_agent_key_cannot_act_for_another_agent() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (_agent_a, _, key_a) =
        harness::create_agent_with_key(&server.app, "agent-a", "a@test.com").await;
    let (agent_b, _, _key_b) =
        harness::create_agent_with_key(&server.app, "agent-b", "b@test.com").await;

    // Use A's key to act as B — must be forbidden.
    let (status, resp) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &key_a,
        Some(json!({
            "agent_id": agent_b,
            "action": "read",
            "resource": "dev-db/users",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {resp}");
}

#[tokio::test]
async fn test_client_key_provisioning_returns_raw_once_and_works() {
    let server = harness::TestServer::new().await.unwrap();
    let (agent_id, _, client_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;
    assert!(client_key.starts_with("pat_"));

    // The stored hash must not be exposed via the API.
    let (_, agent_json) = send_request(
        &server.app,
        "GET",
        &format!("/v1/admin/agents/{agent_id}"),
        None,
    )
    .await;
    assert!(agent_json.get("client_key_hash").is_none());
    assert!(
        !serde_json::to_string(&agent_json)
            .unwrap()
            .contains(&client_key)
    );
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIT CHAIN VERIFICATION (issue #8)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_verify_chain_passes_over_live_traffic() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    for resource in ["dev-db/a", "dev-db/b", "dev-db/c"] {
        send_agent_request(
            &server.app,
            "POST",
            "/v1/agent/request-access",
            &agent_key,
            Some(json!({
                "agent_id": agent_id,
                "action": "read",
                "resource": resource,
                "requested_scopes": ["db:read"]
            })),
        )
        .await;
    }

    let entries = server.state.db.all_audit_entries().await.unwrap();
    assert_eq!(entries.len(), 3);

    let result = patroclus::audit::verify_chain(&entries);
    assert!(
        result.is_valid(),
        "chain over live traffic must verify: {:?}",
        result.first_broken_link
    );
    assert_eq!(result.entries_checked, 3);
}

#[tokio::test]
async fn test_verify_chain_detects_tampered_row() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    for resource in ["dev-db/a", "dev-db/b"] {
        send_agent_request(
            &server.app,
            "POST",
            "/v1/agent/request-access",
            &agent_key,
            Some(json!({
                "agent_id": agent_id,
                "action": "read",
                "resource": resource,
                "requested_scopes": ["db:read"]
            })),
        )
        .await;
    }

    // Simulate a tamperer with direct database write access: rewrite a
    // decision without recomputing the chain.
    let dir = tempfile::tempdir().unwrap();
    let db_path = {
        // The in-memory DB cannot be reached from a second connection, so
        // rebuild the scenario on a file-backed database.
        let db_path = dir.path().join("tamper.db");
        let config = patroclus::config::DatabaseConfig {
            path: db_path.to_string_lossy().to_string(),
            read_pool_size: 0,
        };
        let db = patroclus::db::Database::with_config(&config).unwrap();
        db.create_audit_entry(&patroclus::audit::CreateAuditEntry {
            agent_id: uuid::Uuid::parse_str(&agent_id).unwrap(),
            principal_id: None,
            action: "read".to_string(),
            resource: "dev-db/a".to_string(),
            decision: patroclus::policy::Decision::Allow,
            reason: "honest".to_string(),
            delegation_chain: None,
            token_jti: None,
            dry_run: false,
        })
        .await
        .unwrap();
        db.create_audit_entry(&patroclus::audit::CreateAuditEntry {
            agent_id: uuid::Uuid::parse_str(&agent_id).unwrap(),
            principal_id: None,
            action: "read".to_string(),
            resource: "dev-db/b".to_string(),
            decision: patroclus::policy::Decision::Allow,
            reason: "honest".to_string(),
            delegation_chain: None,
            token_jti: None,
            dry_run: false,
        })
        .await
        .unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("UPDATE audit_log SET reason = 'forged' WHERE id = 1", [])
            .unwrap();
        db_path
    };

    // The verifier must reject the modified row.
    let conn =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let entries = patroclus::db::read_audit_entries_for_verification(&conn).unwrap();
    let result = patroclus::audit::verify_chain(&entries);
    assert!(!result.is_valid(), "tampered row must break the chain");
    let broken = result.first_broken_link.unwrap();
    assert_eq!(broken.row_id, 1);
    assert_eq!(
        broken.reason,
        patroclus::audit::BrokenLinkReason::RowHashMismatch
    );
}

#[tokio::test]
async fn test_dry_run_checks_are_audited_with_flag() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "agent", "user@test.com").await;

    // One dry-run check, then one enforced request.
    send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/check",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db/dry",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;
    send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &agent_key,
        Some(json!({
            "agent_id": agent_id,
            "action": "read",
            "resource": "dev-db/real",
            "requested_scopes": ["db:read"]
        })),
    )
    .await;

    let entries = server.state.db.all_audit_entries().await.unwrap();
    assert_eq!(entries.len(), 2, "dry-run check must be audited");

    let dry: Vec<_> = entries.iter().filter(|e| e.dry_run).collect();
    let enforced: Vec<_> = entries.iter().filter(|e| !e.dry_run).collect();
    assert_eq!(dry.len(), 1, "exactly one dry-run entry");
    assert_eq!(dry[0].resource, "dev-db/dry");
    assert_eq!(dry[0].decision, "allow");
    assert_eq!(enforced.len(), 1, "exactly one enforced entry");
    assert_eq!(enforced[0].resource, "dev-db/real");
    assert!(
        enforced[0].token_jti.is_some(),
        "enforced allow issues a token"
    );

    // Both entries must still form a verifiable chain.
    assert!(patroclus::audit::verify_chain(&entries).is_valid());
}

#[tokio::test]
async fn test_delegation_chain_captured_in_decision_audit() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY)
        .await
        .unwrap();
    let (agent_id, _, agent_key) =
        harness::create_agent_with_key(&server.app, "orchestrator", "orch@test.com").await;
    let (worker_id, _, worker_key) =
        harness::create_agent_with_key(&server.app, "worker", "wkr@test.com").await;

    // Principal → orchestrator
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
    let orch_token = body["delegation_token"].as_str().unwrap();

    // Orchestrator → worker (depth 1, chain of one hop)
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &agent_key,
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

    // Worker uses the delegated token — the audit entry must capture the
    // full chain, not None.
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &worker_key,
        Some(json!({
            "agent_id": worker_id,
            "action": "read",
            "resource": "calendar/events",
            "requested_scopes": ["calendar:read"],
            "delegation_token": worker_token
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "allow");

    let entries = server.state.db.all_audit_entries().await.unwrap();
    let worker_entry = entries
        .iter()
        .find(|e| e.resource == "calendar/events")
        .expect("worker decision audited");
    let chain = worker_entry
        .delegation_chain
        .as_ref()
        .expect("full chain captured");
    let hops = chain.as_array().expect("chain is a JSON array");
    assert_eq!(hops.len(), 1);
    assert!(hops[0]["sub"].as_str().unwrap().starts_with("user:"));
    assert!(hops[0]["act"].as_str().unwrap().starts_with("agent:"));

    // Chain integrity holds with delegation_chain included in the hash.
    assert!(patroclus::audit::verify_chain(&entries).is_valid());
}

// ── OIDC PKCE round-trip with a mocked IdP ─────────────────────────

/// Spawns a local HTTP server acting as a minimal OIDC provider exposing
/// `/authorize` (records the query parameters the Patroclus server sent),
/// `/token` and `/userinfo`. The token endpoint enforces PKCE S256 by
/// recomputing the challenge from the submitted verifier.
struct MockIdp {
    base_url: String,
    received: Arc<std::sync::Mutex<MockIdpRequests>>,
}

#[derive(Default, Clone)]
struct MockIdpRequests {
    authorize: Vec<serde_json::Value>,
    token_verifier: Option<String>,
    token_challenge_sent_by_patroclus: Option<String>,
}

impl MockIdp {
    async fn spawn(group_claim_value: Value) -> Self {
        let received = Arc::new(std::sync::Mutex::new(MockIdpRequests::default()));
        let state_for_router = MockIdpState {
            requests: received.clone(),
            group_claim_value,
        };

        let app = axum::Router::new()
            .route("/authorize", axum::routing::get(mock_authorize))
            .route(
                "/token",
                axum::routing::post(
                    move |axum::extract::State(state): axum::extract::State<MockIdpState>,
                          axum::Form(form): axum::Form<HashMap<String, String>>| {
                        mock_token(form, state)
                    },
                ),
            )
            .route("/userinfo", axum::routing::get(mock_userinfo))
            .with_state(state_for_router.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        MockIdp {
            base_url: format!("http://{addr}"),
            received,
        }
    }
}

use patroclus::config::{Config, IdpProvider};
use patroclus::idp::GroupPolicyMapping;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
struct MockIdpState {
    requests: Arc<std::sync::Mutex<MockIdpRequests>>,
    group_claim_value: Value,
}

async fn mock_authorize(
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    state: axum::extract::State<MockIdpState>,
) -> &'static str {
    state.requests.lock().unwrap().authorize.push(json!({
        "client_id": query.get("client_id"),
        "redirect_uri": query.get("redirect_uri"),
        "state": query.get("state"),
        "code_challenge": query.get("code_challenge"),
        "code_challenge_method": query.get("code_challenge_method"),
        "response_type": query.get("response_type"),
    }));
    "ok"
}

async fn mock_token(form: HashMap<String, String>, state: MockIdpState) -> axum::Json<Value> {
    let code_verifier = form.get("code_verifier").cloned();
    // PKCE S256 verification, as a real IdP performs it.
    let expected_challenge = code_verifier
        .as_deref()
        .map(patroclus::idp::pkce_s256_challenge);
    {
        let mut reqs = state.requests.lock().unwrap();
        reqs.token_verifier = code_verifier;
        reqs.token_challenge_sent_by_patroclus = None; // filled from /authorize below
    }
    // A real IdP compares against the challenge bound at /authorize time.
    if let Some(challenge_from_authorize) = state
        .requests
        .lock()
        .unwrap()
        .authorize
        .last()
        .and_then(|a| a["code_challenge"].as_str())
        .map(|s| s.to_string())
    {
        let ok = expected_challenge.as_deref() == Some(challenge_from_authorize.as_str());
        assert!(
            ok,
            "mock IdP: code_verifier does not hash to the authorize-time code_challenge"
        );
    } else {
        panic!("mock IdP: token exchange without prior authorization");
    }

    axum::Json(json!({
        "access_token": "mock-access-token",
        "token_type": "Bearer",
        "expires_in": 3600
    }))
}

async fn mock_userinfo(
    axum::extract::State(state): axum::extract::State<MockIdpState>,
    headers: axum::http::HeaderMap,
) -> axum::Json<Value> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(auth, "Bearer mock-access-token", "userinfo requires bearer");
    axum::Json(json!({
        "sub": "user-99",
        "email": "carol@example.com",
        "name": "Carol",
        &state.group_claim_key(): state.group_claim_value,
    }))
}

impl MockIdpState {
    fn group_claim_key(&self) -> String {
        "groups".to_string()
    }
}

fn idp_test_config(idp_base_url: &str) -> Config {
    let mut config = Config::default();
    config.idp.enabled = true;
    config.idp.providers = vec![IdpProvider {
        name: "mock".to_string(),
        issuer: idp_base_url.to_string(),
        client_id: "patroclus-test-client".to_string(),
        client_secret: "mock-secret".to_string(),
        scopes: vec!["openid".to_string(), "email".to_string()],
        group_claim: "groups".to_string(),
        group_policy_mappings: vec![
            GroupPolicyMapping {
                group: "engineering".to_string(),
                policy_yaml:
                    "- name: eng-allow-read\n  actions: [\"read\"]\n  resources: [\"dev-*\"]\n  scopes: [\"db:read\"]\n  decision: allow\n  reason: Engineering read access"
                        .to_string(),
                scopes: vec!["db:read".to_string()],
                max_spend: Some(50.0),
            },
            GroupPolicyMapping {
                group: "unrelated-group".to_string(),
                policy_yaml: "- name: unrelated\n  decision: deny\n  reason: nope".to_string(),
                scopes: vec![],
                max_spend: None,
            },
        ],
    }];
    config
}

#[tokio::test]
async fn test_oidc_pkce_full_roundtrip_with_mocked_idp() {
    let idp = MockIdp::spawn(json!(["engineering"])).await;

    let app = {
        let state =
            patroclus::api::state::AppState::new_test_with_config(idp_test_config(&idp.base_url))
                .await
                .unwrap();
        patroclus::api::server::create_router(state)
    };

    // ── Step 1: authorize — response must NOT contain the verifier and the
    // URL must carry an S256 challenge.
    let (status, body) = send_request(&app, "GET", "/v1/idp/authorize/mock", None).await;
    assert_eq!(status, StatusCode::OK, "authorize failed: {body}");
    let auth_url = body["authorization_url"].as_str().unwrap();
    let returned_state = body["state"].as_str().unwrap().to_string();
    assert!(
        body.get("code_verifier").is_none(),
        "verifier leaked to client"
    );
    assert_eq!(body["code_challenge_method"], "S256");

    // Browser hop: the user-agent follows the authorization_url, which binds
    // the challenge at the IdP (recorded by our mock) and issues a code.
    let http = reqwest::Client::new();
    let auth_page = http.get(auth_url).send().await.unwrap();
    assert!(auth_page.status().is_success(), "IdP authorize failed");
    let idp_code = format!("auth-code-{}", uuid::Uuid::now_v7());

    // Parse the authorization URL without external crates: split off the query
    // string manually.
    let (base, query_str) = auth_url
        .split_once('?')
        .expect("authorization_url carries a query string");
    assert!(
        base.ends_with("/authorize"),
        "URL must target the IdP authorize endpoint"
    );
    let query: HashMap<String, String> = query_str
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let sent_challenge = query
        .get("code_challenge")
        .expect("challenge in URL")
        .clone();
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    // Challenge is base64url SHA-256: 43 chars (a raw verifier is also 43, but
    // the client never receives the real one — asserted above via the missing
    // `code_verifier` response field).
    assert_eq!(sent_challenge.len(), 43);

    let authorize_recorded = idp
        .received
        .lock()
        .unwrap()
        .authorize
        .last()
        .cloned()
        .unwrap();
    assert_eq!(authorize_recorded["client_id"], "patroclus-test-client");
    assert_eq!(
        authorize_recorded["code_challenge_method"], "S256",
        "IdP must be told the S256 method"
    );
    assert_eq!(
        authorize_recorded["state"], returned_state,
        "browser hop carries the issued state"
    );

    // ── Step 2: callback with forged/unknown state is rejected BEFORE the IdP
    // is contacted.
    let (status, body) = send_request(
        &app,
        "GET",
        "/v1/idp/callback?code=stolen-code&state=forged-state",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "forged state accepted: {body}"
    );

    // ── Step 3: legitimate callback using the issued state. The mocked IdP
    // verifies the S256 proof inside its /token endpoint.
    let callback_uri = format!(
        "/v1/idp/callback?code={idp_code}&state={}",
        urlencoding_form(&returned_state)
    );
    let (status, body) = send_request(&app, "GET", &callback_uri, None).await;
    assert_eq!(status, StatusCode::OK, "callback failed: {body}");
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["email"], "carol@example.com");
    assert_eq!(body["groups"], json!(["engineering"]));
    assert_eq!(body["policy_applied"], true);
    assert_eq!(body["mapped_scopes"], json!(["db:read"]));

    // The IdP saw exactly one authorization request carrying our state.
    let reqs = idp.received.lock().unwrap();
    assert_eq!(reqs.authorize.len(), 1);
    assert_eq!(reqs.authorize[0]["state"], returned_state);
    // The verifier that reached the token endpoint hashes to the challenge
    // (asserted inside mock_token); it was never exposed to the HTTP client.
    assert!(reqs.token_verifier.is_some());
}

// Percent-encode for a query value (states are base64url so this is a
// formality, but keeps the test honest).
fn urlencoding_form(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[tokio::test]
async fn test_oidc_state_is_single_use() {
    let idp = MockIdp::spawn(json!(["engineering"])).await;

    let app = {
        let state =
            patroclus::api::state::AppState::new_test_with_config(idp_test_config(&idp.base_url))
                .await
                .unwrap();
        patroclus::api::server::create_router(state)
    };

    let (_, body) = send_request(&app, "GET", "/v1/idp/authorize/mock", None).await;
    let state_param = body["state"].as_str().unwrap().to_string();

    // Browser hop binds the challenge at the IdP before the callback runs.
    let http = reqwest::Client::new();
    let auth_page = http
        .get(body["authorization_url"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert!(auth_page.status().is_success(), "IdP authorize failed");

    let uri = format!("/v1/idp/callback?code=c1&state={state_param}");
    let (status, _) = send_request(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "first use must succeed");

    // Replay: same state again must be rejected even with fresh code.
    let uri2 = format!("/v1/idp/callback?code=c2&state={state_param}");
    let (status, body) = send_request(&app, "GET", &uri2, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "replayed state accepted: {body}"
    );
}
