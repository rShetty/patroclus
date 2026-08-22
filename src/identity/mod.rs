use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub principal_type: AgentType,
    pub public_key: Option<String>,
    pub did: Option<String>,
    pub owner_id: Option<Uuid>,
    pub status: AgentStatus,
    #[serde(skip_serializing)]
    #[serde(default)]
    pub client_key_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Service,
    Delegated,
    Autonomous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Suspended,
    Decommissioned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub id: Uuid,
    pub external_id: String,
    pub idp_provider: String,
    pub email: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub principal_type: AgentType,
    pub public_key: Option<String>,
    pub did: Option<String>,
    pub owner_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePrincipalRequest {
    pub external_id: String,
    pub idp_provider: String,
    pub email: String,
    pub display_name: String,
}
