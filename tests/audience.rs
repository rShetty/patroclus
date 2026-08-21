//! RFC 8707 audience-binding enforcement on internal verification paths.
//!
//! Delegation grants are addressed to `agent:<recipient>`; presenting a token
//! at `/v1/agent/request-access` or `/v1/agent/delegate` requires the token's
//! `aud` to match the authenticated agent. A token bound to another audience
//! must be rejected even when its signature, scopes and expiry are valid.

mod harness;

use axum::http::StatusCode;
use harness::{create_agent_with_key, send_agent_request, send_request};
use serde_json::json;

const ALLOW_POLICY: &str = r#"
- name: allow-reads
  actions: ["read"]
  resources: ["*"]
  scopes: ["*"]
  decision: allow
  reason: Read access permitted by policy
"#;

/// Principal delegates to `grantee`; returns the delegation JWT (aud =
/// `agent:<grantee>`).
async fn principal_grant(app: &axum::Router, grantee: &str, scopes: &[&str]) -> String {
    let (_, body) = send_request(
        app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": grantee,
            "scopes": scopes,
            "expires_in_seconds": 900
        })),
    )
    .await;
    body["delegation_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn request_access_rejects_delegation_token_bound_to_another_audience() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (holder_id, _, _holder_key) =
        create_agent_with_key(&server.app, "holder", "holder@test.com").await;
    let (other_id, _, other_key) =
        create_agent_with_key(&server.app, "other", "other@test.com").await;

    // Grant addressed to `holder` (aud = agent:<holder_id>).
    let grant = principal_grant(&server.app, &holder_id, &["calendar:read"]).await;

    // `other` presents the holder's grant — signature and scopes are valid,
    // but the token is not addressed to the requesting agent.
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &other_key,
        Some(json!({
            "agent_id": other_id,
            "action": "read",
            "resource": "calendar/events",
            "requested_scopes": ["calendar:read"],
            "delegation_token": grant
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body["error"].as_str().unwrap().contains("invalid token"),
        "expected invalid-token error, got: {}",
        body
    );
}

#[tokio::test]
async fn request_access_accepts_delegation_token_bound_to_requesting_agent() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (holder_id, _, holder_key) =
        create_agent_with_key(&server.app, "holder", "own@test.com").await;

    let grant = principal_grant(&server.app, &holder_id, &["calendar:read"]).await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/request-access",
        &holder_key,
        Some(json!({
            "agent_id": holder_id,
            "action": "read",
            "resource": "calendar/events",
            "requested_scopes": ["calendar:read"],
            "delegation_token": grant
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "allow");
    assert!(body["token"].is_object());
}

#[tokio::test]
async fn delegate_rejects_parent_token_bound_to_another_audience() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (holder_id, _, _holder_key) =
        create_agent_with_key(&server.app, "holder", "parent@test.com").await;
    let (_impostor_id, _, impostor_key) =
        create_agent_with_key(&server.app, "impostor", "impostor@test.com").await;
    let (sub_id, _, _sub_key) = create_agent_with_key(&server.app, "sub", "sub@test.com").await;

    // Grant addressed to `holder`; the impostor tries to re-delegate it.
    let grant = principal_grant(&server.app, &holder_id, &["calendar:read"]).await;

    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &impostor_key,
        Some(json!({
            "parent_grant_token": grant,
            "sub_agent_id": sub_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 300
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body["error"].as_str().unwrap().contains("invalid token"),
        "expected invalid-token error, got: {}",
        body
    );
}

#[tokio::test]
async fn delegation_chain_validates_and_rebinds_audience_at_each_hop() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let (a_id, _, a_key) = create_agent_with_key(&server.app, "a", "a@test.com").await;
    let (b_id, _, b_key) = create_agent_with_key(&server.app, "b", "b@test.com").await;
    let (c_id, _, _c_key) = create_agent_with_key(&server.app, "c", "c@test.com").await;

    // Hop 0 → 1: grant addressed to a is used by a.
    let hop1 = principal_grant(&server.app, &a_id, &["calendar:read"]).await;
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &a_key,
        Some(json!({
            "parent_grant_token": hop1,
            "sub_agent_id": b_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The child grant must be addressed to its recipient.
    let hop2 = body["delegated_token"].as_str().unwrap();
    let claims = server
        .state
        .token_verifier
        .verify(hop2, Some(&format!("agent:{b_id}")))
        .unwrap();
    assert_eq!(claims.act.delegation_depth, 1);

    // Hop 1 → 2: b presents a token bound to b — accepted.
    let (status, body) = send_agent_request(
        &server.app,
        "POST",
        "/v1/agent/delegate",
        &b_key,
        Some(json!({
            "parent_grant_token": hop2,
            "sub_agent_id": c_id,
            "scopes": ["calendar:read"],
            "expires_in_seconds": 300
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let hop3 = body["delegated_token"].as_str().unwrap();
    let claims = server
        .state
        .token_verifier
        .verify(hop3, Some(&format!("agent:{c_id}")))
        .unwrap();
    assert_eq!(claims.act.delegation_depth, 2);
}
