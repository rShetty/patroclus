//! JWKS endpoint (`GET /.well-known/jwks.json`).
//!
//! Resource servers must be able to fetch the issuer's public keys and
//! validate issued tokens offline using only the JWKS document.

mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use harness::create_agent_with_key;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use patroclus::token::AgentClaims;
use serde_json::{Value, json};
use tower::ServiceExt;

const ALLOW_POLICY: &str = r#"
- name: allow-reads
  actions: ["read"]
  resources: ["*"]
  scopes: ["*"]
  decision: allow
  reason: Read access permitted by policy
"#;

/// Issue an access token through the normal request-access flow.
async fn issue_access_token(server: &harness::TestServer) -> String {
    let (agent_id, _, agent_key) =
        create_agent_with_key(&server.app, "agent", "jwks@test.com").await;
    let (_, body) = harness::send_agent_request(
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
    assert_eq!(body["decision"], "allow", "token issuance failed: {}", body);
    body["token"]["jwt"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn jwks_is_public_and_cache_friendly() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();

    let response = server
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/.well-known/jwks.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cache_control = response
        .headers()
        .get("cache-control")
        .expect("Cache-Control header")
        .to_str()
        .unwrap();
    assert!(
        cache_control.contains("public") && cache_control.contains("max-age"),
        "expected cache-friendly Cache-Control, got: {}",
        cache_control
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let keys = body["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["kty"], "RSA");
    assert_eq!(keys[0]["alg"], "RS256");
    assert_eq!(keys[0]["kid"], "test-key");
}

#[tokio::test]
async fn fetched_jwks_validates_an_issued_token() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let jwt = issue_access_token(&server).await;

    // Fetch the JWKS exactly as a resource server would.
    let (_, body) = harness::send_request(&server.app, "GET", "/.well-known/jwks.json", None).await;
    let jwks: JwkSet = serde_json::from_value(body).unwrap();

    // Select the key named by the token's header kid.
    let header = decode_header(&jwt).expect("decodable JWT header");
    let kid = header.kid.expect("issued tokens carry a kid");
    let jwk = jwks
        .find(&kid)
        .expect("JWKS must contain the signing key referenced by the token");

    // Validate the token against the fetched JWK alone.
    let decoding_key = DecodingKey::from_jwk(jwk).expect("usable JWK");
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&server.state.config.token.issuer]);
    validation.set_audience(&["dev-db"]);
    let token_data = decode::<AgentClaims>(&jwt, &decoding_key, &validation)
        .expect("issued token validates against JWKS key");

    assert!(!token_data.claims.sub.is_empty());
    assert!(token_data.claims.scope.contains("db:read"));
    assert_eq!(token_data.claims.aud, "dev-db");
}

#[tokio::test]
async fn tampered_tokens_fail_against_jwks_key() {
    let server = harness::TestServer::new_with_policy(ALLOW_POLICY).unwrap();
    let jwt = issue_access_token(&server).await;

    let (_, body) = harness::send_request(&server.app, "GET", "/.well-known/jwks.json", None).await;
    let jwks: JwkSet = serde_json::from_value(body).unwrap();
    let header = decode_header(&jwt).unwrap();
    let jwk = jwks.find(header.kid.as_deref().unwrap()).unwrap();
    let decoding_key = DecodingKey::from_jwk(jwk).unwrap();

    // Corrupt the signature segment.
    let mut parts: Vec<&str> = jwt.split('.').collect();
    let last = parts.len() - 1;
    let flipped = if parts[last].starts_with('A') {
        format!("B{}", &parts[last][1..])
    } else {
        format!("A{}", &parts[last][1..])
    };
    parts[last] = Box::leak(flipped.into_boxed_str());
    let tampered = parts.join(".");

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&server.state.config.token.issuer]);
    validation.set_audience(&["dev-db"]);
    assert!(
        decode::<AgentClaims>(&tampered, &decoding_key, &validation).is_err(),
        "tampered token must not validate"
    );
}
