use serde::{Deserialize, Serialize};

use crate::errors::{PatroclusError, Result};

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

        let mut params = vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), authorization_code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("client_id".to_string(), provider.client_id.clone()),
            ("client_secret".to_string(), provider.client_secret.clone()),
            ("code_verifier".to_string(), code_verifier.to_string()),
        ];

        let resp = client
            .post(&format!("{}/token", provider.issuer))
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
            .get(&format!("{}/userinfo", provider.issuer))
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
        }
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
                policy_yaml: "- name: eng-access\n  decision: allow\n  reason: Engineering access".to_string(),
                scopes: vec!["db:read".to_string(), "api:call".to_string()],
                max_spend: Some(100.0),
            },
            GroupPolicyMapping {
                group: "finance".to_string(),
                policy_yaml: "- name: fin-access\n  decision: allow\n  reason: Finance access".to_string(),
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
