use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub principal_id: Option<Uuid>,
    pub resource_id: Uuid,
    pub action: String,
    pub requested_scopes: Vec<String>,
    pub status: ApprovalStatus,
    pub approver_id: Option<Uuid>,
    pub reason: Option<String>,
    pub approval_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApprovalRequest {
    pub agent_id: Uuid,
    pub principal_id: Option<Uuid>,
    pub resource_id: Uuid,
    pub action: String,
    pub requested_scopes: Vec<String>,
    pub ttl_seconds: Option<u64>,
}

impl CreateApprovalRequest {
    pub fn expiry(&self) -> DateTime<Utc> {
        let ttl = self.ttl_seconds.unwrap_or(300);
        Utc::now() + Duration::seconds(ttl as i64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approver_id: Uuid,
    pub approved: bool,
    pub reason: Option<String>,
}
