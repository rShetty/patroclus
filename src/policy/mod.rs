use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{PatroclusError, Result};
use crate::identity::{Agent, AgentType, Principal};

pub mod yaml_engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    pub agent: Agent,
    pub principal: Option<Principal>,
    pub action: String,
    pub resource: String,
    pub requested_scopes: Vec<String>,
    pub session_id: Option<String>,
    pub trajectory: Vec<TrajectoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryEvent {
    pub action: String,
    pub resource: String,
    pub decision: Decision,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    RequireApproval { approver_id: Option<Uuid>, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    pub decision: Decision,
    pub approved_scopes: Vec<String>,
    pub reason: String,
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub key: String,
    pub value: serde_json::Value,
}

pub trait PolicyEngine: Send + Sync {
    fn evaluate(&self, ctx: &PolicyContext) -> Result<PolicyEvaluation>;
}

pub fn create_engine(engine_type: &str) -> Result<Box<dyn PolicyEngine>> {
    match engine_type {
        "yaml" => Ok(Box::new(yaml_engine::YamlEngine::new())),
        other => Err(PatroclusError::NotImplemented(format!(
            "policy engine '{}' not yet implemented",
            other
        ))),
    }
}
