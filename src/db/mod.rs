use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::audit::{AuditEntry, CreateAuditEntry};
use crate::errors::{PatroclusError, Result};
use crate::identity::{Agent, AgentStatus, AgentType, CreateAgentRequest, CreatePrincipalRequest, Principal};

pub mod migrations;

pub struct Database {
    conn: Arc<parking_lot::Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| PatroclusError::Database(format!("failed to open database: {}", e)))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        migrations::run(&conn)?;
        Ok(Database {
            conn: Arc::new(parking_lot::Mutex::new(conn)),
        })
    }

    pub fn create_default_policy(&self) -> Result<()> {
        let conn = self.conn.lock();
        let exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM policies WHERE status = 'active'", [], |row| row.get(0))
            .unwrap_or(0);
        if exists == 0 {
            let id = uuid::Uuid::now_v7().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO policies (id, name, version, engine, definition, status, created_at, updated_at)
                 VALUES (?, 'default', 1, 'yaml', ?, 'active', ?, ?)",
                rusqlite::params![
                    id,
                    "- name: deny-by-default\n  decision: deny\n  reason: No matching policy\n",
                    now,
                    now,
                ],
            )?;
        }
        Ok(())
    }

    pub fn create_agent(&self, req: &CreateAgentRequest) -> Result<Agent> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now();
        let type_str = match req.principal_type {
            AgentType::Service => "service",
            AgentType::Delegated => "delegated",
            AgentType::Autonomous => "autonomous",
        };
        conn.execute(
            "INSERT INTO agents (id, name, principal_type, public_key, did, owner_id, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?)",
            params![
                id.to_string(),
                req.name,
                type_str,
                req.public_key,
                req.did,
                req.owner_id.map(|u| u.to_string()),
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )?;
        drop(conn);
        self.get_agent(id)
    }

    pub fn get_agent(&self, id: Uuid) -> Result<Agent> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, principal_type, public_key, did, owner_id, status, created_at, updated_at
             FROM agents WHERE id = ?",
        )?;
        let agent = stmt.query_row(params![id.to_string()], |row| {
            let type_str: String = row.get(2)?;
            let status_str: String = row.get(6)?;
            let principal_type = match type_str.as_str() {
                "service" => AgentType::Service,
                "delegated" => AgentType::Delegated,
                _ => AgentType::Autonomous,
            };
            let status = match status_str.as_str() {
                "suspended" => AgentStatus::Suspended,
                "decommissioned" => AgentStatus::Decommissioned,
                _ => AgentStatus::Active,
            };
            let owner_id_str: Option<String> = row.get(5)?;
            Ok(Agent {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                principal_type,
                public_key: row.get(3)?,
                did: row.get(4)?,
                owner_id: owner_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                status,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        });
        match agent {
            Ok(a) => Ok(a),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(PatroclusError::AgentNotFound(id.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_agents(&self) -> Result<Vec<Agent>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, principal_type, public_key, did, owner_id, status, created_at, updated_at
             FROM agents ORDER BY created_at DESC",
        )?;
        let agents = stmt.query_map([], |row| {
            let type_str: String = row.get(2)?;
            let status_str: String = row.get(6)?;
            let principal_type = match type_str.as_str() {
                "service" => AgentType::Service,
                "delegated" => AgentType::Delegated,
                _ => AgentType::Autonomous,
            };
            let status = match status_str.as_str() {
                "suspended" => AgentStatus::Suspended,
                "decommissioned" => AgentStatus::Decommissioned,
                _ => AgentStatus::Active,
            };
            let owner_id_str: Option<String> = row.get(5)?;
            Ok(Agent {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                principal_type,
                public_key: row.get(3)?,
                did: row.get(4)?,
                owner_id: owner_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                status,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        let mut result = Vec::new();
        for agent in agents {
            result.push(agent?);
        }
        Ok(result)
    }

    pub fn create_principal(&self, req: &CreatePrincipalRequest) -> Result<Principal> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now();
        conn.execute(
            "INSERT INTO principals (id, external_id, idp_provider, email, display_name, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id.to_string(),
                req.external_id,
                req.idp_provider,
                req.email,
                req.display_name,
                now.to_rfc3339(),
            ],
        )?;
        drop(conn);
        self.get_principal(id)
    }

    pub fn get_principal(&self, id: Uuid) -> Result<Principal> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, external_id, idp_provider, email, display_name, created_at
             FROM principals WHERE id = ?",
        )?;
        let principal = stmt.query_row(params![id.to_string()], |row| {
            Ok(Principal {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                external_id: row.get(1)?,
                idp_provider: row.get(2)?,
                email: row.get(3)?,
                display_name: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        Ok(principal)
    }

    pub fn get_principal_by_email(&self, email: &str) -> Option<Principal> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, external_id, idp_provider, email, display_name, created_at
             FROM principals WHERE email = ?",
        )
        .ok()?;
        stmt.query_row(params![email], |row| {
            Ok(Principal {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                external_id: row.get(1)?,
                idp_provider: row.get(2)?,
                email: row.get(3)?,
                display_name: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })
        .ok()
    }

    pub fn create_audit_entry(&self, entry: &CreateAuditEntry) -> Result<AuditEntry> {
        let conn = self.conn.lock();
        let prev_hash: String = conn
            .query_row("SELECT row_hash FROM audit_log ORDER BY id DESC LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| "0000000000000000000000000000000000000000000000000000000000000000".to_string());

        let decision_str = match &entry.decision {
            crate::policy::Decision::Allow => "allow",
            crate::policy::Decision::Deny => "deny",
            crate::policy::Decision::RequireApproval { .. } => "require_approval",
        };

        let now = Utc::now();
        let mut audit = AuditEntry {
            id: 0,
            prev_hash,
            row_hash: String::new(),
            agent_id: entry.agent_id,
            principal_id: entry.principal_id,
            action: entry.action.clone(),
            resource: entry.resource.clone(),
            decision: decision_str.to_string(),
            reason: entry.reason.clone(),
            delegation_chain: entry.delegation_chain.clone(),
            token_jti: entry.token_jti.clone(),
            timestamp: now,
        };
        audit.row_hash = audit.compute_hash();

        conn.execute(
            "INSERT INTO audit_log (prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                audit.prev_hash,
                audit.row_hash,
                audit.agent_id.to_string(),
                audit.principal_id.map(|u| u.to_string()),
                audit.action,
                audit.resource,
                audit.decision,
                audit.reason,
                audit.delegation_chain.as_ref().map(|v| v.to_string()),
                audit.token_jti,
                audit.timestamp.to_rfc3339(),
            ],
        )?;

        audit.id = conn.last_insert_rowid();
        Ok(audit)
    }

    pub fn list_audit_entries(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, timestamp
             FROM audit_log ORDER BY id DESC LIMIT ?",
        )?;
        let entries = stmt.query_map(params![limit], |row| {
            let principal_id_str: Option<String> = row.get(4)?;
            let chain_str: Option<String> = row.get(9)?;
            Ok(AuditEntry {
                id: row.get(0)?,
                prev_hash: row.get(1)?,
                row_hash: row.get(2)?,
                agent_id: Uuid::parse_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                principal_id: principal_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                action: row.get(5)?,
                resource: row.get(6)?,
                decision: row.get(7)?,
                reason: row.get(8)?,
                delegation_chain: chain_str.and_then(|s| serde_json::from_str(&s).ok()),
                token_jti: row.get(10)?,
                timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    // ── Policy management ──────────────────────────────────────────

    pub fn load_active_policy_yaml(&self) -> Result<String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT definition FROM policies WHERE status = 'active' ORDER BY updated_at DESC LIMIT 1",
        )?;
        let yaml: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
        Ok(yaml.unwrap_or_default())
    }

    pub fn create_policy(&self, name: &str, engine: &str, definition: &str) -> Result<()> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE policies SET status = 'deprecated', updated_at = ? WHERE status = 'active'",
            params![now],
        )?;
        conn.execute(
            "INSERT INTO policies (id, name, version, engine, definition, status, created_at, updated_at)
             VALUES (?, ?, 1, ?, ?, 'active', ?, ?)",
            params![id, name, engine, definition, now, now],
        )?;
        Ok(())
    }

    pub fn list_policies(&self) -> Result<Vec<(Uuid, String, String, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, engine, status, definition FROM policies ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ── Resource management ────────────────────────────────────────

    pub fn create_resource(&self, req: &crate::resource::CreateResourceRequest) -> Result<crate::resource::Resource> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now();
        let type_str = match req.resource_type {
            crate::resource::ResourceType::McpServer => "mcp_server",
            crate::resource::ResourceType::Api => "api",
            crate::resource::ResourceType::Database => "database",
            crate::resource::ResourceType::CloudService => "cloud_service",
        };
        let sens_str = match req.sensitivity {
            crate::resource::Sensitivity::Low => "low",
            crate::resource::Sensitivity::Medium => "medium",
            crate::resource::Sensitivity::High => "high",
            crate::resource::Sensitivity::Critical => "critical",
        };
        conn.execute(
            "INSERT INTO resources (id, name, resource_type, uri, actions, sensitivity, owner_id, credential_config, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id.to_string(),
                req.name,
                type_str,
                req.uri,
                req.actions.to_string(),
                sens_str,
                req.owner_id.map(|u| u.to_string()),
                req.credential_config.as_ref().map(|v| v.to_string()),
                now.to_rfc3339(),
            ],
        )?;
        drop(conn);
        self.get_resource(id)
    }

    pub fn get_resource(&self, id: Uuid) -> Result<crate::resource::Resource> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, resource_type, uri, actions, sensitivity, owner_id, credential_config, created_at
             FROM resources WHERE id = ?",
        )?;
        let resource = stmt.query_row(params![id.to_string()], |row| {
            let type_str: String = row.get(2)?;
            let sens_str: String = row.get(5)?;
            let owner_id_str: Option<String> = row.get(6)?;
            let config_str: Option<String> = row.get(7)?;
            let actions_str: String = row.get(4)?;
            let resource_type = match type_str.as_str() {
                "mcp_server" => crate::resource::ResourceType::McpServer,
                "api" => crate::resource::ResourceType::Api,
                "cloud_service" => crate::resource::ResourceType::CloudService,
                _ => crate::resource::ResourceType::Database,
            };
            let sensitivity = match sens_str.as_str() {
                "low" => crate::resource::Sensitivity::Low,
                "high" => crate::resource::Sensitivity::High,
                "critical" => crate::resource::Sensitivity::Critical,
                _ => crate::resource::Sensitivity::Medium,
            };
            Ok(crate::resource::Resource {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                resource_type,
                uri: row.get(3)?,
                actions: serde_json::from_str(&actions_str).unwrap_or(serde_json::Value::Null),
                sensitivity,
                owner_id: owner_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                credential_config: config_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        Ok(resource)
    }

    pub fn list_resources(&self) -> Result<Vec<crate::resource::Resource>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, resource_type, uri, actions, sensitivity, owner_id, credential_config, created_at
             FROM resources ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let type_str: String = row.get(2)?;
            let sens_str: String = row.get(5)?;
            let owner_id_str: Option<String> = row.get(6)?;
            let config_str: Option<String> = row.get(7)?;
            let actions_str: String = row.get(4)?;
            let resource_type = match type_str.as_str() {
                "mcp_server" => crate::resource::ResourceType::McpServer,
                "api" => crate::resource::ResourceType::Api,
                "cloud_service" => crate::resource::ResourceType::CloudService,
                _ => crate::resource::ResourceType::Database,
            };
            let sensitivity = match sens_str.as_str() {
                "low" => crate::resource::Sensitivity::Low,
                "high" => crate::resource::Sensitivity::High,
                "critical" => crate::resource::Sensitivity::Critical,
                _ => crate::resource::Sensitivity::Medium,
            };
            Ok(crate::resource::Resource {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                resource_type,
                uri: row.get(3)?,
                actions: serde_json::from_str(&actions_str).unwrap_or(serde_json::Value::Null),
                sensitivity,
                owner_id: owner_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                credential_config: config_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn find_resource_by_uri(&self, uri: &str) -> Result<Option<Uuid>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id FROM resources WHERE uri = ?")?;
        let id_str: Option<String> = stmt.query_row(params![uri], |row| row.get(0)).ok();
        Ok(id_str.and_then(|s| Uuid::parse_str(&s).ok()))
    }

    pub fn list_all_grants(&self) -> Result<Vec<Grant>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, parent_grant_id, scopes, constraints, expires_at, revocable, revoked_at, created_at
             FROM grants ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let parent_str: Option<String> = row.get(3)?;
            let scopes_str: String = row.get(4)?;
            let constraints_str: Option<String> = row.get(5)?;
            let revoked_at_str: Option<String> = row.get(8)?;
            let revocable: i64 = row.get(7)?;
            Ok(Grant {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: Uuid::parse_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                parent_grant_id: parent_str.and_then(|s| Uuid::parse_str(&s).ok()),
                scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                constraints: constraints_str.and_then(|s| serde_json::from_str(&s).ok()),
                expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                revocable: revocable != 0,
                revoked_at: revoked_at_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ── Grant management ───────────────────────────────────────────

    pub fn create_grant(
        &self,
        agent_id: Uuid,
        principal_id: Uuid,
        parent_grant_id: Option<Uuid>,
        scopes: &[String],
        constraints: Option<&serde_json::Value>,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<Uuid> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO grants (id, agent_id, principal_id, parent_grant_id, scopes, constraints, expires_at, revocable, revoked_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, NULL, ?)",
            params![
                id.to_string(),
                agent_id.to_string(),
                principal_id.to_string(),
                parent_grant_id.map(|u| u.to_string()),
                serde_json::to_string(scopes).unwrap_or_default(),
                constraints.map(|v| v.to_string()),
                expires_at.to_rfc3339(),
                now,
            ],
        )?;
        Ok(id)
    }

    pub fn get_grant(&self, id: Uuid) -> Result<Grant> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, parent_grant_id, scopes, constraints, expires_at, revocable, revoked_at, created_at
             FROM grants WHERE id = ?",
        )?;
        let grant = stmt.query_row(params![id.to_string()], |row| {
            let parent_str: Option<String> = row.get(3)?;
            let scopes_str: String = row.get(4)?;
            let constraints_str: Option<String> = row.get(5)?;
            let revoked_at_str: Option<String> = row.get(8)?;
            let revocable: i64 = row.get(7)?;
            Ok(Grant {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: Uuid::parse_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                parent_grant_id: parent_str.and_then(|s| Uuid::parse_str(&s).ok()),
                scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                constraints: constraints_str.and_then(|s| serde_json::from_str(&s).ok()),
                expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                revocable: revocable != 0,
                revoked_at: revoked_at_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        Ok(grant)
    }

    pub fn list_grants_for_agent(&self, agent_id: Uuid) -> Result<Vec<Grant>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, parent_grant_id, scopes, constraints, expires_at, revocable, revoked_at, created_at
             FROM grants WHERE agent_id = ? AND revoked_at IS NULL AND expires_at > ?
             ORDER BY created_at DESC",
        )?;
        let now = Utc::now().to_rfc3339();
        let rows = stmt.query_map(params![agent_id.to_string(), now], |row| {
            let parent_str: Option<String> = row.get(3)?;
            let scopes_str: String = row.get(4)?;
            let constraints_str: Option<String> = row.get(5)?;
            let revoked_at_str: Option<String> = row.get(8)?;
            let revocable: i64 = row.get(7)?;
            Ok(Grant {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: Uuid::parse_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                parent_grant_id: parent_str.and_then(|s| Uuid::parse_str(&s).ok()),
                scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                constraints: constraints_str.and_then(|s| serde_json::from_str(&s).ok()),
                expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                revocable: revocable != 0,
                revoked_at: revoked_at_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn revoke_grant(&self, id: Uuid) -> Result<Vec<Uuid>> {
        let mut revoked = Vec::new();
        let conn = self.conn.lock();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE grants SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
            params![now, id.to_string()],
        )?;
        revoked.push(id);
        let mut children: Vec<String> = conn
            .prepare("SELECT id FROM grants WHERE parent_grant_id = ? AND revoked_at IS NULL")?
            .query_map(params![id.to_string()], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(conn);
        for child_str in children {
            if let Ok(child_id) = Uuid::parse_str(&child_str) {
                let mut child_revoked = self.revoke_grant(child_id)?;
                revoked.append(&mut child_revoked);
            }
        }
        Ok(revoked)
    }

    // ── Token management ───────────────────────────────────────────

    pub fn record_token(
        &self,
        jti: &str,
        grant_id: Option<Uuid>,
        agent_id: Uuid,
        scopes: &[String],
        audience: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tokens (id, grant_id, agent_id, scopes, audience, issued_at, expires_at, revoked, dpop_bound, key_thumbprint)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, NULL)",
            params![
                jti,
                grant_id.map(|u| u.to_string()),
                agent_id.to_string(),
                serde_json::to_string(scopes).unwrap_or_default(),
                audience,
                now,
                expires_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn revoke_token(&self, jti: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE tokens SET revoked = 1 WHERE id = ?", params![jti])?;
        Ok(())
    }

    pub fn is_token_revoked(&self, jti: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let revoked: i64 = conn
            .query_row("SELECT revoked FROM tokens WHERE id = ?", params![jti], |row| row.get(0))
            .unwrap_or(0);
        Ok(revoked != 0)
    }

    pub fn revoke_agent_tokens(&self, agent_id: Uuid) -> Result<usize> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE tokens SET revoked = 1 WHERE agent_id = ? AND revoked = 0",
            params![agent_id.to_string()],
        )?;
        let count = conn.changes() as usize;
        Ok(count)
    }

    // ── Approval management ────────────────────────────────────────

    pub fn create_approval_request(
        &self,
        agent_id: Uuid,
        principal_id: Option<Uuid>,
        resource_id: Uuid,
        action: &str,
        requested_scopes: &[String],
        ttl_seconds: u64,
    ) -> Result<crate::approval::ApprovalRequest> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);
        conn.execute(
            "INSERT INTO approval_requests (id, agent_id, principal_id, resource_id, action, requested_scopes, status, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            params![
                id.to_string(),
                agent_id.to_string(),
                principal_id.map(|u| u.to_string()),
                resource_id.to_string(),
                action,
                serde_json::to_string(requested_scopes).unwrap_or_default(),
                expires_at.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )?;
        drop(conn);
        self.get_approval_request(id)
    }

    pub fn get_approval_request(&self, id: Uuid) -> Result<crate::approval::ApprovalRequest> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, resource_id, action, requested_scopes, status, approver_id, reason, approval_token, expires_at, created_at, resolved_at
             FROM approval_requests WHERE id = ?",
        )?;
        let req = stmt.query_row(params![id.to_string()], |row| {
            let principal_id_str: Option<String> = row.get(2)?;
            let approver_id_str: Option<String> = row.get(7)?;
            let scopes_str: String = row.get(5)?;
            let status_str: String = row.get(6)?;
            let resolved_at_str: Option<String> = row.get(12)?;
            let status = match status_str.as_str() {
                "approved" => crate::approval::ApprovalStatus::Approved,
                "denied" => crate::approval::ApprovalStatus::Denied,
                "expired" => crate::approval::ApprovalStatus::Expired,
                _ => crate::approval::ApprovalStatus::Pending,
            };
            Ok(crate::approval::ApprovalRequest {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: principal_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                resource_id: Uuid::parse_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                action: row.get(4)?,
                requested_scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                status,
                approver_id: approver_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                reason: row.get(8)?,
                approval_token: row.get(9)?,
                expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                resolved_at: resolved_at_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
            })
        })?;
        Ok(req)
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<crate::approval::ApprovalRequest>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, resource_id, action, requested_scopes, status, approver_id, reason, approval_token, expires_at, created_at, resolved_at
             FROM approval_requests WHERE status = 'pending' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let principal_id_str: Option<String> = row.get(2)?;
            let approver_id_str: Option<String> = row.get(7)?;
            let scopes_str: String = row.get(5)?;
            let resolved_at_str: Option<String> = row.get(12)?;
            Ok(crate::approval::ApprovalRequest {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: principal_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                resource_id: Uuid::parse_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                action: row.get(4)?,
                requested_scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                status: crate::approval::ApprovalStatus::Pending,
                approver_id: approver_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                reason: row.get(8)?,
                approval_token: row.get(9)?,
                expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                resolved_at: resolved_at_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn resolve_approval_request(
        &self,
        id: Uuid,
        approver_id: Uuid,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<crate::approval::ApprovalRequest> {
        let conn = self.conn.lock();
        let now = Utc::now();
        let status = if approved { "approved" } else { "denied" };
        let approval_token = if approved {
            Some(Uuid::now_v7().to_string())
        } else {
            None
        };
        conn.execute(
            "UPDATE approval_requests SET status = ?, approver_id = ?, reason = ?, approval_token = ?, resolved_at = ? WHERE id = ? AND status = 'pending'",
            params![status, approver_id.to_string(), reason, approval_token.as_ref(), now.to_rfc3339(), id.to_string()],
        )?;
        if conn.changes() == 0 {
            return Err(PatroclusError::Database("approval request not found or already resolved".to_string()));
        }
        drop(conn);
        self.get_approval_request(id)
    }

    // ── Vault credential management ────────────────────────────────

    pub fn store_vault_credential(
        &self,
        principal_id: Uuid,
        provider: &str,
        encrypted_token: &[u8],
        nonce: &[u8],
        encryption_key_id: &str,
        scopes: &[String],
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Uuid> {
        let conn = self.conn.lock();
        let id = Uuid::now_v7();
        let now = Utc::now();
        conn.execute(
            "INSERT OR REPLACE INTO vault_credentials (id, principal_id, provider, encrypted_token, nonce, encryption_key_id, scopes, expires_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id.to_string(),
                principal_id.to_string(),
                provider,
                encrypted_token,
                nonce,
                encryption_key_id,
                serde_json::to_string(scopes).unwrap_or_default(),
                expires_at.map(|t| t.to_rfc3339()),
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )?;
        Ok(id)
    }

    pub fn get_vault_credential(
        &self,
        principal_id: Uuid,
        provider: &str,
    ) -> Result<Option<VaultCredentialRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, principal_id, provider, encrypted_token, nonce, encryption_key_id, scopes, expires_at, created_at, updated_at
             FROM vault_credentials WHERE principal_id = ? AND provider = ? ORDER BY updated_at DESC LIMIT 1",
        )?;
        let result = stmt.query_row(
            params![principal_id.to_string(), provider],
            |row| {
                let scopes_str: String = row.get(6)?;
                let expires_str: Option<String> = row.get(7)?;
                Ok(VaultCredentialRecord {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                    principal_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    provider: row.get(2)?,
                    encrypted_token: row.get(3)?,
                    nonce: row.get(4)?,
                    encryption_key_id: row.get(5)?,
                    scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
                    expires_at: expires_str.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).map(|d| d.with_timezone(&Utc)).unwrap_or(Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?).map(|d| d.with_timezone(&Utc)).unwrap_or(Utc::now()),
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VaultCredentialRecord {
    pub id: Uuid,
    pub principal_id: Uuid,
    pub provider: String,
    pub encrypted_token: Vec<u8>,
    pub nonce: Vec<u8>,
    pub encryption_key_id: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Grant {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub principal_id: Uuid,
    pub parent_grant_id: Option<Uuid>,
    pub scopes: Vec<String>,
    pub constraints: Option<serde_json::Value>,
    pub expires_at: chrono::DateTime<Utc>,
    pub revocable: bool,
    pub revoked_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}
