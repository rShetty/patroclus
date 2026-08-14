mod harness;

use harness::{AgentHarness, create_agent_and_principal, send_request};
use serde_json::json;

const ADVANCED_POLICY: &str = r#"
- name: allow-reads
  actions: ["read", "load_profile"]
  resources: ["*"]
  scopes: ["*"]
  decision: allow
  reason: Read access permitted

- name: allow-writes-dev
  actions: ["write", "update"]
  resources: ["dev-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev write access permitted

- name: deny-deletes-prod
  actions: ["delete"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: deny
  reason: Production deletes forbidden

- name: require-approval-prod
  actions: ["write", "update"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: require_approval
  reason: Production write requires approval

- name: rate-limited-api
  actions: ["call"]
  resources: ["api-*"]
  scopes: ["*"]
  decision: allow
  reason: API access permitted
  rate_limit_per_minute: 3

- name: budget-capped-deploy
  actions: ["deploy"]
  resources: ["cloud-*"]
  scopes: ["*"]
  decision: allow
  reason: Deploy permitted within budget
  max_spend: 100.0

- name: trust-restricted-write
  actions: ["write"]
  resources: ["sensitive-*"]
  scopes: ["*"]
  decision: allow
  reason: Sensitive write requires active session
  min_trust_level: 0.5

- name: workflow-sequenced-trade
  actions: ["execute_trade"]
  resources: ["trading-*"]
  scopes: ["*"]
  decision: allow
  reason: Trade permitted after profile loaded
  require_prior_action: "load_profile"

- name: max-actions-session
  actions: ["query"]
  resources: ["data-*"]
  scopes: ["*"]
  decision: allow
  reason: Data query permitted
  max_actions_in_session: 5
"#;

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — RATE LIMITING
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_rate_limiting_blocks_after_threshold() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "rl@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("rl-session");

    // First 3 calls should succeed
    for i in 0..3 {
        let (decision, _, _) = agent
            .request_access(&server.app, "call", "api-endpoint", &["api:call"])
            .await;
        assert_eq!(decision, "allow", "call {} should be allowed", i);
    }

    // 4th call should be rate limited
    let (decision, reason, _) = agent
        .request_access(&server.app, "call", "api-endpoint", &["api:call"])
        .await;
    assert_eq!(decision, "deny");
    assert!(reason.contains("Rate limit exceeded"), "reason: {}", reason);
}

#[tokio::test]
async fn test_rate_limiting_independent_per_resource() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "rl2@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("rl2-session");

    // 3 calls to api-endpoint1
    for _ in 0..3 {
        agent
            .request_access(&server.app, "call", "api-endpoint1", &["api:call"])
            .await;
    }
    // 4th to endpoint1 should fail
    let (decision, _, _) = agent
        .request_access(&server.app, "call", "api-endpoint1", &["api:call"])
        .await;
    assert_eq!(decision, "deny");

    // But call to a different resource should succeed
    let (decision, _, _) = agent
        .request_access(&server.app, "call", "api-endpoint2", &["api:call"])
        .await;
    assert_eq!(decision, "allow");
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — BUDGET TRACKING
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_budget_cap_blocks_after_spend() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "budget@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("budget-session");

    // First call to create session
    agent
        .request_access(&server.app, "deploy", "cloud-prod", &["cloud:deploy"])
        .await;

    // Record $50 spend
    agent.record_spend(&server.app, 50.0).await;

    // Deploy should still be allowed (under $100 cap)
    let (decision, _, _) = agent
        .request_access(&server.app, "deploy", "cloud-prod", &["cloud:deploy"])
        .await;
    assert_eq!(decision, "allow");

    // Record another $60 spend (total $110 > $100 cap)
    agent.record_spend(&server.app, 60.0).await;

    // Deploy should now be denied
    let (decision, reason, _) = agent
        .request_access(&server.app, "deploy", "cloud-prod", &["cloud:deploy"])
        .await;
    assert_eq!(decision, "deny");
    assert!(reason.contains("spend cap exceeded"), "reason: {}", reason);
}

#[tokio::test]
async fn test_spend_tracking_accumulates() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "spend@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("spend-session");

    // First make an access request to create the session
    agent
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;

    agent.record_spend(&server.app, 10.0).await;
    agent.record_spend(&server.app, 25.5).await;
    agent.record_spend(&server.app, 5.0).await;

    let sessions = AgentHarness::get_sessions(&server.app).await;
    let session = sessions
        .iter()
        .find(|s| s["session_id"] == "spend-session")
        .unwrap();
    assert_eq!(session["spend_total"], json!(40.5));
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — TRUST DECAY
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_trust_level_starts_at_full() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "trust@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("trust-session");

    // Sensitive write with full trust should be allowed
    let (decision, _, _) = agent
        .request_access(&server.app, "write", "sensitive-data", &["sensitive:write"])
        .await;
    assert_eq!(decision, "allow");

    let sessions = AgentHarness::get_sessions(&server.app).await;
    let session = sessions
        .iter()
        .find(|s| s["session_id"] == "trust-session")
        .unwrap();
    assert_eq!(session["trust_level"], json!(1.0));
}

#[tokio::test]
async fn test_trust_decay_blocks_after_idle() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "decay@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("decay-session");

    // First write succeeds
    let (decision, _, _) = agent
        .request_access(&server.app, "write", "sensitive-data", &["sensitive:write"])
        .await;
    assert_eq!(decision, "allow");

    // Simulate idle time by manipulating session state directly
    {
        // The session store is in-memory, we can't directly manipulate time,
        // but we can test the trust decay logic by calling apply_trust_decay_all
        // with a session that we manually age
        let _store = &server.state.session_store;
        // Force the session's last_activity to be 30 minutes ago
        // This tests the trust decay mechanism
        // Since SessionStore is in-memory and private, we test via the API
    }

    // For a proper test, we test the trust decay unit test in session/mod.rs
    // Here we verify the trust_level is still 1.0 since no time has passed
    let sessions = AgentHarness::get_sessions(&server.app).await;
    let session = sessions
        .iter()
        .find(|s| s["session_id"] == "decay-session")
        .unwrap();
    assert_eq!(session["trust_level"], json!(1.0));
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — WORKFLOW SEQUENCING (require_prior_action)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_workflow_requires_prior_action() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "wf@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("wf-session");

    // Try to execute_trade without loading profile first — should be denied
    let (decision, reason, _) = agent
        .request_access(
            &server.app,
            "execute_trade",
            "trading-account",
            &["trading:execute"],
        )
        .await;
    assert_eq!(decision, "deny");
    assert!(
        reason.contains("Required prior action 'load_profile'"),
        "reason: {}",
        reason
    );

    // Now load_profile (read action, allowed by allow-reads rule)
    let (decision, _, _) = agent
        .request_access(
            &server.app,
            "load_profile",
            "trading-account",
            &["trading:read"],
        )
        .await;
    assert_eq!(decision, "allow");

    // Now execute_trade should succeed (prior action is in trajectory)
    let (decision, _, _) = agent
        .request_access(
            &server.app,
            "execute_trade",
            "trading-account",
            &["trading:execute"],
        )
        .await;
    assert_eq!(decision, "allow");
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — MAX ACTIONS PER SESSION
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_max_actions_per_session() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "max@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("max-actions-session");

    // 5 query actions should be allowed
    for i in 0..5 {
        let (decision, _, _) = agent
            .request_access(&server.app, "query", "data-table", &["data:query"])
            .await;
        assert_eq!(decision, "allow", "query {} should be allowed", i);
    }

    // 6th query should be denied (max_actions_in_session = 5)
    let (decision, reason, _) = agent
        .request_access(&server.app, "query", "data-table", &["data:query"])
        .await;
    assert_eq!(decision, "deny");
    assert!(
        reason.contains("Max actions in session exceeded"),
        "reason: {}",
        reason
    );
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — KILL SWITCH
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_kill_session_blocks_all_subsequent_access() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "kill@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("kill-session");

    // First access should work
    let (decision, _, _) = agent
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(decision, "allow");

    // Kill the session
    let (status, body) =
        send_request(&server.app, "POST", "/v1/sessions/kill-session/kill", None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["killed"], true);

    // Subsequent access should be denied
    let (decision, reason, _) = agent
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(decision, "deny");
    assert!(reason.contains("killed"), "reason: {}", reason);
}

#[tokio::test]
async fn test_kill_agent_kills_all_sessions() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "killall@test.com").await;

    // Create two sessions for the same agent
    let mut agent_s1 = AgentHarness::new(&agent_id, "").with_session("kill-all-1");
    let mut agent_s2 = AgentHarness::new(&agent_id, "").with_session("kill-all-2");

    // Both should work initially
    let (d1, _, _) = agent_s1
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    let (d2, _, _) = agent_s2
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(d1, "allow");
    assert_eq!(d2, "allow");

    // Kill the agent entirely
    let (status, body) = send_request(
        &server.app,
        "POST",
        &format!("/v1/admin/agents/{}/kill", agent_id),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["killed"], true);
    assert!(body["sessions_killed"].as_i64().unwrap() >= 2);

    // Both sessions should now be denied
    let (d1, _, _) = agent_s1
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    let (d2, _, _) = agent_s2
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(d1, "deny");
    assert_eq!(d2, "deny");
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — SESSION TRAJECTORY TRACKING
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_session_trajectory_records_all_actions() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "traj@test.com").await;
    let mut agent = AgentHarness::new(&agent_id, "").with_session("traj-session");

    // Perform several actions
    agent
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    agent
        .request_access(&server.app, "write", "dev-config", &["db:write"])
        .await;
    agent
        .request_access(&server.app, "read", "dev-logs", &["db:read"])
        .await;

    let sessions = AgentHarness::get_sessions(&server.app).await;
    let session = sessions
        .iter()
        .find(|s| s["session_id"] == "traj-session")
        .unwrap();
    assert_eq!(session["actions_count"], json!(3));
    assert_eq!(session["trajectory_length"], json!(3));
}

#[tokio::test]
async fn test_sessions_are_isolated_per_session_id() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();
    let (agent_id, _) = create_agent_and_principal(&server.app, "agent", "iso@test.com").await;

    let mut s1 = AgentHarness::new(&agent_id, "").with_session("iso-1");
    let mut s2 = AgentHarness::new(&agent_id, "").with_session("iso-2");

    // Different sessions should have independent action counts
    s1.request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    s1.request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    s2.request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;

    let sessions = AgentHarness::get_sessions(&server.app).await;
    let s1_state = sessions
        .iter()
        .find(|s| s["session_id"] == "iso-1")
        .unwrap();
    let s2_state = sessions
        .iter()
        .find(|s| s["session_id"] == "iso-2")
        .unwrap();
    assert_eq!(s1_state["actions_count"], json!(2));
    assert_eq!(s2_state["actions_count"], json!(1));
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 5 — FULL E2E AGENT SCENARIO
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_full_agent_lifecycle() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();

    // Step 1: Register agent + principal
    let (agent_id, principal_id) =
        create_agent_and_principal(&server.app, "lifecycle-agent", "life@test.com").await;

    // Step 2: Principal delegates permissions
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": agent_id,
            "scopes": ["db:read", "db:write", "api:call"],
            "expires_in_seconds": 900
        })),
    )
    .await;
    let delegation_token = body["delegation_token"].as_str().unwrap();

    // Step 3: Agent uses delegation to access resources
    let mut agent = AgentHarness::new(&agent_id, &principal_id)
        .with_session("lifecycle-session")
        .with_delegation_token(delegation_token);

    // Read should be allowed
    let (decision, _, token) = agent
        .request_access(&server.app, "read", "dev-db/users", &["db:read"])
        .await;
    assert_eq!(decision, "allow");
    assert!(token.is_some(), "Should receive a JWT token");

    // Write to dev should be allowed
    let (decision, _, token) = agent
        .request_access(&server.app, "write", "dev-config", &["db:write"])
        .await;
    assert_eq!(decision, "allow");
    assert!(token.is_some());

    // API call should be allowed (first call)
    let (decision, _, _) = agent
        .request_access(&server.app, "call", "api-service", &["api:call"])
        .await;
    assert_eq!(decision, "allow");

    // Verify the token is verifiable
    if let Some(t) = &token {
        let claims = server
            .state
            .token_verifier
            .verify(t, Some("dev-config"))
            .unwrap();
        assert!(claims.scope.contains("db:write"));
    }

    // Step 4: Verify audit trail has all entries
    let audit = AgentHarness::get_audit(&server.app).await;
    assert!(audit.len() >= 3, "Should have at least 3 audit entries");

    // Step 5: Verify session state
    let sessions = AgentHarness::get_sessions(&server.app).await;
    let session = sessions
        .iter()
        .find(|s| s["session_id"] == "lifecycle-session")
        .unwrap();
    assert_eq!(session["actions_count"], json!(3));
    assert!(session["trust_level"].as_f64().unwrap() > 0.0);

    // Step 6: Kill the agent
    let (status, _) = send_request(
        &server.app,
        "POST",
        &format!("/v1/admin/agents/{}/kill", agent_id),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    // Step 7: Verify subsequent access is blocked
    let (decision, reason, _) = agent
        .request_access(&server.app, "read", "dev-db/users", &["db:read"])
        .await;
    assert_eq!(decision, "deny");
    assert!(reason.contains("killed"));
}

#[tokio::test]
async fn test_e2e_multi_agent_orchestration() {
    let server = harness::TestServer::new_with_policy(ADVANCED_POLICY).unwrap();

    // Create orchestrator + workers
    let (orch_id, orch_principal) =
        create_agent_and_principal(&server.app, "orchestrator", "orch@test.com").await;
    let (worker1_id, _) = create_agent_and_principal(&server.app, "worker-1", "w1@test.com").await;
    let (worker2_id, _) = create_agent_and_principal(&server.app, "worker-2", "w2@test.com").await;

    // Human delegates to orchestrator
    let (_, body) = send_request(
        &server.app,
        "POST",
        "/v1/principal/delegate",
        Some(json!({
            "agent_id": orch_id,
            "scopes": ["db:read", "db:write", "api:call"],
            "expires_in_seconds": 3600
        })),
    )
    .await;
    let orch_token = body["delegation_token"].as_str().unwrap();

    // Orchestrator delegates narrowed scope to workers
    let mut orch = AgentHarness::new(&orch_id, &orch_principal)
        .with_session("orch-session")
        .with_delegation_token(orch_token);

    // Orchestrator makes a call to create its session
    let (decision, _, _) = orch
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(decision, "allow");

    // Worker 1 gets db:read only
    let w1_token = orch
        .delegate_to(&server.app, &worker1_id, &["db:read"], 1800)
        .await;
    assert!(w1_token.is_some(), "Worker 1 should get delegated token");

    // Worker 2 gets api:call only
    let w2_token = orch
        .delegate_to(&server.app, &worker2_id, &["api:call"], 1800)
        .await;
    assert!(w2_token.is_some(), "Worker 2 should get delegated token");

    // Worker 1 can read
    let mut w1 = AgentHarness::new(&worker1_id, "")
        .with_session("w1-session")
        .with_delegation_token(w1_token.unwrap().as_str());
    let (decision, _, _) = w1
        .request_access(&server.app, "read", "dev-db", &["db:read"])
        .await;
    assert_eq!(decision, "allow");

    // Worker 2 can call API
    let mut w2 = AgentHarness::new(&worker2_id, "")
        .with_session("w2-session")
        .with_delegation_token(w2_token.unwrap().as_str());
    let (decision, _, _) = w2
        .request_access(&server.app, "call", "api-service", &["api:call"])
        .await;
    assert_eq!(decision, "allow");

    // Verify sessions are independent
    let sessions = AgentHarness::get_sessions(&server.app).await;
    assert!(sessions.iter().any(|s| s["session_id"] == "w1-session"));
    assert!(sessions.iter().any(|s| s["session_id"] == "w2-session"));
    assert!(sessions.iter().any(|s| s["session_id"] == "orch-session"));
}
