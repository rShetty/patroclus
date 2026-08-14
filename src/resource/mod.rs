use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::AgentType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: Uuid,
    pub name: String,
    pub resource_type: ResourceType,
    pub uri: String,
    pub actions: serde_json::Value,
    pub sensitivity: Sensitivity,
    pub owner_id: Option<Uuid>,
    pub credential_config: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    McpServer,
    Api,
    Database,
    CloudService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResourceRequest {
    pub name: String,
    pub resource_type: ResourceType,
    pub uri: String,
    pub actions: serde_json::Value,
    pub sensitivity: Sensitivity,
    pub owner_id: Option<Uuid>,
    pub credential_config: Option<serde_json::Value>,
}
