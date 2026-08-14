use crate::errors::{PatroclusError, Result};
use crate::policy::{Constraint, Decision, PolicyContext, PolicyEvaluation, PolicyEngine};

pub struct YamlEngine {
    rules: Vec<Rule>,
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
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ConstraintDef {
    key: String,
    value: serde_json::Value,
}

impl YamlEngine {
    pub fn new() -> Self {
        YamlEngine { rules: Vec::new() }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let mut engine = YamlEngine::new();
        engine.load_from_str(yaml)?;
        Ok(engine)
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
            let matched = rule.resources.iter().any(|pattern| {
                match_pattern(pattern, &ctx.resource)
            });
            if !matched {
                return false;
            }
        }

        if !rule.scopes.is_empty() {
            let matched = ctx.requested_scopes.iter().any(|req| {
                rule.scopes.iter().any(|allowed| match_pattern(allowed, req))
            });
            if !matched {
                return false;
            }
        }

        true
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
                            rule.scopes.iter().any(|allowed| match_pattern(allowed, req))
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
