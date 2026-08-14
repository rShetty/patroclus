use std::sync::Arc;

use crate::config::Config;
use crate::crypto::KeyPair;
use crate::db::Database;
use crate::policy::PolicyEngine;
use crate::token::issuer::TokenIssuer;
use crate::token::verifier::TokenVerifier;
use crate::vault::Vault;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Config>,
    pub policy_engine: Arc<dyn PolicyEngine>,
    pub token_issuer: Arc<TokenIssuer>,
    pub token_verifier: Arc<TokenVerifier>,
    pub vault: Option<Arc<Vault>>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let db = Database::new(&config.database.path)?;
        db.create_default_policy()?;

        let policy_yaml = db.load_active_policy_yaml()?;
        let policy_yaml_ref = if policy_yaml.is_empty() { None } else { Some(policy_yaml.as_str()) };
        let policy_engine: Arc<dyn PolicyEngine> = Arc::from(
            crate::policy::create_engine(&config.policy.engine, policy_yaml_ref)?
        );

        let keypair = KeyPair::load_or_generate(
            &config.token.private_key_path,
            &config.token.public_key_path,
        )?;

        let token_issuer = Arc::new(TokenIssuer::from_pem(
            &keypair.private_pem,
            &config.token.issuer,
            "key-1",
        )?);

        let token_verifier = Arc::new(TokenVerifier::from_pem(
            &keypair.public_pem,
            &config.token.issuer,
        )?);

        let vault = match Vault::from_file(&config.vault.encryption_key_path) {
            Ok(v) => {
                tracing::info!("Vault initialized with key from {}", config.vault.encryption_key_path);
                Some(Arc::new(v))
            }
            Err(_) => {
                tracing::warn!(
                    "No vault key found at {} — credential vault disabled",
                    config.vault.encryption_key_path
                );
                None
            }
        };

        Ok(AppState {
            db: Arc::new(db),
            config: Arc::new(config),
            policy_engine,
            token_issuer,
            token_verifier,
            vault,
        })
    }

    pub fn new_test() -> anyhow::Result<Self> {
        let config = Config::default();
        let db = Database::new(":memory:")?;
        db.create_default_policy()?;

        let policy_yaml = db.load_active_policy_yaml()?;
        let policy_yaml_ref = if policy_yaml.is_empty() { None } else { Some(policy_yaml.as_str()) };
        let policy_engine: Arc<dyn PolicyEngine> = Arc::from(
            crate::policy::create_engine(&config.policy.engine, policy_yaml_ref)?
        );

        let keypair = KeyPair::generate()?;

        let token_issuer = Arc::new(TokenIssuer::from_pem(
            &keypair.private_pem,
            &config.token.issuer,
            "test-key",
        )?);

        let token_verifier = Arc::new(TokenVerifier::from_pem(
            &keypair.public_pem,
            &config.token.issuer,
        )?);

        let vault = Arc::new(Vault::new(b"test-vault-key-material")?);

        Ok(AppState {
            db: Arc::new(db),
            config: Arc::new(config),
            policy_engine,
            token_issuer,
            token_verifier,
            vault: Some(vault),
        })
    }
}
