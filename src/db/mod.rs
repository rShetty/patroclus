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
        })?;
        Ok(agent)
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
}
