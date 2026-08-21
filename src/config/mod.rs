use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub token: TokenConfig,
    pub policy: PolicyConfig,
    pub vault: VaultConfig,
    #[serde(default)]
    pub idp: IdpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins. Empty list = no cross-origin browser access.
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    /// Size of the optional read connection pool (r2d2_sqlite). `0` (the
    /// default) disables pooling and serves reads from the shared write
    /// connection. Only meaningful for file-backed databases.
    #[serde(default)]
    pub read_pool_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub issuer: String,
    pub private_key_path: String,
    pub public_key_path: String,
    pub default_ttl_seconds: u64,
    pub max_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub engine: String,
    pub default_decision: String,
    pub max_delegation_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub encryption_key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub providers: Vec<IdpProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpProvider {
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub group_claim: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8484,
                cors_allowed_origins: vec![],
            },
            database: DatabaseConfig {
                path: "patroclus.db".to_string(),
                read_pool_size: 0,
            },
            token: TokenConfig {
                issuer: "http://localhost:8484".to_string(),
                private_key_path: "keys/private.pem".to_string(),
                public_key_path: "keys/public.pem".to_string(),
                default_ttl_seconds: 900,
                max_ttl_seconds: 3600,
            },
            policy: PolicyConfig {
                engine: "yaml".to_string(),
                default_decision: "deny".to_string(),
                max_delegation_depth: 3,
            },
            vault: VaultConfig {
                encryption_key_path: "keys/vault.key".to_string(),
            },
            idp: IdpConfig::default(),
        }
    }
}

impl Config {
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
