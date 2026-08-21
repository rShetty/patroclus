#![allow(dead_code)]
use axum::Router;
use patroclus::api::server::create_router;
use patroclus::api::state::AppState;
pub struct TestServer {
    pub app: Router,
    pub state: AppState,
}

impl TestServer {
    pub fn new() -> anyhow::Result<Self> {
        let state = AppState::new_test()?;
        let app = create_router(state.clone());
        Ok(TestServer { app, state })
    }

    pub fn new_with_policy(yaml: &str) -> anyhow::Result<Self> {
        let state = AppState::new_test()?;
        state.db.create_policy("test", "yaml", yaml)?;
        state.reload_policy()?;
        let app = create_router(state.clone());
        Ok(TestServer { app, state })
    }
}

/// A simulated agent that interacts with Patroclus infrastructure.
/// This is the test harness that exercises the full end-to-end flow.
pub struct AgentHarness {
    pub agent_id: String,
    pub principal_id: String,
    pub session_id: String,
    pub client_key: Option<String>,
    pub delegation_token: Option<String>,
    pub access_token: Option<String>,
    pub token_jti: Option<String>,
    pub actions_taken: Vec<String>,
}

impl AgentHarness {
    pub fn new(agent_id: &str, principal_id: &str) -> Self {
        AgentHarness {
            agent_id: agent_id.to_string(),
            principal_id: principal_id.to_string(),
            session_id: format!("session-{}", uuid::Uuid::now_v7()),
            client_key: None,
            delegation_token: None,
            access_token: None,
            token_jti: None,
            actions_taken: Vec::new(),
        }
    }

    pub fn with_client_key(mut self, key: &str) -> Self {
        self.client_key = Some(key.to_string());
        self
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = session_id.to_string();
        self
    }

    pub fn with_delegation_token(mut self, token: &str) -> Self {
        self.delegation_token = Some(token.to_string());
        self
    }

    /// Request access to a resource. Returns (decision, reason, token_jwt_or_none).
    pub async fn request_access(
        &mut self,
        app: &Router,
        action: &str,
        resource: &str,
        scopes: &[&str],
    ) -> (String, String, Option<String>) {
        let scope_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        let mut body = json!({
            "agent_id": self.agent_id,
            "action": action,
            "resource": resource,
            "requested_scopes": scope_vec,
            "context": {
                "session_id": self.session_id,
            }
        });
        if let Some(token) = &self.delegation_token {
            body["delegation_token"] = json!(token);
        }

        let key = self
            .client_key
            .clone()
            .expect("agent harness requires a client key");
        let (_status, resp) =
            send_agent_request(app, "POST", "/v1/agent/request-access", &key, Some(body)).await;
        self.actions_taken.push(format!("{}:{}", action, resource));

        let decision = resp["decision"].as_str().unwrap_or("error").to_string();
        let reason = resp["reason"].as_str().unwrap_or("").to_string();
        let token = resp["token"]["jwt"].as_str().map(|s| s.to_string());

        if let Some(t) = &token {
            self.access_token = Some(t.clone());
            self.token_jti = resp["token"]["jti"].as_str().map(|s| s.to_string());
        }

        (decision, reason, token)
    }

    /// Dry-run check without issuing a token.
    pub async fn check_access(
        &self,
        app: &Router,
        action: &str,
        resource: &str,
        scopes: &[&str],
    ) -> (bool, String) {
        let scope_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        let body = json!({
            "agent_id": self.agent_id,
            "action": action,
            "resource": resource,
            "requested_scopes": scope_vec,
            "context": {
                "session_id": self.session_id,
            }
        });

        let key = self
            .client_key
            .clone()
            .expect("agent harness requires a client key");
        let (_, resp) = send_agent_request(app, "POST", "/v1/agent/check", &key, Some(body)).await;
        let allowed = resp["allowed"].as_bool().unwrap_or(false);
        let reason = resp["reason"].as_str().unwrap_or("").to_string();
        (allowed, reason)
    }

    /// Delegate permissions to a sub-agent.
    pub async fn delegate_to(
        &self,
        app: &Router,
        sub_agent_id: &str,
        scopes: &[&str],
        expires_in_seconds: u64,
    ) -> Option<String> {
        let scope_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        // Prefer delegation_token for sub-delegation since access_token
        // from request_access has a different audience
        let token = self
            .delegation_token
            .as_ref()
            .or(self.access_token.as_ref())?;
        let body = json!({
            "parent_grant_token": token,
            "sub_agent_id": sub_agent_id,
            "scopes": scope_vec,
            "expires_in_seconds": expires_in_seconds,
        });

        let key = self
            .client_key
            .clone()
            .expect("agent harness requires a client key");
        let (status, resp) =
            send_agent_request(app, "POST", "/v1/agent/delegate", &key, Some(body)).await;
        if status == StatusCode::OK {
            resp["delegated_token"].as_str().map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Record spend for budget tracking.
    pub async fn record_spend(&self, app: &Router, amount: f64) {
        let body = json!({
            "amount": amount,
            "session_id": self.session_id,
        });
        let _ = send_request(
            app,
            "POST",
            &format!("/v1/admin/agents/{}/spend", self.agent_id),
            Some(body),
        )
        .await;
    }

    /// Get all audit entries (for verification).
    pub async fn get_audit(app: &Router) -> Vec<serde_json::Value> {
        let (_, resp) = send_request(app, "GET", "/v1/admin/audit", None).await;
        resp.as_array().cloned().unwrap_or_default()
    }

    /// Get session list.
    pub async fn get_sessions(app: &Router) -> Vec<serde_json::Value> {
        let (_, resp) = send_request(app, "GET", "/v1/sessions", None).await;
        resp["sessions"].as_array().cloned().unwrap_or_default()
    }
}

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Admin token configured by [`patroclus::api::auth::AuthConfig::for_test`],
/// which `AppState::new_test` installs.
pub const TEST_ADMIN_TOKEN: &str = "test-admin-token";

fn is_privileged_path(uri: &str) -> bool {
    ["/v1/admin", "/v1/vault", "/v1/sessions", "/v1/principal"]
        .iter()
        .any(|p| uri == *p || uri.starts_with(&format!("{p}/")))
}

pub async fn send_request(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if is_privileged_path(uri) {
        builder = builder.header("Authorization", format!("Bearer {TEST_ADMIN_TOKEN}"));
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

/// Send a request to an agent-facing route with the agent's client key.
pub async fn send_agent_request(
    app: &Router,
    method: &str,
    uri: &str,
    client_key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-client-key", client_key);
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

pub async fn create_agent_and_principal(
    app: &Router,
    agent_name: &str,
    principal_email: &str,
) -> (String, String) {
    let (_, principal) = send_request(
        app,
        "POST",
        "/v1/admin/principals",
        Some(json!({
            "external_id": principal_email,
            "idp_provider": "local",
            "email": principal_email,
            "display_name": principal_email
        })),
    )
    .await;
    let principal_id = principal["id"].as_str().unwrap().to_string();

    let (_, agent) = send_request(
        app,
        "POST",
        "/v1/admin/agents",
        Some(json!({
            "name": agent_name,
            "principal_type": "delegated",
            "owner_id": principal_id
        })),
    )
    .await;
    let agent_id = agent["id"].as_str().unwrap().to_string();

    (agent_id, principal_id)
}

/// Create an agent + principal and provision its client key.
/// Returns `(agent_id, principal_id, client_key)`.
pub async fn create_agent_with_key(
    app: &Router,
    agent_name: &str,
    principal_email: &str,
) -> (String, String, String) {
    let (agent_id, principal_id) =
        create_agent_and_principal(app, agent_name, principal_email).await;
    let (status, resp) = send_request(
        app,
        "POST",
        &format!("/v1/admin/agents/{agent_id}/client-key"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "client-key provisioning failed: {resp}"
    );
    let client_key = resp["client_key"]
        .as_str()
        .expect("client_key in provisioning response")
        .to_string();
    (agent_id, principal_id, client_key)
}
