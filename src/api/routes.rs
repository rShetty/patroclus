use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::state::AppState;
use crate::audit::CreateAuditEntry;
use crate::errors::PatroclusError;
use crate::gateway::{AccessRequest, AccessResponse};
use crate::identity::{CreateAgentRequest, CreatePrincipalRequest};
use crate::policy::{Decision, PolicyContext};
use crate::token::IssueTokenParams;

type MethodRouter = axum::routing::MethodRouter<AppState>;

pub fn all_routes() -> Vec<(String, MethodRouter)> {
    vec![
        // Dashboard
        ("/".to_string(), get(dashboard)),
        // Health
        ("/health".to_string(), get(health)),
        // Agent-facing
        ("/v1/agent/request-access".to_string(), post(request_access)),
        ("/v1/agent/check".to_string(), post(check_access)),
        ("/v1/agent/delegate".to_string(), post(delegate)),
        (
            "/v1/agent/approval-status/{id}".to_string(),
            get(approval_status),
        ),
        // Principal-facing
        (
            "/v1/principal/delegate".to_string(),
            post(principal_delegate),
        ),
        (
            "/v1/principal/grants".to_string(),
            get(list_principal_grants),
        ),
        (
            "/v1/principal/grants/{id}/revoke".to_string(),
            post(revoke_grant),
        ),
        (
            "/v1/principal/approvals".to_string(),
            get(list_pending_approvals),
        ),
        (
            "/v1/principal/approvals/{id}/approve".to_string(),
            post(approve_request),
        ),
        (
            "/v1/principal/approvals/{id}/deny".to_string(),
            post(deny_request),
        ),
        // Admin — Agents
        (
            "/v1/admin/agents".to_string(),
            post(create_agent).get(list_agents),
        ),
        ("/v1/admin/agents/{id}".to_string(), get(get_agent)),
        // Admin — Principals
        ("/v1/admin/principals".to_string(), post(create_principal)),
        // Admin — Resources
        (
            "/v1/admin/resources".to_string(),
            post(create_resource).get(list_resources),
        ),
        // Admin — Policies
        (
            "/v1/admin/policies".to_string(),
            post(create_policy).get(list_policies),
        ),
        // Admin — Audit
        ("/v1/admin/audit".to_string(), get(list_audit)),
        // Admin — Token revocation
        (
            "/v1/admin/tokens/{jti}/revoke".to_string(),
            post(revoke_token),
        ),
        // Vault — credential storage and vending
        (
            "/v1/vault/credentials".to_string(),
            post(store_credential).get(list_vault_credentials),
        ),
        ("/v1/vault/vend".to_string(), post(vend_credential)),
        (
            "/v1/vault/generate-key".to_string(),
            post(generate_vault_key),
        ),
        // Session management
        ("/v1/sessions".to_string(), get(list_sessions)),
        ("/v1/sessions/{id}/kill".to_string(), post(kill_session)),
        // Kill switch — emergency stop for agent
        ("/v1/admin/agents/{id}/kill".to_string(), post(kill_agent)),
        (
            "/v1/admin/agents/{id}/spend".to_string(),
            post(record_spend),
        ),
        // IdP Federation
        (
            "/v1/idp/authorize/{provider}".to_string(),
            get(idp_authorize),
        ),
        ("/v1/idp/callback".to_string(), get(idp_callback)),
        ("/v1/idp/userinfo".to_string(), post(idp_userinfo)),
        ("/v1/idp/providers".to_string(), get(list_idp_providers)),
    ]
}

async fn dashboard() -> (StatusCode, String) {
    (StatusCode::OK, crate::dashboard::dashboard_html())
}

async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok", "service": "patroclus" })),
    )
}

// ── Agent-facing routes ───────────────────────────────────────────

async fn request_access(
    State(state): State<AppState>,
    Json(req): Json<AccessRequest>,
) -> Result<Json<AccessResponse>, PatroclusError> {
    let agent = state.db.get_agent(req.agent_id)?;
    let principal = if let Some(token) = &req.delegation_token {
        let claims = state.token_verifier.verify(token, None)?;
        state
            .db
            .get_principal_by_email(&claims.sub.replace("user:", ""))
    } else {
        agent
            .owner_id
            .and_then(|oid| state.db.get_principal(oid).ok())
    };

    let session_id = req
        .context
        .as_ref()
        .and_then(|c| c.session_id.clone())
        .unwrap_or_else(|| format!("agent-{}-default", agent.id));

    let session = state.session_store.get_or_create_session(
        &session_id,
        agent.id,
        principal.as_ref().map(|p| p.id),
    );

    if session.killed {
        return Ok(Json(AccessResponse {
            decision: "deny".to_string(),
            token: None,
            approval: None,
            reason: "Agent session has been killed by emergency stop".to_string(),
            approved_scopes: vec![],
        }));
    }

    let ctx = PolicyContext {
        agent: agent.clone(),
        principal: principal.clone(),
        action: req.action.clone(),
        resource: req.resource.clone(),
        requested_scopes: req.requested_scopes.clone(),
        session_id: Some(session_id.clone()),
        trajectory: session.trajectory.clone(),
    };

    let eval = state.eval_engine(&ctx)?;
    let mut response = AccessResponse::from(eval.clone());

    match &eval.decision {
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

            let (jwt, jti) = state.token_issuer.issue(&params)?;
            let expires_at = params.expiry();

            state.db.record_token(
                &jti,
                None,
                agent.id,
                &eval.approved_scopes,
                &req.resource,
                expires_at,
            )?;

            response.token = Some(crate::gateway::IssuedTokenInfo {
                jwt,
                expires_at,
                scopes: eval.approved_scopes.clone(),
                jti,
            });
        }
        Decision::RequireApproval { .. } => {
            let resource_id = state.db.find_resource_by_uri(&req.resource).ok().flatten();
            let approval = state.db.create_approval_request(
                agent.id,
                principal.as_ref().map(|p| p.id),
                resource_id,
                &req.action,
                &req.requested_scopes,
                300,
            )?;
            response.approval = Some(crate::gateway::ApprovalInfo {
                request_id: approval.id,
                status: "pending".to_string(),
            });
        }
        Decision::Deny => {}
    }

    state.session_store.record_action(
        &session_id,
        crate::policy::TrajectoryEvent {
            action: req.action.clone(),
            resource: req.resource.clone(),
            decision: eval.decision.clone(),
            timestamp: chrono::Utc::now(),
        },
    );

    let audit = CreateAuditEntry {
        agent_id: agent.id,
        principal_id: principal.as_ref().map(|p| p.id),
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
    let principal = agent
        .owner_id
        .and_then(|oid| state.db.get_principal(oid).ok());

    let ctx = PolicyContext {
        agent: agent.clone(),
        principal: principal.clone(),
        action: req.action.clone(),
        resource: req.resource.clone(),
        requested_scopes: req.requested_scopes.clone(),
        session_id: req.context.as_ref().and_then(|c| c.session_id.clone()),
        trajectory: Vec::new(),
    };

    let eval = state.eval_engine(&ctx)?;

    Ok(Json(serde_json::json!({
        "allowed": matches!(eval.decision, Decision::Allow),
        "decision": match eval.decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
            Decision::RequireApproval { .. } => "require_approval",
        },
        "approved_scopes": eval.approved_scopes,
        "reason": eval.reason,
    })))
}

async fn delegate(
    State(state): State<AppState>,
    Json(req): Json<crate::gateway::DelegateRequest>,
) -> Result<Json<crate::gateway::DelegateResponse>, PatroclusError> {
    let parent_claims = state.token_verifier.verify(&req.parent_grant_token, None)?;
    let parent_scopes: Vec<String> = parent_claims
        .scope
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    for scope in &req.scopes {
        if !parent_scopes.contains(scope) {
            return Err(PatroclusError::ScopeEscalation {
                requested: scope.clone(),
                parent: parent_claims.scope.clone(),
            });
        }
    }

    let max_depth = state.config.policy.max_delegation_depth;
    let new_depth = parent_claims.act.delegation_depth + 1;
    if new_depth > max_depth {
        return Err(PatroclusError::DelegationDepthExceeded {
            max: max_depth,
            actual: new_depth,
        });
    }

    let parent_expiry =
        chrono::DateTime::from_timestamp(parent_claims.exp, 0).unwrap_or_else(Utc::now);
    let requested_expiry = Utc::now() + Duration::seconds(req.expires_in_seconds as i64);
    let effective_expiry = parent_expiry.min(requested_expiry);
    let effective_ttl = (effective_expiry - Utc::now()).num_seconds().max(0) as u64;

    let mut delegation_chain = parent_claims.act.delegation_chain.unwrap_or_default();
    delegation_chain.push(crate::token::DelegationHop {
        sub: parent_claims.sub.clone(),
        act: parent_claims.act.sub.clone(),
    });

    let params = IssueTokenParams {
        issuer: state.config.token.issuer.clone(),
        subject: parent_claims.sub.clone(),
        agent_id: format!("agent:{}", req.sub_agent_id),
        scopes: req.scopes.clone(),
        audience: parent_claims.aud.clone(),
        ttl_seconds: effective_ttl,
        delegation_depth: new_depth,
        delegation_chain: Some(delegation_chain),
        constraints: parent_claims.constraints.clone(),
    };

    let (jwt, _jti) = state.token_issuer.issue(&params)?;

    Ok(Json(crate::gateway::DelegateResponse {
        delegated_token: jwt,
        expires_at: effective_expiry,
        scopes: req.scopes,
    }))
}

async fn approval_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::approval::ApprovalRequest>, PatroclusError> {
    let req = state.db.get_approval_request(id)?;
    Ok(Json(req))
}

// ── Principal-facing routes ───────────────────────────────────────

#[derive(Deserialize)]
struct PrincipalDelegateRequest {
    agent_id: Uuid,
    scopes: Vec<String>,
    constraints: Option<serde_json::Value>,
    expires_in_seconds: u64,
}

async fn principal_delegate(
    State(state): State<AppState>,
    Json(req): Json<PrincipalDelegateRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let agent = state.db.get_agent(req.agent_id)?;
    if agent.owner_id.is_none() {
        return Err(PatroclusError::AgentNotFound(
            "agent has no owner principal".to_string(),
        ));
    }
    let principal_id = agent.owner_id.unwrap();
    let principal = state.db.get_principal(principal_id)?;

    let expires_at = Utc::now() + Duration::seconds(req.expires_in_seconds as i64);
    let grant_id = state.db.create_grant(
        agent.id,
        principal_id,
        None,
        &req.scopes,
        req.constraints.as_ref(),
        expires_at,
    )?;

    let params = IssueTokenParams {
        issuer: state.config.token.issuer.clone(),
        subject: format!("user:{}", principal.email),
        agent_id: format!("agent:{}", agent.id),
        scopes: req.scopes.clone(),
        audience: format!("agent:{}", agent.id),
        ttl_seconds: req
            .expires_in_seconds
            .min(state.config.token.max_ttl_seconds),
        delegation_depth: 0,
        delegation_chain: None,
        constraints: req.constraints.clone(),
    };

    let (jwt, jti) = state.token_issuer.issue(&params)?;

    state.db.record_token(
        &jti,
        Some(grant_id),
        agent.id,
        &req.scopes,
        &format!("agent:{}", agent.id),
        expires_at,
    )?;

    Ok(Json(serde_json::json!({
        "grant_id": grant_id,
        "delegation_token": jwt,
        "scopes": req.scopes,
        "expires_at": expires_at,
    })))
}

async fn list_principal_grants(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let grants = state.db.list_all_grants()?;
    Ok(Json(serde_json::json!({ "grants": grants })))
}

#[derive(Deserialize)]
struct RevokeGrantRequest {
    #[serde(default)]
    #[allow(dead_code)]
    cascade: bool,
}

async fn revoke_grant(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(_req): Json<RevokeGrantRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let revoked = state.db.revoke_grant(id)?;
    Ok(Json(serde_json::json!({
        "revoked_grants": revoked,
        "count": revoked.len(),
    })))
}

async fn list_pending_approvals(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::approval::ApprovalRequest>>, PatroclusError> {
    let approvals = state.db.list_pending_approvals()?;
    Ok(Json(approvals))
}

#[derive(Deserialize)]
struct ApproveRequest {
    approver_id: Uuid,
    #[serde(default)]
    reason: Option<String>,
}

async fn approve_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveRequest>,
) -> Result<Json<crate::approval::ApprovalRequest>, PatroclusError> {
    let approval =
        state
            .db
            .resolve_approval_request(id, req.approver_id, true, req.reason.as_deref())?;
    Ok(Json(approval))
}

async fn deny_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveRequest>,
) -> Result<Json<crate::approval::ApprovalRequest>, PatroclusError> {
    let approval =
        state
            .db
            .resolve_approval_request(id, req.approver_id, false, req.reason.as_deref())?;
    Ok(Json(approval))
}

// ── Admin routes ──────────────────────────────────────────────────

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

async fn create_resource(
    State(state): State<AppState>,
    Json(req): Json<crate::resource::CreateResourceRequest>,
) -> Result<Json<crate::resource::Resource>, PatroclusError> {
    let resource = state.db.create_resource(&req)?;
    Ok(Json(resource))
}

async fn list_resources(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::resource::Resource>>, PatroclusError> {
    let resources = state.db.list_resources()?;
    Ok(Json(resources))
}

#[derive(Deserialize)]
struct CreatePolicyRequest {
    name: String,
    engine: String,
    definition: String,
}

async fn create_policy(
    State(state): State<AppState>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    state
        .db
        .create_policy(&req.name, &req.engine, &req.definition)?;
    state
        .reload_policy()
        .map_err(|e| PatroclusError::Config(e.to_string()))?;
    tracing::info!("Policy '{}' created and hot-reloaded", req.name);

    Ok(Json(
        serde_json::json!({ "status": "created", "reloaded": true }),
    ))
}

async fn list_policies(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let policies = state.db.list_policies()?;
    let result: Vec<serde_json::Value> = policies
        .into_iter()
        .map(|(id, name, engine, status, definition)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "engine": engine,
                "status": status,
                "definition": definition,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "policies": result })))
}

async fn list_audit(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::audit::AuditEntry>>, PatroclusError> {
    let entries = state.db.list_audit_entries(100)?;
    Ok(Json(entries))
}

async fn revoke_token(
    State(state): State<AppState>,
    Path(jti): Path<String>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    state.db.revoke_token(&jti)?;
    state.token_verifier.revoke(&jti);
    Ok(Json(serde_json::json!({ "revoked": jti })))
}

// ── Vault routes ──────────────────────────────────────────────────

async fn store_credential(
    State(state): State<AppState>,
    Json(req): Json<crate::vault::StoreCredentialRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let vault = state.vault.as_ref().ok_or_else(|| {
        PatroclusError::Vault("Vault not initialized — no encryption key configured".to_string())
    })?;

    let (encrypted, nonce) = vault.encrypt(&req.refresh_token)?;
    let id = state.db.store_vault_credential(
        req.principal_id,
        &req.provider,
        &encrypted,
        &nonce,
        vault.key_id(),
        &req.scopes,
        req.expires_at,
    )?;

    Ok(Json(serde_json::json!({
        "id": id,
        "provider": req.provider,
        "principal_id": req.principal_id,
        "scopes": req.scopes,
        "stored": true,
    })))
}

async fn list_vault_credentials(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let _vault = state
        .vault
        .as_ref()
        .ok_or_else(|| PatroclusError::Vault("Vault not initialized".to_string()))?;
    Ok(Json(serde_json::json!({
        "credentials": [],
        "note": "Vault credential listing requires admin authentication (not yet implemented)"
    })))
}

#[derive(Deserialize)]
struct VendCredentialBody {
    principal_id: Uuid,
    provider: String,
    requested_scopes: Vec<String>,
    agent_token_jti: String,
    client_id: Option<String>,
    client_secret: Option<String>,
}

async fn vend_credential(
    State(state): State<AppState>,
    Json(req): Json<VendCredentialBody>,
) -> Result<Json<crate::vault::VendCredentialResponse>, PatroclusError> {
    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| PatroclusError::Vault("Vault not initialized".to_string()))?;

    let record = state
        .db
        .get_vault_credential(req.principal_id, &req.provider)?
        .ok_or_else(|| {
            PatroclusError::Vault(format!(
                "No stored credential for provider '{}' and principal '{}'",
                req.provider, req.principal_id
            ))
        })?;

    let refresh_token = vault.decrypt(&record.encrypted_token, &record.nonce)?;

    let client_id = req.client_id.unwrap_or_default();
    let client_secret = req.client_secret.unwrap_or_default();

    let provider =
        crate::vault::providers::create_provider(&req.provider, &client_id, &client_secret)
            .ok_or_else(|| PatroclusError::Vault(format!("Unknown provider: {}", req.provider)))?;

    let token_response = provider
        .exchange_refresh(&refresh_token, &req.requested_scopes)
        .await?;

    let expires_at = token_response
        .expires_in
        .map(|secs| chrono::Utc::now() + chrono::Duration::seconds(secs as i64));

    Ok(Json(crate::vault::VendCredentialResponse {
        provider: req.provider.clone(),
        access_token: token_response.access_token,
        scopes: req.requested_scopes,
        expires_at,
        vended_for_jti: req.agent_token_jti,
    }))
}

async fn generate_vault_key(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let path = &state.config.vault.encryption_key_path;
    crate::vault::Vault::generate_key(path)?;
    Ok(Json(serde_json::json!({
        "generated": true,
        "path": path,
    })))
}

// ── Session management & kill switch ──────────────────────────────

async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let sessions = state.session_store.list_sessions();
    let result: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.session_id,
                "agent_id": s.agent_id,
                "principal_id": s.principal_id,
                "created_at": s.created_at,
                "last_activity": s.last_activity,
                "actions_count": s.actions_count,
                "spend_total": s.spend_total,
                "tokens_used": s.tokens_used,
                "trust_level": s.trust_level,
                "killed": s.killed,
                "trajectory_length": s.trajectory.len(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "sessions": result })))
}

async fn kill_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let killed = state.session_store.kill_session(&id);
    Ok(Json(serde_json::json!({
        "killed": killed,
        "session_id": id,
    })))
}

async fn kill_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let sessions = state.session_store.list_sessions();
    let mut killed = 0;
    for s in &sessions {
        if s.agent_id == id && !s.killed {
            state.session_store.kill_session(&s.session_id);
            killed += 1;
        }
    }
    state.db.revoke_agent_tokens(id)?;
    Ok(Json(serde_json::json!({
        "killed": true,
        "agent_id": id,
        "sessions_killed": killed,
    })))
}

#[derive(Deserialize)]
struct RecordSpendRequest {
    amount: f64,
    session_id: Option<String>,
}

async fn record_spend(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<RecordSpendRequest>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("agent-{}-default", id));
    state.session_store.record_spend(&session_id, req.amount);
    let session = state.session_store.get_session(&session_id);
    Ok(Json(serde_json::json!({
        "agent_id": id,
        "session_id": session_id,
        "spend_recorded": req.amount,
        "cumulative_spend": session.map(|s| s.cumulative_spend()).unwrap_or(req.amount),
    })))
}

// ── IdP Federation routes ─────────────────────────────────────────

async fn idp_authorize(
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let provider = state
        .config
        .idp
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| {
            PatroclusError::Config(format!("IdP provider '{}' not configured", provider_name))
        })?;

    let redirect_uri = format!("{}/v1/idp/callback", state.config.token.issuer);
    let state_param = uuid::Uuid::now_v7().to_string();
    let code_verifier = uuid::Uuid::now_v7().to_string();

    let auth_url = crate::idp::IdpFederation::authorization_url(
        provider,
        &redirect_uri,
        &state_param,
        &code_verifier,
    );

    Ok(Json(serde_json::json!({
        "authorization_url": auth_url,
        "state": state_param,
        "code_verifier": code_verifier,
        "provider": provider_name,
    })))
}

#[derive(Deserialize)]
struct IdpCallbackQuery {
    code: String,
    state: String,
}

async fn idp_callback(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<IdpCallbackQuery>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    if !state.config.idp.enabled || state.config.idp.providers.is_empty() {
        return Err(PatroclusError::Config(
            "IdP federation not enabled".to_string(),
        ));
    }

    let provider = &state.config.idp.providers[0];
    let redirect_uri = format!("{}/v1/idp/callback", state.config.token.issuer);

    let access_token = crate::idp::IdpFederation::exchange_oidc_token(
        provider,
        &params.code,
        &redirect_uri,
        &params.state,
    )
    .await?;

    let user_info = crate::idp::IdpFederation::fetch_userinfo(provider, &access_token).await?;

    let principal = state.db.get_principal_by_email(&user_info.email);
    let principal_id = if let Some(p) = principal {
        p.id
    } else {
        state
            .db
            .create_principal(&crate::identity::CreatePrincipalRequest {
                external_id: user_info.subject.clone(),
                idp_provider: provider.name.clone(),
                email: user_info.email.clone(),
                display_name: user_info.name.unwrap_or_else(|| user_info.email.clone()),
            })?
            .id
    };

    Ok(Json(serde_json::json!({
        "authenticated": true,
        "principal_id": principal_id,
        "email": user_info.email,
        "groups": user_info.groups,
        "issuer": user_info.issuer,
    })))
}

#[derive(Deserialize)]
struct IdpUserInfoRequest {
    provider: String,
    access_token: String,
}

async fn idp_userinfo(
    State(state): State<AppState>,
    Json(req): Json<IdpUserInfoRequest>,
) -> Result<Json<crate::idp::IdpUserInfo>, PatroclusError> {
    let provider = state
        .config
        .idp
        .providers
        .iter()
        .find(|p| p.name == req.provider)
        .ok_or_else(|| {
            PatroclusError::Config(format!("IdP provider '{}' not found", req.provider))
        })?;

    let user_info = crate::idp::IdpFederation::fetch_userinfo(provider, &req.access_token).await?;
    Ok(Json(user_info))
}

async fn list_idp_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, PatroclusError> {
    let providers: Vec<serde_json::Value> = state
        .config
        .idp
        .providers
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "issuer": p.issuer,
                "client_id": p.client_id,
                "scopes": p.scopes,
                "group_claim": p.group_claim,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "enabled": state.config.idp.enabled,
        "providers": providers,
    })))
}
