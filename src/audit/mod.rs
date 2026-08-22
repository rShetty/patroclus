use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::policy::Decision;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub prev_hash: String,
    pub row_hash: String,
    pub agent_id: Uuid,
    pub principal_id: Option<Uuid>,
    pub action: String,
    pub resource: String,
    pub decision: String,
    pub reason: String,
    pub delegation_chain: Option<serde_json::Value>,
    pub token_jti: Option<String>,
    /// True when the entry records a dry-run evaluation (`/v1/agent/check`)
    /// rather than an enforced access decision. Part of the hashed payload so
    /// flipping the flag after the fact breaks the chain.
    pub dry_run: bool,
    pub timestamp: DateTime<Utc>,
}

impl AuditEntry {
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.prev_hash.as_bytes());
        hasher.update(self.agent_id.to_string().as_bytes());
        if let Some(pid) = self.principal_id {
            hasher.update(pid.to_string().as_bytes());
        }
        hasher.update(self.action.as_bytes());
        hasher.update(self.resource.as_bytes());
        hasher.update(self.decision.as_bytes());
        hasher.update(self.reason.as_bytes());
        hasher.update(self.timestamp.to_rfc3339().as_bytes());
        if let Some(chain) = &self.delegation_chain
            && let Ok(s) = serde_json::to_string(chain)
        {
            hasher.update(s.as_bytes());
        }
        if let Some(jti) = &self.token_jti {
            hasher.update(jti.as_bytes());
        }
        hasher.update([u8::from(self.dry_run)]);
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAuditEntry {
    pub agent_id: Uuid,
    pub principal_id: Option<Uuid>,
    pub action: String,
    pub resource: String,
    pub decision: Decision,
    pub reason: String,
    pub delegation_chain: Option<serde_json::Value>,
    pub token_jti: Option<String>,
    /// Marks the entry as a dry-run (non-enforcing) evaluation.
    #[serde(default)]
    pub dry_run: bool,
}

mod verifier;

pub use verifier::{BrokenLink, BrokenLinkReason, ChainVerification, verify_chain};
