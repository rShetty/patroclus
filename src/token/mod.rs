use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub mod issuer;
pub mod verifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClaims {
    pub iss: String,
    pub sub: String,
    pub act: ActClaim,
    pub scope: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnf: Option<CnfClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActClaim {
    pub sub: String,
    pub delegation_depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_chain: Option<Vec<DelegationHop>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationHop {
    pub sub: String,
    pub act: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CnfClaim {
    pub jkt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTokenParams {
    pub issuer: String,
    pub subject: String,
    pub agent_id: String,
    pub scopes: Vec<String>,
    pub audience: String,
    pub ttl_seconds: u64,
    pub delegation_depth: usize,
    pub delegation_chain: Option<Vec<DelegationHop>>,
    pub constraints: Option<serde_json::Value>,
}

impl IssueTokenParams {
    pub fn expiry(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.ttl_seconds as i64)
    }
}
