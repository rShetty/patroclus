use std::sync::Arc;

use crate::errors::{PatroclusError, Result};
use crate::policy::{Constraint, Decision, PolicyContext, PolicyEngine, PolicyEvaluation};
use crate::session::SessionStore;

pub struct YamlEngine {
    rules: Vec<Rule>,
    session_store: Option<Arc<SessionStore>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Rule {
    name: String,
    #[serde(default)]
    agent_types: Vec<String>,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    resources: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    decision: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    require_approval_from: Option<String>,
    #[serde(default)]
    constraints: Vec<ConstraintDef>,
    #[serde(default)]
    rate_limit_per_minute: Option<u64>,
    #[serde(default)]
    max_spend: Option<f64>,
    #[serde(default)]
    min_trust_level: Option<f64>,
    #[serde(default)]
    require_prior_action: Option<String>,
    #[serde(default)]
    max_actions_in_session: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ConstraintDef {
    key: String,
    value: serde_json::Value,
}

impl YamlEngine {
    pub fn new() -> Self {
        YamlEngine {
            rules: Vec::new(),
            session_store: None,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let mut engine = YamlEngine::new();
        engine.load_from_str(yaml)?;
        Ok(engine)
    }

    pub fn with_session_store(mut self, store: Arc<SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn load_from_str(&mut self, yaml: &str) -> Result<()> {
        let parsed: Vec<Rule> = serde_yaml::from_str(yaml)
            .map_err(|e| PatroclusError::Config(format!("YAML policy parse error: {}", e)))?;
        self.rules = parsed;
        Ok(())
    }

    fn matches(&self, rule: &Rule, ctx: &PolicyContext) -> bool {
        if !rule.agent_types.is_empty() {
            let agent_type = match ctx.agent.principal_type {
                AgentType::Service => "service",
                AgentType::Delegated => "delegated",
                AgentType::Autonomous => "autonomous",
            };
            if !rule.agent_types.contains(&agent_type.to_string()) {
                return false;
            }
        }

        if !rule.actions.is_empty() && !rule.actions.contains(&ctx.action) {
            return false;
        }

        if !rule.resources.is_empty() {
            let matched = rule
                .resources
                .iter()
                .any(|pattern| match_pattern(pattern, &ctx.resource));
            if !matched {
                return false;
            }
        }

        if !rule.scopes.is_empty() {
            let matched = ctx.requested_scopes.iter().any(|req| {
                rule.scopes
                    .iter()
                    .any(|allowed| match_pattern(allowed, req))
            });
            if !matched {
                return false;
            }
        }

        true
    }

    fn check_temporal_conditions(&self, rule: &Rule, ctx: &PolicyContext) -> Option<String> {
        let session = match (&self.session_store, &ctx.session_id) {
            (Some(store), Some(sid)) => store.get_session(sid),
            _ => return None,
        };

        if let Some(session) = session {
            if session.killed {
                return Some("Session has been killed by emergency stop".to_string());
            }

            if let Some(max_actions) = rule.max_actions_in_session {
                if session.actions_count >= max_actions {
                    return Some(format!(
                        "Max actions in session exceeded ({} >= {})",
                        session.actions_count, max_actions
                    ));
                }
            }

            if let Some(max_spend) = rule.max_spend {
                if session.cumulative_spend() >= max_spend {
                    return Some(format!(
                        "Session spend cap exceeded ($ {:.2} >= $ {:.2})",
                        session.cumulative_spend(),
                        max_spend
                    ));
                }
            }

            if let Some(min_trust) = rule.min_trust_level {
                if !session.is_allowed_by_trust(min_trust) {
                    return Some(format!(
                        "Trust level too low ({:.2} < {:.2}) — session may have been idle",
                        session.trust_level, min_trust
                    ));
                }
            }

            if let Some(required_prior) = &rule.require_prior_action {
                let has_prior = session
                    .trajectory
                    .iter()
                    .any(|e| &e.action == required_prior);
                if !has_prior {
                    return Some(format!(
                        "Required prior action '{}' not found in session trajectory",
                        required_prior
                    ));
                }
            }
        }

        None
    }

    fn check_rate_limit(&self, rule: &Rule, ctx: &PolicyContext) -> Option<String> {
        if let Some(max_per_min) = rule.rate_limit_per_minute {
            if let (Some(store), Some(sid)) = (&self.session_store, &ctx.session_id) {
                let key = format!("{}:{}:{}", sid, ctx.action, ctx.resource);
                if let Err(e) = store.check_rate_limit(&key, max_per_min) {
                    return Some(e.to_string());
                }
            }
        }
        None
    }
}

fn match_pattern(pattern: &str, value: &str) -> bool {
    if pattern == "*" || pattern == value {
        return true;
    }
    if pattern.ends_with("/*") {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }
    if pattern.ends_with(":*") {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }
    if pattern.ends_with("-*") {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }
    if let Some(prefix) = pattern.strip_suffix("*") {
        return value.starts_with(prefix);
    }
    false
}

use crate::identity::AgentType;

impl PolicyEngine for YamlEngine {
    fn evaluate(&self, ctx: &PolicyContext) -> Result<PolicyEvaluation> {
        for rule in &self.rules {
            if self.matches(rule, ctx) {
                if let Some(reason) = self.check_temporal_conditions(rule, ctx) {
                    return Ok(PolicyEvaluation {
                        decision: Decision::Deny,
                        approved_scopes: Vec::new(),
                        reason,
                        constraints: Vec::new(),
                    });
                }

                if let Some(reason) = self.check_rate_limit(rule, ctx) {
                    return Ok(PolicyEvaluation {
                        decision: Decision::Deny,
                        approved_scopes: Vec::new(),
                        reason,
                        constraints: Vec::new(),
                    });
                }

                let decision = match rule.decision.as_str() {
                    "allow" => Decision::Allow,
                    "deny" => Decision::Deny,
                    "require_approval" => Decision::RequireApproval {
                        approver_id: None,
                        reason: rule.reason.clone(),
                    },
                    _ => Decision::Deny,
                };

                let approved_scopes = if rule.scopes.is_empty() {
                    ctx.requested_scopes.clone()
                } else {
                    ctx.requested_scopes
                        .iter()
                        .filter(|req| {
                            rule.scopes
                                .iter()
                                .any(|allowed| match_pattern(allowed, req))
                        })
                        .cloned()
                        .collect()
                };

                let constraints = rule
                    .constraints
                    .iter()
                    .map(|c| Constraint {
                        key: c.key.clone(),
                        value: c.value.clone(),
                    })
                    .collect();

                return Ok(PolicyEvaluation {
                    decision,
                    approved_scopes,
                    reason: rule.reason.clone(),
                    constraints,
                });
            }
        }

        Ok(PolicyEvaluation {
            decision: Decision::Deny,
            approved_scopes: Vec::new(),
            reason: "No matching policy found (default deny)".to_string(),
            constraints: Vec::new(),
        })
    }
}
