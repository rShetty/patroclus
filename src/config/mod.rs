use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub token: TokenConfig,
    pub policy: PolicyConfig,
    pub vault: VaultConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
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

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8484,
            },
            database: DatabaseConfig {
                path: "patroclus.db".to_string(),
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
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
