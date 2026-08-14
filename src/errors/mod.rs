use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatroclusError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    #[error("principal not found: {0}")]
    PrincipalNotFound(String),

    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    #[error("policy denied: {reason}")]
    PolicyDenied { reason: String },

    #[error("approval required: {reason}")]
    ApprovalRequired { reason: String },

    #[error("invalid token: {0}")]
    InvalidToken(String),

    #[error("expired token")]
    ExpiredToken,

    #[error("revoked token: {0}")]
    RevokedToken(String),

    #[error("scope escalation attempted: requested {requested} exceeds parent {parent}")]
    ScopeEscalation { requested: String, parent: String },

    #[error("delegation depth exceeded: max {max}, got {actual}")]
    DelegationDepthExceeded { max: usize, actual: usize },

    #[error("vault error: {0}")]
    Vault(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

pub type Result<T> = std::result::Result<T, PatroclusError>;

impl From<rusqlite::Error> for PatroclusError {
    fn from(e: rusqlite::Error) -> Self {
        PatroclusError::Database(e.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for PatroclusError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        PatroclusError::Crypto(e.to_string())
    }
}

impl From<std::io::Error> for PatroclusError {
    fn from(e: std::io::Error) -> Self {
        PatroclusError::Config(e.to_string())
    }
}

impl IntoResponse for PatroclusError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            PatroclusError::AgentNotFound(_)
            | PatroclusError::PrincipalNotFound(_)
            | PatroclusError::ResourceNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            PatroclusError::PolicyDenied { .. } => (StatusCode::FORBIDDEN, self.to_string()),
            PatroclusError::ApprovalRequired { .. } => (StatusCode::FORBIDDEN, self.to_string()),
            PatroclusError::InvalidToken(_)
            | PatroclusError::ExpiredToken
            | PatroclusError::RevokedToken(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            PatroclusError::ScopeEscalation { .. }
            | PatroclusError::DelegationDepthExceeded { .. } => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
