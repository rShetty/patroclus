use serde::{Deserialize, Serialize};

use crate::idp::GroupPolicyMapping;

/// Environment variable prefix recognized by the config layer.
pub const ENV_PREFIX: &str = "PATROCLUS_";
/// Separator for nested configuration keys (`PATROCLUS_SERVER__PORT`).
pub const ENV_NEST_SEPARATOR: &str = "__";

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
    /// Client secret. Inject via environment
    /// (`PATROCLUS_IDP__PROVIDERS__<NAME>__CLIENT_SECRET`) rather than a file
    /// in production.
    pub client_secret: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub group_claim: String,
    /// Group → policy mappings applied when a user authenticates through this
    /// provider. Matched groups are combined into the session policy.
    #[serde(default)]
    pub group_policy_mappings: Vec<GroupPolicyMapping>,
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
        let config: Config = toml::from_str(&content)?;
        config.layer_with_env()
    }

    /// Apply environment-variable overrides on top of the loaded file values
    /// (file < env layering).
    ///
    /// Any variable named `PATROCLUS_<SECTION>__<FIELD>` (double underscore as
    /// the nesting separator, e.g. `PATROCLUS_SERVER__PORT=9000`) overrides the
    /// matching field of this config. Values are parsed as TOML scalars so
    /// numbers, booleans and arrays work naturally; strings that do not parse
    /// as another type are taken verbatim. Unknown variables and fields are
    /// ignored (with a warning) so unrelated `PATROCLUS_*` consumers cannot
    /// break startup.
    ///
    /// This is the supported channel for secrets in container deployments:
    /// e.g. `PATROCLUS_TOKEN__PRIVATE_KEY_PATH` or an IdP client secret can be
    /// injected without baking them into the config file.
    pub fn layer_with_env(mut self) -> anyhow::Result<Self> {
        use std::collections::BTreeMap;

        // Collect PATROCLUS_* variables grouped by section so each section is
        // deserialized once.
        let mut by_section: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let prefix_len = ENV_PREFIX.len();

        for (name, value) in std::env::vars() {
            if !name.starts_with(ENV_PREFIX) || name.len() <= prefix_len {
                continue;
            }
            let remainder = &name[prefix_len..];
            let Some((section_raw, key)) = remainder.split_once(ENV_NEST_SEPARATOR) else {
                // Flat names like PATROCLUS_LOG_FORMAT belong to other
                // subsystems, not the layered config.
                continue;
            };
            let section = section_raw.to_ascii_lowercase();
            let field = key.to_ascii_lowercase();
            by_section.entry(section).or_default().insert(field, value);
        }

        for (section, fields) in by_section {
            match section.as_str() {
                "server" => {
                    self.server = patch_section(&self.server, &fields, "server")?;
                }
                "database" => {
                    self.database = patch_section(&self.database, &fields, "database")?;
                }
                "token" => {
                    self.token = patch_section(&self.token, &fields, "token")?;
                }
                "policy" => {
                    self.policy = patch_section(&self.policy, &fields, "policy")?;
                }
                "vault" => {
                    self.vault = patch_section(&self.vault, &fields, "vault")?;
                }
                other => {
                    tracing::warn!(
                        "ignoring env config for unknown section '{other}' \
                         (known sections: server, database, token, policy, vault)"
                    );
                }
            }
        }

        Ok(self)
    }
}

/// Rebuild a config section with environment overrides applied.
///
/// Serializes the current section to TOML, applies each `PATROCLUS_*` scalar,
/// then deserializes back into the strongly-typed section struct. Parse errors
/// fail loudly rather than silently dropping an operator override.
fn patch_section<T: Serialize + serde::de::DeserializeOwned>(
    current: &T,
    fields: &std::collections::BTreeMap<String, String>,
    section_name: &str,
) -> anyhow::Result<T> {
    let base = toml::Value::try_from(current)?;
    let table = match base {
        toml::Value::Table(t) => t,
        _ => anyhow::bail!("config section {section_name} is not a table"),
    };

    let mut table = table;
    for (field, raw_value) in fields {
        let parsed = parse_env_scalar(raw_value)?;
        match table.get(field) {
            Some(_) => {
                table.insert(field.clone(), parsed);
                tracing::info!(
                    "config override from environment: {}{}{}",
                    ENV_PREFIX,
                    section_name.to_ascii_uppercase(),
                    format!("{ENV_NEST_SEPARATOR}{field}").to_ascii_uppercase()
                );
            }
            None => {
                tracing::warn!(
                    "ignoring unknown env config field {ENV_PREFIX}{}{ENV_NEST_SEPARATOR}{field}",
                    section_name.to_ascii_uppercase()
                );
            }
        }
    }

    Ok(table.try_into()?)
}

/// Parse an environment string as a TOML scalar. Falls back to treating it as
/// a literal string when it does not parse as a number or boolean. Values in
/// TOML array syntax (e.g. `["a","b"]`) are parsed as arrays.
fn parse_env_scalar(raw: &str) -> anyhow::Result<toml::Value> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        // Array values arrive as TOML syntax; parse strictly and surface
        // syntax errors instead of storing a string.
        return Ok(
            toml::from_str::<toml::Value>(&format!("value = {trimmed}"))?
                .get("value")
                .cloned()
                .unwrap_or(toml::Value::String(raw.to_string())),
        );
    }
    if let Ok(v) = trimmed.parse::<i64>() {
        return Ok(toml::Value::Integer(v));
    }
    if let Ok(v) = trimmed.parse::<f64>() {
        return Ok(toml::Value::Float(v));
    }
    if let Ok(v) = trimmed.parse::<bool>() {
        return Ok(toml::Value::Boolean(v));
    }
    Ok(toml::Value::String(raw.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The process environment is global state: every test that touches it
    /// must hold this lock for its whole body.
    static ENV_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    fn set_env(key: &str, value: &str) {
        // SAFETY: access is serialized by ENV_LOCK and no other thread in the
        // test process reads these variables concurrently.
        unsafe { std::env::set_var(key, value) };
    }

    fn unset_env(key: &str) {
        // SAFETY: see set_env.
        unsafe { std::env::remove_var(key) };
    }

    /// Write a small config file (the "file layer") into a temp dir and load
    /// it under caller-set environment variables.
    struct FileLayer {
        path: std::path::PathBuf,
        _dir: tempfile::TempDir,
    }

    impl FileLayer {
        fn write(toml_body: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.toml");
            std::fs::write(&path, toml_body).unwrap();
            FileLayer { path, _dir: dir }
        }

        fn load(&self) -> anyhow::Result<Config> {
            Config::load(&self.path)
        }
    }

    const FILE_TOML: &str = r#"
[server]
host = "127.0.0.1"
port = 8484

[database]
path = "file.db"

[token]
issuer = "http://localhost:8484"
private_key_path = "keys/private.pem"
public_key_path = "keys/public.pem"
default_ttl_seconds = 900
max_ttl_seconds = 3600

[policy]
engine = "yaml"
default_decision = "deny"
max_delegation_depth = 3

[vault]
encryption_key_path = "keys/vault.key"
"#;

    #[test]
    fn env_override_wins_over_file_value() {
        let _guard = ENV_LOCK.lock();
        set_env("PATROCLUS_SERVER__PORT", "9999");
        set_env("PATROCLUS_DATABASE__PATH", "/var/lib/patroclus/env-wins.db");

        let file_layer = FileLayer::write(FILE_TOML);
        let config = file_layer.load().expect("layered config loads");

        assert_eq!(config.server.port, 9999, "env must override file port");
        assert_eq!(
            config.database.path, "/var/lib/patroclus/env-wins.db",
            "env must override file db path"
        );
        // Fields without an override keep their file values.
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.token.default_ttl_seconds, 900);

        unset_env("PATROCLUS_SERVER__PORT");
        unset_env("PATROCLUS_DATABASE__PATH");
    }

    #[test]
    fn env_values_are_typed_not_strings() {
        let _guard = ENV_LOCK.lock();
        set_env("PATROCLUS_POLICY__MAX_DELEGATION_DEPTH", "7");
        set_env("PATROCLUS_TOKEN__DEFAULT_TTL_SECONDS", "60");

        let file_layer = FileLayer::write(FILE_TOML);
        let config = file_layer.load().expect("layered config loads");

        assert_eq!(config.policy.max_delegation_depth, 7);
        assert_eq!(config.token.default_ttl_seconds, 60);

        unset_env("PATROCLUS_POLICY__MAX_DELEGATION_DEPTH");
        unset_env("PATROCLUS_TOKEN__DEFAULT_TTL_SECONDS");
    }

    #[test]
    fn secret_paths_injectable_via_env() {
        let _guard = ENV_LOCK.lock();
        set_env(
            "PATROCLUS_VAULT__ENCRYPTION_KEY_PATH",
            "/run/secrets/vault.key",
        );
        set_env(
            "PATROCLUS_TOKEN__PRIVATE_KEY_PATH",
            "/run/secrets/token-signing.pem",
        );

        let file_layer = FileLayer::write(FILE_TOML);
        let config = file_layer.load().expect("layered config loads");

        assert_eq!(config.vault.encryption_key_path, "/run/secrets/vault.key");
        assert_eq!(
            config.token.private_key_path,
            "/run/secrets/token-signing.pem"
        );

        unset_env("PATROCLUS_VAULT__ENCRYPTION_KEY_PATH");
        unset_env("PATROCLUS_TOKEN__PRIVATE_KEY_PATH");
    }

    #[test]
    fn unknown_sections_fields_and_flat_names_are_ignored() {
        let _guard = ENV_LOCK.lock();
        set_env("PATROCLUS_UNKNOWN_SECTION__FIELD", "x");
        set_env("PATROCLUS_SERVER__NO_SUCH_FIELD", "y");
        set_env("PATROCLUS_LOG_FORMAT", "json"); // flat name: other subsystem
        set_env("PATROCLUS_INSECURE_DEV", "1"); // flat name: auth subsystem

        let file_layer = FileLayer::write(FILE_TOML);
        let config = file_layer
            .load()
            .expect("unknown vars do not break startup");

        // Nothing was applied.
        assert_eq!(config.server.port, 8484);
        assert_eq!(config.database.path, "file.db");

        unset_env("PATROCLUS_UNKNOWN_SECTION__FIELD");
        unset_env("PATROCLUS_SERVER__NO_SUCH_FIELD");
        unset_env("PATROCLUS_LOG_FORMAT");
        unset_env("PATROCLUS_INSECURE_DEV");
    }

    #[test]
    fn invalid_scalar_type_fails_loudly() {
        let _guard = ENV_LOCK.lock();
        set_env("PATROCLUS_SERVER__PORT", "not-a-number");

        let file_layer = FileLayer::write(FILE_TOML);
        let result = file_layer.load();

        assert!(result.is_err(), "unparsable override must fail startup");

        unset_env("PATROCLUS_SERVER__PORT");
    }

    #[test]
    fn cors_origins_list_overridable_via_env() {
        let _guard = ENV_LOCK.lock();
        // TOML array syntax accepted for list fields.
        set_env(
            "PATROCLUS_SERVER__CORS_ALLOWED_ORIGINS",
            "[\"https://a.example\"]",
        );

        let file_layer = FileLayer::write(FILE_TOML);
        let config = file_layer.load().expect("layered config loads");

        assert_eq!(
            config.server.cors_allowed_origins,
            vec!["https://a.example"]
        );

        unset_env("PATROCLUS_SERVER__CORS_ALLOWED_ORIGINS");
    }
}
