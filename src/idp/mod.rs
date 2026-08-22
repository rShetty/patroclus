use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{PatroclusError, Result};

/// Lifetime of a pending OIDC authorization (the `state` parameter). A login
/// must complete within this window or the transaction is rejected.
pub const PKCE_STATE_TTL_SECONDS: i64 = 600;

/// A pending OIDC authorization stored server-side, keyed by the single-use
/// `state` parameter.
#[derive(Debug, Clone)]
pub struct PkceTransaction {
    pub provider_name: String,
    /// PKCE code_verifier. Never leaves the server between authorize and
    /// callback.
    pub code_verifier: String,
    pub created_at: DateTime<Utc>,
}

/// Server-side store of pending PKCE transactions.
///
/// The authorization endpoint records `(state → code_verifier)` here; the
/// callback consumes the entry exactly once to recover the verifier for the
/// token exchange. Entries expire after [`PKCE_STATE_TTL_SECONDS`].
#[derive(Default)]
pub struct PkceStore {
    transactions: RwLock<HashMap<String, PkceTransaction>>,
}

impl PkceStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new pending transaction, pruning expired entries first so the
    /// map stays bounded.
    pub fn insert(&self, state: String, provider_name: String, code_verifier: String) {
        let mut txns = self.transactions.write();
        let now = Utc::now();
        txns.retain(|_, t| {
            now.signed_duration_since(t.created_at) < Duration::seconds(PKCE_STATE_TTL_SECONDS)
        });
        txns.insert(
            state,
            PkceTransaction {
                provider_name,
                code_verifier,
                created_at: now,
            },
        );
    }

    /// Consume a pending transaction. Returns `None` for unknown, expired, or
    /// already-used states (single-use semantics).
    pub fn consume(&self, state: &str) -> Option<PkceTransaction> {
        let mut txns = self.transactions.write();
        let txn = txns.remove(state)?;
        let age = Utc::now().signed_duration_since(txn.created_at);
        if age < Duration::seconds(PKCE_STATE_TTL_SECONDS) {
            Some(txn)
        } else {
            None
        }
    }
}

/// Generate a PKCE `code_verifier`: 32 random bytes, base64url-encoded
/// (43 characters of the RFC 7636 §4.1 unreserved alphabet).
pub fn generate_code_verifier() -> String {
    random_b64url(32)
}

/// Generate a cryptographically random OIDC `state` value.
pub fn generate_state() -> String {
    random_b64url(24)
}

fn random_b64url(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// S256 `code_challenge` for a verifier: `BASE64URL(SHA256(verifier))`
/// (RFC 7636 §4.2). Only the challenge is ever sent to the IdP.
pub fn pkce_s256_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpUserInfo {
    pub subject: String,
    pub email: String,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub issuer: String,
    pub raw_claims: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupPolicyMapping {
    pub group: String,
    pub policy_yaml: String,
    pub scopes: Vec<String>,
    pub max_spend: Option<f64>,
}

pub struct IdpFederation;

impl IdpFederation {
    pub async fn exchange_oidc_token(
        provider: &crate::config::IdpProvider,
        authorization_code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<String> {
        let client = reqwest::Client::new();

        let params = vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), authorization_code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("client_id".to_string(), provider.client_id.clone()),
            ("client_secret".to_string(), provider.client_secret.clone()),
            ("code_verifier".to_string(), code_verifier.to_string()),
        ];

        let resp = client
            .post(format!("{}/token", provider.issuer))
            .form(&params)
            .send()
            .await
            .map_err(|e| PatroclusError::Config(format!("IdP token exchange failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(PatroclusError::Config(format!(
                "IdP token exchange HTTP {}: {}",
                status, body
            )));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| PatroclusError::Config(format!("IdP response parse: {}", e)))?;

        data.get("access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| PatroclusError::Config("No access_token in IdP response".to_string()))
    }

    pub async fn fetch_userinfo(
        provider: &crate::config::IdpProvider,
        access_token: &str,
    ) -> Result<IdpUserInfo> {
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}/userinfo", provider.issuer))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| PatroclusError::Config(format!("IdP userinfo failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(PatroclusError::Config(format!(
                "IdP userinfo HTTP {}: {}",
                status, body
            )));
        }

        let claims: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| PatroclusError::Config(format!("IdP userinfo parse: {}", e)))?;

        let group_claim = &provider.group_claim;
        let groups: Vec<String> = claims
            .get(group_claim)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| g.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(IdpUserInfo {
            subject: claims
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            email: claims
                .get("email")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: claims
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            groups,
            issuer: provider.issuer.clone(),
            raw_claims: claims,
        })
    }

    pub fn build_policy_from_groups(
        user_info: &IdpUserInfo,
        mappings: &[GroupPolicyMapping],
    ) -> Option<(String, Vec<String>)> {
        let mut combined_scopes = Vec::new();
        let mut policies = Vec::new();

        for mapping in mappings {
            if user_info.groups.contains(&mapping.group) {
                policies.push(mapping.policy_yaml.clone());
                combined_scopes.extend(mapping.scopes.iter().cloned());
            }
        }

        if policies.is_empty() {
            return None;
        }

        combined_scopes.sort();
        combined_scopes.dedup();

        Some((policies.join("\n"), combined_scopes))
    }

    pub fn authorization_url(
        provider: &crate::config::IdpProvider,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String {
        let scopes = if provider.scopes.is_empty() {
            "openid email profile".to_string()
        } else {
            provider.scopes.join(" ")
        };

        format!(
            "{}/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            provider.issuer,
            urlencoding::encode(&provider.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state),
            urlencoding::encode(code_challenge),
        )
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                    c.to_string()
                } else {
                    format!("%{:02X}", c as u8)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IdpProvider;

    fn test_provider() -> IdpProvider {
        IdpProvider {
            name: "test".to_string(),
            issuer: "https://idp.example.com".to_string(),
            client_id: "client123".to_string(),
            client_secret: "secret".to_string(),
            scopes: vec!["openid".to_string(), "email".to_string()],
            group_claim: "groups".to_string(),
            group_policy_mappings: vec![],
        }
    }

    #[test]
    fn test_pkce_s256_challenge_matches_rfc7636_appendix_b() {
        // Test vector from RFC 7636 Appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_s256_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_code_verifier_shape() {
        let verifier = generate_code_verifier();
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "verifier must be base64url without padding"
        );
        // Randomness: two draws must not collide.
        assert_ne!(verifier, generate_code_verifier());
    }

    #[test]
    fn test_state_is_unique() {
        let a = generate_state();
        let b = generate_state();
        assert_ne!(a, b);
        assert!(!a.contains('='));
    }

    #[tokio::test]
    async fn test_pkce_store_roundtrip_and_single_use() {
        let store = PkceStore::new();
        store.insert(
            "state-1".to_string(),
            "prov".to_string(),
            "verifier-1".to_string(),
        );

        let txn = store.consume("state-1").expect("valid state");
        assert_eq!(txn.provider_name, "prov");
        assert_eq!(txn.code_verifier, "verifier-1");

        // Single use: second consume fails.
        assert!(store.consume("state-1").is_none());
        // Unknown state fails.
        assert!(store.consume("state-other").is_none());
    }

    #[test]
    fn test_pkce_store_expired_state_rejected() {
        let store = PkceStore::new();
        {
            let mut txns = store.transactions.write();
            txns.insert(
                "stale".to_string(),
                PkceTransaction {
                    provider_name: "prov".to_string(),
                    code_verifier: "v".to_string(),
                    created_at: Utc::now() - Duration::seconds(PKCE_STATE_TTL_SECONDS + 60),
                },
            );
        }
        // Expired entries are rejected and purged on access.
        assert!(store.consume("stale").is_none());
        assert!(store.transactions.read().is_empty());
    }

    #[test]
    fn test_authorization_url() {
        let provider = test_provider();
        let url = IdpFederation::authorization_url(
            &provider,
            "http://localhost:8484/callback",
            "state123",
            "challenge456",
        );
        assert!(url.contains("https://idp.example.com/authorize"));
        assert!(url.contains("client_id=client123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge=challenge456"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_build_policy_from_groups_match() {
        let user = IdpUserInfo {
            subject: "sub123".to_string(),
            email: "alice@example.com".to_string(),
            name: Some("Alice".to_string()),
            groups: vec!["engineering".to_string(), "admins".to_string()],
            issuer: "https://idp.example.com".to_string(),
            raw_claims: serde_json::json!({}),
        };

        let mappings = vec![
            GroupPolicyMapping {
                group: "engineering".to_string(),
                policy_yaml: "- name: eng-access\n  decision: allow\n  reason: Engineering access"
                    .to_string(),
                scopes: vec!["db:read".to_string(), "api:call".to_string()],
                max_spend: Some(100.0),
            },
            GroupPolicyMapping {
                group: "finance".to_string(),
                policy_yaml: "- name: fin-access\n  decision: allow\n  reason: Finance access"
                    .to_string(),
                scopes: vec!["billing:read".to_string()],
                max_spend: None,
            },
        ];

        let result = IdpFederation::build_policy_from_groups(&user, &mappings);
        assert!(result.is_some());
        let (policy, scopes) = result.unwrap();
        assert!(policy.contains("eng-access"));
        assert!(!policy.contains("fin-access"));
        assert!(scopes.contains(&"db:read".to_string()));
        assert!(!scopes.contains(&"billing:read".to_string()));
    }

    #[test]
    fn test_build_policy_from_groups_no_match() {
        let user = IdpUserInfo {
            subject: "sub123".to_string(),
            email: "bob@example.com".to_string(),
            name: None,
            groups: vec!["marketing".to_string()],
            issuer: "https://idp.example.com".to_string(),
            raw_claims: serde_json::json!({}),
        };

        let mappings = vec![GroupPolicyMapping {
            group: "engineering".to_string(),
            policy_yaml: "- name: eng".to_string(),
            scopes: vec![],
            max_spend: None,
        }];

        let result = IdpFederation::build_policy_from_groups(&user, &mappings);
        assert!(result.is_none());
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("abc-123"), "abc-123");
        assert_eq!(urlencoding::encode("a+b=c"), "a%2Bb%3Dc");
    }
}
