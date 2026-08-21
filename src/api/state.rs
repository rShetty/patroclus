use std::sync::Arc;

use parking_lot::RwLock;

use crate::api::auth::AuthConfig;
use crate::config::Config;
use crate::crypto::KeyPair;
use crate::db::Database;
use crate::policy::PolicyEngine;
use crate::session::SessionStore;
use crate::token::issuer::TokenIssuer;
use crate::token::verifier::TokenVerifier;
use crate::vault::Vault;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Config>,
    pub auth: Arc<AuthConfig>,
    pub policy_engine: Arc<RwLock<Arc<dyn PolicyEngine>>>,
    pub token_issuer: Arc<TokenIssuer>,
    pub token_verifier: Arc<TokenVerifier>,
    pub vault: Option<Arc<Vault>>,
    pub session_store: Arc<SessionStore>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let db = Database::with_config(&config.database)?;
        db.create_default_policy().await?;

        let session_store = Arc::new(SessionStore::new());

        let policy_engine = Self::build_engine_async(&db, &session_store, &config).await?;

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
                tracing::info!(
                    "Vault initialized with key from {}",
                    config.vault.encryption_key_path
                );
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
            auth: Arc::new(AuthConfig::from_env()),
            policy_engine: Arc::new(RwLock::new(policy_engine)),
            token_issuer,
            token_verifier,
            vault,
            session_store,
        })
    }

    pub async fn new_test() -> anyhow::Result<Self> {
        let config = Config::default();
        let db = Database::new(":memory:")?;
        db.create_default_policy().await?;

        let session_store = Arc::new(SessionStore::new());

        let policy_engine = Self::build_engine_async(&db, &session_store, &config).await?;

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
            auth: Arc::new(AuthConfig::for_test()),
            policy_engine: Arc::new(RwLock::new(policy_engine)),
            token_issuer,
            token_verifier,
            vault: Some(vault),
            session_store,
        })
    }

    pub async fn reload_policy(&self) -> anyhow::Result<()> {
        let engine = Self::build_engine_async(&self.db, &self.session_store, &self.config).await?;
        let mut guard = self.policy_engine.write();
        *guard = engine;
        tracing::info!("Policy engine reloaded");
        Ok(())
    }

    /// Async variant of [`Self::build_engine`] that reads the active policy
    /// through the blocking-pool database layer.
    async fn build_engine_async(
        db: &Database,
        session_store: &Arc<SessionStore>,
        config: &Config,
    ) -> anyhow::Result<Arc<dyn PolicyEngine>> {
        let policy_yaml = db.load_active_policy_yaml().await?;
        let yaml_ref = if policy_yaml.is_empty() {
            None
        } else {
            Some(policy_yaml.as_str())
        };
        let engine = crate::policy::create_engine_with_sessions(
            &config.policy.engine,
            yaml_ref,
            session_store.clone(),
        )?;
        Ok(Arc::from(engine))
    }

    pub fn eval_engine(
        &self,
        ctx: &crate::policy::PolicyContext,
    ) -> crate::errors::Result<crate::policy::PolicyEvaluation> {
        let guard = self.policy_engine.read();
        guard.evaluate(ctx)
    }
}
