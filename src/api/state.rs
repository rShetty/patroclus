use std::sync::Arc;

use crate::config::Config;
use crate::db::Database;
use crate::policy::PolicyEngine;
use crate::token::issuer::TokenIssuer;
use crate::token::verifier::TokenVerifier;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Config>,
    pub policy_engine: Arc<dyn PolicyEngine>,
    pub token_issuer: Arc<TokenIssuer>,
    pub token_verifier: Arc<TokenVerifier>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let db = Database::new(&config.database.path)?;
        db.create_default_policy()?;

        let policy_engine = crate::policy::create_engine(&config.policy.engine)?;

        let private_key = std::fs::read_to_string(&config.token.private_key_path).unwrap_or_else(|_| {
            tracing::warn!("No private key found at {}, generating ephemeral key", config.token.private_key_path);
            String::new()
        });

        let public_key = std::fs::read_to_string(&config.token.public_key_path).unwrap_or_else(|_| {
            String::new()
        });

        let token_issuer = if private_key.is_empty() {
            tracing::warn!("Using ephemeral key for token issuance — not for production");
            Arc::new(TokenIssuer::ephemeral(&config.token.issuer)?)
        } else {
            Arc::new(TokenIssuer::from_pem(&private_key, &config.token.issuer, "key-1")?)
        };

        let token_verifier = if public_key.is_empty() {
            Arc::new(TokenVerifier::ephemeral(&config.token.issuer)?)
        } else {
            Arc::new(TokenVerifier::from_pem(&public_key, &config.token.issuer)?)
        };

        Ok(AppState {
            db: Arc::new(db),
            config: Arc::new(config),
            policy_engine: policy_engine.into(),
            token_issuer,
            token_verifier,
        })
    }
}
