use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::state::AppState;
use crate::audit::CreateAuditEntry;
use crate::errors::PatroclusError;
use crate::gateway::{AccessRequest, AccessResponse};
use crate::identity::{CreateAgentRequest, CreatePrincipalRequest};
use crate::policy::{Decision, PolicyContext, PolicyEvaluation};
use crate::token::IssueTokenParams;

type MethodRouter = axum::routing::MethodRouter<AppState>;

pub fn all_routes() -> Vec<(String, MethodRouter)> {
    vec![
        // Health
        ("/health".to_string(), get(health)),

        // Agent-facing
        ("/v1/agent/request-access".to_string(), post(request_access)),
        ("/v1/agent/check".to_string(), post(check_access)),
        ("/v1/agent/delegate".to_string(), post(delegate)),

        // Admin — Agents
        ("/v1/admin/agents".to_string(), post(create_agent).get(list_agents)),
        ("/v1/admin/agents/{id}".to_string(), get(get_agent)),

        // Admin — Principals
        ("/v1/admin/principals".to_string(), post(create_principal)),

        // Admin — Audit
        ("/v1/admin/audit".to_string(), get(list_audit)),
    ]
}

async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok", "service": "patroclus" })))
}

async fn request_access(
    State(state): State<AppState>,
    Json(req): Json<AccessRequest>,
) -> Result<Json<AccessResponse>, PatroclusError> {
    let agent = state.db.get_agent(req.agent_id)?;
    let principal = if let Some(_token) = &req.delegation_token {
        None
    } else {
        agent.owner_id.and_then(|oid| state.db.get_principal(oid).ok())
    };

    let ctx = PolicyContext {
        agent: agent.clone(),
        principal: principal.clone(),
        action: req.action.clone(),
        resource: req.resource.clone(),
        requested_scopes: req.requested_scopes.clone(),
        session_id: req.context.as_ref().and_then(|c| c.session_id.clone()),
        trajectory: Vec::new(),
    };

    let eval = state.policy_engine.evaluate(&ctx)?;

    let mut response = AccessResponse::from(eval.clone());

    match eval.decision {
        Decision::Allow => {
            let params = IssueTokenParams {
                issuer: state.config.token.issuer.clone(),
                subject: principal
                    .as_ref()
                    .map(|p| format!("user:{}", p.email))
                    .unwrap_or_else(|| format!("agent:{}", agent.id)),
                agent_id: format!("agent:{}", agent.id),
                scopes: eval.approved_scopes.clone(),
                audience: req.resource.clone(),
                ttl_seconds: state.config.token.default_ttl_seconds,
                delegation_depth: 0,
                delegation_chain: None,
                constraints: if eval.constraints.is_empty() {
                    None
                } else {
                    let mut map = serde_json::Map::new();
                    for c in &eval.constraints {
                        map.insert(c.key.clone(), c.value.clone());
                    }
                    Some(serde_json::Value::Object(map))
                },
            };

            let jwt = state.token_issuer.issue(&params)?;
            let jti = extract_jti(&jwt);

            response.token = Some(crate::gateway::IssuedTokenInfo {
                jwt,
                expires_at: params.expiry(),
                scopes: eval.approved_scopes.clone(),
                jti: jti.clone(),
            });
        }
        Decision::Deny => {}
        Decision::RequireApproval { .. } => {}
    }

    let audit = CreateAuditEntry {
        agent_id: agent.id,
        principal_id: principal.map(|p| p.id),
        action: req.action,
        resource: req.resource,
        decision: eval.decision.clone(),
        reason: eval.reason.clone(),
        delegation_chain: None,
        token_jti: response.token.as_ref().map(|t| t.jti.clone()),
    };
    state.db.create_audit_entry(&audit)?;

    Ok(Json(response))
}

async fn check_access(
    State(state): State<AppState>,
    Json(req): Json<AccessRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let agent = state.db.get_agent(req.agent_id)?;
    let principal = agent.owner_id.and_then(|oid| state.db.get_principal(oid).ok());

    let ctx = PolicyContext {
        agent: agent.clone(),
        principal: principal.clone(),
        action: req.action.clone(),
        resource: req.resource.clone(),
        requested_scopes: req.requested_scopes.clone(),
        session_id: req.context.as_ref().and_then(|c| c.session_id.clone()),
        trajectory: Vec::new(),
    };

    let eval = state.policy_engine.evaluate(&ctx)?;

    Ok(Json(serde_json::json!({
        "allowed": matches!(eval.decision, Decision::Allow),
        "decision": format!("{:?}", eval.decision).to_lowercase(),
        "approved_scopes": eval.approved_scopes,
        "reason": eval.reason,
    })))
}

async fn delegate(
    State(_state): State<AppState>,
    Json(_req): Json<crate::gateway::DelegateRequest>,
) -> Result<Json<crate::gateway::DelegateResponse>, PatroclusError> {
    Err(PatroclusError::NotImplemented("delegation flow".to_string()))
}

async fn create_agent(
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<crate::identity::Agent>, PatroclusError> {
    let agent = state.db.create_agent(&req)?;
    Ok(Json(agent))
}

async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::identity::Agent>>, PatroclusError> {
    let agents = state.db.list_agents()?;
    Ok(Json(agents))
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::identity::Agent>, PatroclusError> {
    let agent = state.db.get_agent(id)?;
    Ok(Json(agent))
}

async fn create_principal(
    State(state): State<AppState>,
    Json(req): Json<CreatePrincipalRequest>,
) -> Result<Json<crate::identity::Principal>, PatroclusError> {
    let principal = state.db.create_principal(&req)?;
    Ok(Json(principal))
}

async fn list_audit(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::audit::AuditEntry>>, PatroclusError> {
    let entries = state.db.list_audit_entries(100)?;
    Ok(Json(entries))
}

fn extract_jti(jwt: &str) -> String {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Uuid::now_v7().to_string();
    }
    use base64::Engine;
    if let Ok(payload) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        if let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&payload) {
            if let Some(jti) = claims.get("jti") {
                return jti.as_str().unwrap_or_default().to_string();
            }
        }
    }
    Uuid::now_v7().to_string()
}
