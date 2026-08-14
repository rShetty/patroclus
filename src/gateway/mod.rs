use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::policy::PolicyEvaluation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub agent_id: Uuid,
    pub action: String,
    pub resource: String,
    pub requested_scopes: Vec<String>,
    pub delegation_token: Option<String>,
    pub context: Option<RequestContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub session_id: Option<String>,
    pub task: Option<String>,
    pub max_amount: Option<f64>,
    pub time_window: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessResponse {
    pub decision: String,
    pub token: Option<IssuedTokenInfo>,
    pub approval: Option<ApprovalInfo>,
    pub reason: String,
    pub approved_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedTokenInfo {
    pub jwt: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scopes: Vec<String>,
    pub jti: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInfo {
    pub request_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateRequest {
    pub parent_grant_token: String,
    pub sub_agent_id: Uuid,
    pub scopes: Vec<String>,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateResponse {
    pub delegated_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scopes: Vec<String>,
}

impl From<PolicyEvaluation> for AccessResponse {
    fn from(eval: PolicyEvaluation) -> Self {
        let (decision, approval) = match &eval.decision {
            crate::policy::Decision::Allow => ("allow".to_string(), None),
            crate::policy::Decision::Deny => ("deny".to_string(), None),
            crate::policy::Decision::RequireApproval { reason, .. } => {
                ("require_approval".to_string(), Some(ApprovalInfo {
                    request_id: Uuid::nil(),
                    status: "pending".to_string(),
                }))
            }
        };
        AccessResponse {
            decision,
            token: None,
            approval,
            reason: eval.reason,
            approved_scopes: eval.approved_scopes,
        }
    }
}
