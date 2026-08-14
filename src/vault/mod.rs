use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultCredential {
    pub id: Uuid,
    pub principal_id: Uuid,
    pub provider: String,
    pub encrypted_token: Vec<u8>,
    pub encryption_key_id: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCredentialRequest {
    pub principal_id: Uuid,
    pub provider: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendCredentialRequest {
    pub principal_id: Uuid,
    pub provider: String,
    pub requested_scopes: Vec<String>,
    pub agent_token_jti: String,
}
