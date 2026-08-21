//! Authentication for the Patroclus control plane.
//!
//! Two authentication domains exist:
//!
//! * **Admin** — human/operator surfaces (`/v1/admin/*`, `/v1/vault/*`,
//!   `/v1/sessions*`, `/v1/principal/*`). Authenticated with a static admin
//!   bearer token sourced from `PATROCLUS_ADMIN_TOKEN`.
//! * **Agent** — agent-facing decision surfaces (`/v1/agent/*`). Authenticated
//!   with a per-agent client key issued at provisioning time; only its
//!   SHA-256 hash is stored.
//!
//! `/`, `/health` and the IdP browser federation endpoints (`/v1/idp/*`) are
//! public by design.

use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::errors::ErrorKind;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::state::AppState;
use crate::errors::PatroclusError;

/// Well-known environment variable holding the admin bearer token.
pub const ADMIN_TOKEN_ENV: &str = "PATROCLUS_ADMIN_TOKEN";
/// Environment variable that permits insecure startup in release builds.
pub const INSECURE_DEV_ENV: &str = "PATROCLUS_INSECURE_DEV";
/// Header carrying an agent's client key on agent-facing routes.
pub const AGENT_KEY_HEADER: &str = "x-client-key";

/// Token material used by tests and fixtures.
pub const TEST_ADMIN_TOKEN: &str = "test-admin-token";

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub admin_token: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        AuthConfig {
            admin_token: std::env::var(ADMIN_TOKEN_ENV)
                .ok()
                .filter(|t| !t.is_empty()),
        }
    }

    /// Refuse to start a release build without an admin token unless the
    /// operator explicitly opted into insecure development mode.
    pub fn ensure_startable(&self, is_release: bool) -> Result<(), String> {
        if self.admin_token.is_some() {
            return Ok(());
        }
        if std::env::var(INSECURE_DEV_ENV).as_deref() == Ok("1") {
            tracing::warn!(
                "{} is not set but {}=1 — running WITHOUT admin authentication",
                ADMIN_TOKEN_ENV,
                INSECURE_DEV_ENV
            );
            return Ok(());
        }
        if is_release {
            Err(format!(
                "refusing to start: {ADMIN_TOKEN_ENV} is not set. Set it to a strong secret \
                 (or set {INSECURE_DEV_ENV}=1 for local development only)."
            ))
        } else {
            tracing::warn!(
                "{} is not set — admin routes are UNAUTHENTICATED (debug build)",
                ADMIN_TOKEN_ENV
            );
            Ok(())
        }
    }

    pub fn for_test() -> Self {
        AuthConfig {
            admin_token: Some(TEST_ADMIN_TOKEN.to_string()),
        }
    }
}

/// Identity established for an authenticated agent request.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedAgent {
    pub agent_id: Uuid,
}

/// SHA-256 hex digest used for client-key storage and comparison.
pub fn hash_client_key(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Generate a new agent client key. Returns `(raw_key, stored_hash)` —
/// the raw key is shown once at provisioning time.
pub fn generate_client_key() -> (String, String) {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    let raw = format!("pat_{hex}");
    let hash = hash_client_key(&raw);
    (raw, hash)
}

/// Constant-time equality over byte slices of equal length semantics.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn paths_start_with(path: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
}

fn is_admin_path(path: &str) -> bool {
    paths_start_with(
        path,
        &["/v1/admin", "/v1/vault", "/v1/sessions", "/v1/principal"],
    )
}

fn is_agent_path(path: &str) -> bool {
    paths_start_with(path, &["/v1/agent"])
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

/// Axum middleware enforcing the two authentication domains.
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, Response> {
    let path = req.uri().path().to_string();

    if is_admin_path(&path) {
        let expected = state.auth.admin_token.as_deref();
        let Some(expected) = expected else {
            // Debug builds without a token run unauthenticated (warned at startup).
            return Ok(next.run(req).await);
        };
        let presented = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        let ok = constant_time_eq(presented.as_bytes(), expected.as_bytes());
        if !ok {
            return Err(unauthorized("missing or invalid admin token"));
        }
        return Ok(next.run(req).await);
    }

    if is_agent_path(&path) {
        let raw_key = req
            .headers()
            .get(AGENT_KEY_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if raw_key.is_empty() {
            return Err(unauthorized("missing agent client key"));
        }
        let agent = state
            .db
            .get_agent_by_client_key_hash(&hash_client_key(raw_key))
            .await;
        match agent {
            Ok(Some(agent)) => {
                let mut req = req;
                req.extensions_mut()
                    .insert(AuthenticatedAgent { agent_id: agent.id });
                return Ok(next.run(req).await);
            }
            _ => return Err(unauthorized("invalid agent client key")),
        }
    }

    Ok(next.run(req).await)
}

/// Extract the authenticated agent identity inserted by the middleware.
pub fn authenticated_agent(
    extensions: &axum::http::Extensions,
) -> Result<AuthenticatedAgent, PatroclusError> {
    extensions
        .get::<AuthenticatedAgent>()
        .copied()
        .ok_or_else(|| {
            PatroclusError::Forbidden("request was not authenticated as an agent".to_string())
        })
}

/// Verify a token's signature/expiry, mapping errors to unauthorized.
pub fn verify_bearer_or_unauthorized(err: &jsonwebtoken::errors::Error) -> StatusCode {
    match err.kind() {
        ErrorKind::ExpiredSignature => StatusCode::UNAUTHORIZED,
        _ => StatusCode::UNAUTHORIZED,
    }
}
