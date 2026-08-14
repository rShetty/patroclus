use serde::{Deserialize, Serialize};

use crate::errors::{PatroclusError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: Option<String>,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub default_scopes: Vec<String>,
}

pub trait TokenExchangeProvider: Send + Sync {
    fn name(&self) -> &str;
    fn exchange_refresh(
        &self,
        refresh_token: &str,
        requested_scopes: &[String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderTokenResponse>> + Send>>;
}

pub struct GitHubProvider {
    config: ProviderConfig,
}

impl GitHubProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        GitHubProvider {
            config: ProviderConfig {
                name: "github".to_string(),
                token_url: "https://github.com/login/oauth/access_token".to_string(),
                client_id,
                client_secret,
                default_scopes: vec!["repo".to_string(), "read:user".to_string()],
            },
        }
    }
}

impl TokenExchangeProvider for GitHubProvider {
    fn name(&self) -> &str {
        "github"
    }

    fn exchange_refresh(
        &self,
        refresh_token: &str,
        requested_scopes: &[String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderTokenResponse>> + Send>>
    {
        let config = self.config.clone();
        let token = refresh_token.to_string();
        let scopes = requested_scopes.to_vec();

        Box::pin(async move {
            let client = reqwest::Client::new();
            let mut params = vec![
                ("client_id".to_string(), config.client_id.clone()),
                ("client_secret".to_string(), config.client_secret.clone()),
                ("refresh_token".to_string(), token.clone()),
                ("grant_type".to_string(), "refresh_token".to_string()),
            ];
            if !scopes.is_empty() {
                params.push(("scope".to_string(), scopes.join(" ")));
            }

            let resp = client
                .post(&config.token_url)
                .header("Accept", "application/json")
                .form(&params)
                .send()
                .await
                .map_err(|e| {
                    PatroclusError::Vault(format!("GitHub token exchange failed: {}", e))
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(PatroclusError::Vault(format!(
                    "GitHub token exchange HTTP {}: {}",
                    status, body
                )));
            }

            let data: serde_json::Value = resp.json().await.map_err(|e| {
                PatroclusError::Vault(format!("GitHub response parse error: {}", e))
            })?;

            if let Some(err) = data.get("error") {
                return Err(PatroclusError::Vault(format!(
                    "GitHub token exchange error: {}",
                    err.as_str().unwrap_or("unknown")
                )));
            }

            Ok(ProviderTokenResponse {
                access_token: data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                token_type: data
                    .get("token_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bearer")
                    .to_string(),
                scope: data
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                expires_in: data.get("expires_in").and_then(|v| v.as_u64()),
                refresh_token: data
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
    }
}

pub struct GoogleProvider {
    config: ProviderConfig,
}

impl GoogleProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        GoogleProvider {
            config: ProviderConfig {
                name: "google".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                client_id,
                client_secret,
                default_scopes: vec!["https://www.googleapis.com/auth/userinfo.email".to_string()],
            },
        }
    }
}

impl TokenExchangeProvider for GoogleProvider {
    fn name(&self) -> &str {
        "google"
    }

    fn exchange_refresh(
        &self,
        refresh_token: &str,
        requested_scopes: &[String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderTokenResponse>> + Send>>
    {
        let config = self.config.clone();
        let token = refresh_token.to_string();
        let scopes = requested_scopes.to_vec();

        Box::pin(async move {
            let client = reqwest::Client::new();
            let mut params = vec![
                ("client_id".to_string(), config.client_id.clone()),
                ("client_secret".to_string(), config.client_secret.clone()),
                ("refresh_token".to_string(), token.clone()),
                ("grant_type".to_string(), "refresh_token".to_string()),
            ];
            if !scopes.is_empty() {
                params.push(("scope".to_string(), scopes.join(" ")));
            }

            let resp = client
                .post(&config.token_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| {
                    PatroclusError::Vault(format!("Google token exchange failed: {}", e))
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(PatroclusError::Vault(format!(
                    "Google token exchange HTTP {}: {}",
                    status, body
                )));
            }

            let data: serde_json::Value = resp.json().await.map_err(|e| {
                PatroclusError::Vault(format!("Google response parse error: {}", e))
            })?;

            if let Some(err) = data.get("error") {
                return Err(PatroclusError::Vault(format!(
                    "Google token exchange error: {}",
                    err.as_str().unwrap_or("unknown")
                )));
            }

            Ok(ProviderTokenResponse {
                access_token: data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                token_type: data
                    .get("token_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Bearer")
                    .to_string(),
                scope: data
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                expires_in: data.get("expires_in").and_then(|v| v.as_u64()),
                refresh_token: data
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
    }
}

pub struct SlackProvider {
    config: ProviderConfig,
}

impl SlackProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        SlackProvider {
            config: ProviderConfig {
                name: "slack".to_string(),
                token_url: "https://slack.com/api/oauth.token".to_string(),
                client_id,
                client_secret,
                default_scopes: vec!["chat:write".to_string(), "channels:read".to_string()],
            },
        }
    }
}

impl TokenExchangeProvider for SlackProvider {
    fn name(&self) -> &str {
        "slack"
    }

    fn exchange_refresh(
        &self,
        refresh_token: &str,
        _requested_scopes: &[String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderTokenResponse>> + Send>>
    {
        let config = self.config.clone();
        let token = refresh_token.to_string();

        Box::pin(async move {
            let client = reqwest::Client::new();
            let params = vec![
                ("client_id".to_string(), config.client_id.clone()),
                ("client_secret".to_string(), config.client_secret.clone()),
                ("refresh_token".to_string(), token.clone()),
                ("grant_type".to_string(), "refresh_token".to_string()),
            ];

            let resp = client
                .post(&config.token_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| {
                    PatroclusError::Vault(format!("Slack token exchange failed: {}", e))
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(PatroclusError::Vault(format!(
                    "Slack token exchange HTTP {}: {}",
                    status, body
                )));
            }

            let data: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| PatroclusError::Vault(format!("Slack response parse error: {}", e)))?;

            if !data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Err(PatroclusError::Vault(format!(
                    "Slack token exchange error: {}",
                    data.get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                )));
            }

            Ok(ProviderTokenResponse {
                access_token: data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                token_type: "bearer".to_string(),
                scope: data
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                expires_in: None,
                refresh_token: data
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
    }
}

pub fn create_provider(
    provider_name: &str,
    client_id: &str,
    client_secret: &str,
) -> Option<Box<dyn TokenExchangeProvider>> {
    match provider_name {
        "github" => Some(Box::new(GitHubProvider::new(
            client_id.to_string(),
            client_secret.to_string(),
        ))),
        "google" => Some(Box::new(GoogleProvider::new(
            client_id.to_string(),
            client_secret.to_string(),
        ))),
        "slack" => Some(Box::new(SlackProvider::new(
            client_id.to_string(),
            client_secret.to_string(),
        ))),
        _ => None,
    }
}
