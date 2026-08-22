use std::future::Future;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, params};
use uuid::Uuid;

use crate::audit::{AuditEntry, CreateAuditEntry};
use crate::errors::{PatroclusError, Result};
use crate::identity::{
    Agent, AgentStatus, AgentType, CreateAgentRequest, CreatePrincipalRequest, Principal,
};

pub mod migrations;

/// SQLite `busy_timeout` in milliseconds. With WAL enabled writers only block
/// each other briefly (checkpointing), so a modest timeout is enough to ride
/// out transient lock contention without stalling requests indefinitely.
pub(crate) const BUSY_TIMEOUT_MS: u64 = 5_000;

/// Apply per-connection pragmas and tuning.
///
/// * `journal_mode=WAL` — readers never block the writer and vice versa.
/// * `busy_timeout` — wait instead of failing with `SQLITE_BUSY`.
/// * `synchronous=NORMAL` — the recommended durability setting for WAL; safe
///   against application crashes, only vulnerable to power loss on the last
///   transactions before a crash.
///
/// # Checkpointing
///
/// WAL checkpointing is left at SQLite's defaults: an automatic checkpoint
/// runs every 1000 WAL pages (~4 MiB by default), moving frames back into the
/// main database file so the WAL does not grow without bound. A passive
/// truncating checkpoint (`wal_checkpoint(TRUNCATE)`) can additionally be
/// triggered via [`Database::checkpoint_wal`] — suitable for a periodic
/// maintenance task or a low-traffic window.
fn tune_connection(conn: &Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;",
    )?;
    Ok(())
}

pub struct Database {
    /// Single shared connection used for writes (and for reads when no read
    /// pool is configured). Access is serialized behind a mutex, but every
    /// call runs on the blocking thread pool via [`tokio::task::spawn_blocking`]
    /// so tokio workers are never blocked by SQLite I/O.
    conn: Arc<Mutex<Connection>>,
    /// Optional pool of read-only connections (see
    /// `[database].read_pool_size`). Reads served from this pool run
    /// concurrently against WAL-mode snapshots instead of queueing on the
    /// single write connection.
    read_pool: Option<Arc<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| PatroclusError::Database(format!("failed to open database: {}", e)))?;
        tune_connection(&conn)?;
        migrations::run(&conn)?;

        // The read pool is only built for real files: `:memory:` connections
        // would give pool members separate private databases.
        Ok(Database {
            conn: Arc::new(Mutex::new(conn)),
            read_pool: None,
        })
    }

    /// Construct a database honouring the runtime configuration, including
    /// the optional read pool for read-heavy endpoints.
    pub fn with_config(config: &crate::config::DatabaseConfig) -> Result<Self> {
        let conn = Connection::open(&config.path)
            .map_err(|e| PatroclusError::Database(format!("failed to open database: {}", e)))?;
        tune_connection(&conn)?;
        migrations::run(&conn)?;

        let read_pool = if config.read_pool_size > 0 && config.path != ":memory:" {
            let manager = r2d2_sqlite::SqliteConnectionManager::file(&config.path)
                .with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX);
            let pool = r2d2::Builder::new()
                .max_size(config.read_pool_size as u32)
                .build(manager)
                .map_err(|e| PatroclusError::Database(format!("read pool init failed: {}", e)))?;
            Some(Arc::new(pool))
        } else {
            None
        };

        if let Some(pool) = &read_pool {
            tracing::info!(
                "SQLite read pool enabled with {} connections",
                pool.max_size()
            );
        }

        Ok(Database {
            conn: Arc::new(Mutex::new(conn)),
            read_pool,
        })
    }

    /// Run a blocking closure with the shared (write) connection off the
    /// async worker threads.
    fn spawn_write<T, F>(&self, f: F) -> impl Future<Output = Result<T>>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        async move {
            tokio::task::spawn_blocking(move || {
                let mut guard = conn.lock();
                f(&mut guard)
            })
            .await
            .map_err(|e| PatroclusError::Database(format!("db task panicked: {e}")))?
        }
    }

    /// Run a blocking read closure, preferring a pooled read-only connection
    /// when one is configured.
    fn spawn_read<T, F>(&self, f: F) -> impl Future<Output = Result<T>>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        let pool = self.read_pool.clone();
        async move {
            tokio::task::spawn_blocking(move || match pool {
                Some(pool) => {
                    let pooled = pool.get().map_err(|e| {
                        PatroclusError::Database(format!("read pool exhausted: {e}"))
                    })?;
                    f(&pooled)
                }
                None => {
                    let guard = conn.lock();
                    f(&guard)
                }
            })
            .await
            .map_err(|e| PatroclusError::Database(format!("db task panicked: {e}")))?
        }
    }

    /// Force a WAL checkpoint (`PRAGMA wal_checkpoint(TRUNCATE)`). Exposed
    /// for periodic maintenance; automatic checkpointing otherwise suffices.
    pub async fn checkpoint_wal(&self) -> Result<()> {
        self.spawn_write(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            Ok(())
        })
        .await
    }

    pub async fn create_default_policy(&self) -> Result<()> {
        self.spawn_write(Self::create_default_policy_sync).await
    }

    fn create_default_policy_sync(conn: &mut Connection) -> Result<()> {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policies WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
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

    pub async fn create_agent(&self, req: &CreateAgentRequest) -> Result<Agent> {
        let req = req.clone();
        self.spawn_write(move |conn| Self::create_agent_sync(conn, &req))
            .await
    }

    fn create_agent_sync(conn: &mut Connection, req: &CreateAgentRequest) -> Result<Agent> {
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
        Self::get_agent_sync(conn, id)
    }

    /// Store the SHA-256 hash of an agent's client key. The raw key is never
    /// persisted.
    pub async fn set_agent_client_key_hash(&self, agent_id: Uuid, key_hash: &str) -> Result<()> {
        let key_hash = key_hash.to_string();
        self.spawn_write(move |conn| {
            Self::set_agent_client_key_hash_sync(conn, agent_id, &key_hash)
        })
        .await
    }

    fn set_agent_client_key_hash_sync(
        conn: &mut Connection,
        agent_id: Uuid,
        key_hash: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let updated = conn.execute(
            "UPDATE agents SET client_key_hash = ?, updated_at = ? WHERE id = ?",
            params![key_hash, now.to_rfc3339(), agent_id.to_string()],
        )?;
        if updated == 0 {
            return Err(PatroclusError::AgentNotFound(agent_id.to_string()));
        }
        Ok(())
    }

    /// Look up an active agent by its client-key hash. Returns `Ok(None)`
    /// when no agent matches so callers can treat every failure uniformly.
    pub async fn get_agent_by_client_key_hash(&self, key_hash: &str) -> Result<Option<Agent>> {
        let key_hash = key_hash.to_string();
        self.spawn_read(move |conn| Self::get_agent_by_client_key_hash_sync(conn, &key_hash))
            .await
    }

    fn get_agent_by_client_key_hash_sync(
        conn: &Connection,
        key_hash: &str,
    ) -> Result<Option<Agent>> {
        let agent_id = {
            let mut stmt = conn
                .prepare("SELECT id FROM agents WHERE client_key_hash = ? AND status = 'active'")?;
            let result = stmt.query_row(params![key_hash], |row| {
                let id_str: String = row.get(0)?;
                Ok(Uuid::parse_str(&id_str).unwrap_or_default())
            });
            match result {
                Ok(id) => Some(id),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            }
        };
        match agent_id {
            Some(id) => Ok(Some(Self::get_agent_sync(conn, id)?)),
            None => Ok(None),
        }
    }

    /// Cheap liveness probe for the database: runs a trivial query.
    pub async fn health_check(&self) -> Result<()> {
        self.spawn_read(|conn| {
            conn.query_row("SELECT 1", [], |_| Ok(()))?;
            Ok(())
        })
        .await
    }

    pub async fn get_agent(&self, id: Uuid) -> Result<Agent> {
        self.spawn_read(move |conn| Self::get_agent_sync(conn, id))
            .await
    }

    fn get_agent_sync(conn: &Connection, id: Uuid) -> Result<Agent> {
        let mut stmt = conn.prepare(
            "SELECT id, name, principal_type, public_key, did, owner_id, status, client_key_hash, created_at, updated_at
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
                client_key_hash: row.get(7)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
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

    pub async fn list_agents(&self) -> Result<Vec<Agent>> {
        self.spawn_read(Self::list_agents_sync).await
    }

    fn list_agents_sync(conn: &Connection) -> Result<Vec<Agent>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, principal_type, public_key, did, owner_id, status, client_key_hash, created_at, updated_at
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
                client_key_hash: row.get(7)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
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

    pub async fn create_principal(&self, req: &CreatePrincipalRequest) -> Result<Principal> {
        let req = req.clone();
        self.spawn_write(move |conn| Self::create_principal_sync(conn, &req))
            .await
    }

    fn create_principal_sync(
        conn: &mut Connection,
        req: &CreatePrincipalRequest,
    ) -> Result<Principal> {
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
        Self::get_principal_sync(conn, id)
    }

    pub async fn get_principal(&self, id: Uuid) -> Result<Principal> {
        self.spawn_read(move |conn| Self::get_principal_sync(conn, id))
            .await
    }

    fn get_principal_sync(conn: &Connection, id: Uuid) -> Result<Principal> {
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

    pub async fn get_principal_by_email(&self, email: &str) -> Option<Principal> {
        let email = email.to_string();
        self.spawn_read(move |conn| Ok(Self::get_principal_by_email_sync(conn, &email)))
            .await
            .unwrap_or(None)
    }

    fn get_principal_by_email_sync(conn: &Connection, email: &str) -> Option<Principal> {
        let mut stmt = conn
            .prepare(
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

    pub async fn create_audit_entry(&self, entry: &CreateAuditEntry) -> Result<AuditEntry> {
        let entry = entry.clone();
        self.spawn_write(move |conn| Self::create_audit_entry_sync(conn, &entry))
            .await
    }

    fn create_audit_entry_sync(
        conn: &mut Connection,
        entry: &CreateAuditEntry,
    ) -> Result<AuditEntry> {
        let prev_hash: String = conn
            .query_row(
                "SELECT row_hash FROM audit_log ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| {
                "0000000000000000000000000000000000000000000000000000000000000000".to_string()
            });

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
            dry_run: entry.dry_run,
            timestamp: now,
        };
        audit.row_hash = audit.compute_hash();

        conn.execute(
            "INSERT INTO audit_log (prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, dry_run, timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                audit.dry_run,
                audit.timestamp.to_rfc3339(),
            ],
        )?;

        audit.id = conn.last_insert_rowid();
        Ok(audit)
    }

    pub async fn list_audit_entries(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        self.spawn_read(move |conn| Self::list_audit_entries_sync(conn, limit))
            .await
    }

    fn list_audit_entries_sync(conn: &Connection, limit: i64) -> Result<Vec<AuditEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, dry_run, timestamp
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
                dry_run: row.get::<_, i64>(11)? != 0,
                timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
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

    /// Fetch the complete audit log in insertion order (ascending id).
    ///
    /// The hash-chain verifier needs every row from genesis onward — a
    /// truncated window would report spurious linkage breaks at its edges.
    pub async fn all_audit_entries(&self) -> Result<Vec<AuditEntry>> {
        self.spawn_read(Self::all_audit_entries_sync).await
    }

    fn all_audit_entries_sync(conn: &Connection) -> Result<Vec<AuditEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, dry_run, timestamp
             FROM audit_log ORDER BY id ASC",
        )?;
        let entries = stmt.query_map([], Self::audit_entry_from_row)?;
        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    fn audit_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
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
            dry_run: row.get::<_, i64>(11)? != 0,
            timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or(Utc::now()),
        })
    }
    // ── Policy management ──────────────────────────────────────────

    pub async fn load_active_policy_yaml(&self) -> Result<String> {
        self.spawn_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT definition FROM policies WHERE status = 'active' ORDER BY updated_at DESC LIMIT 1",
            )?;
            let yaml: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
            Ok(yaml.unwrap_or_default())
        })
        .await
    }

    pub async fn create_policy(&self, name: &str, engine: &str, definition: &str) -> Result<()> {
        let name = name.to_string();
        let engine = engine.to_string();
        let definition = definition.to_string();
        self.spawn_write(move |conn| {
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
        })
        .await
    }

    #[allow(clippy::type_complexity)]
    pub async fn list_policies(&self) -> Result<Vec<(Uuid, String, String, String, String)>> {
        self.spawn_read(|conn| {
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
        })
        .await
    }

    // ── Resource management ────────────────────────────────────────

    pub async fn create_resource(
        &self,
        req: &crate::resource::CreateResourceRequest,
    ) -> Result<crate::resource::Resource> {
        let req = req.clone();
        self.spawn_write(move |conn| Self::create_resource_sync(conn, &req))
            .await
    }

    fn create_resource_sync(
        conn: &mut Connection,
        req: &crate::resource::CreateResourceRequest,
    ) -> Result<crate::resource::Resource> {
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
        Self::get_resource_sync(conn, id)
    }

    pub async fn get_resource(&self, id: Uuid) -> Result<crate::resource::Resource> {
        self.spawn_read(move |conn| Self::get_resource_sync(conn, id))
            .await
    }

    fn get_resource_sync(conn: &Connection, id: Uuid) -> Result<crate::resource::Resource> {
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

    pub async fn list_resources(&self) -> Result<Vec<crate::resource::Resource>> {
        self.spawn_read(Self::list_resources_sync).await
    }

    fn list_resources_sync(conn: &Connection) -> Result<Vec<crate::resource::Resource>> {
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

    pub async fn find_resource_by_uri(&self, uri: &str) -> Result<Option<Uuid>> {
        let uri = uri.to_string();
        self.spawn_read(move |conn| {
            let mut stmt = conn.prepare("SELECT id FROM resources WHERE uri = ?")?;
            let id_str: Option<String> = stmt.query_row(params![uri], |row| row.get(0)).ok();
            Ok(id_str.and_then(|s| Uuid::parse_str(&s).ok()))
        })
        .await
    }

    pub async fn list_all_grants(&self) -> Result<Vec<Grant>> {
        self.spawn_read(Self::list_all_grants_sync).await
    }

    fn list_all_grants_sync(conn: &Connection) -> Result<Vec<Grant>> {
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
                revoked_at: revoked_at_str
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
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

    #[allow(clippy::too_many_arguments)]
    pub async fn create_grant(
        &self,
        agent_id: Uuid,
        principal_id: Uuid,
        parent_grant_id: Option<Uuid>,
        scopes: &[String],
        constraints: Option<&serde_json::Value>,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<Uuid> {
        let scopes = scopes.to_vec();
        let constraints = constraints.cloned();
        self.spawn_write(move |conn| {
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
                    serde_json::to_string(&scopes).unwrap_or_default(),
                    constraints.map(|v| v.to_string()),
                    expires_at.to_rfc3339(),
                    now,
                ],
            )?;
            Ok(id)
        })
        .await
    }

    pub async fn get_grant(&self, id: Uuid) -> Result<Grant> {
        self.spawn_read(move |conn| Self::get_grant_sync(conn, id))
            .await
    }

    fn get_grant_sync(conn: &Connection, id: Uuid) -> Result<Grant> {
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
                revoked_at: revoked_at_str
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
            })
        })?;
        Ok(grant)
    }

    pub async fn list_grants_for_agent(&self, agent_id: Uuid) -> Result<Vec<Grant>> {
        self.spawn_read(move |conn| Self::list_grants_for_agent_sync(conn, agent_id))
            .await
    }

    fn list_grants_for_agent_sync(conn: &Connection, agent_id: Uuid) -> Result<Vec<Grant>> {
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
                revoked_at: revoked_at_str
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
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

    pub async fn revoke_grant(&self, id: Uuid) -> Result<Vec<Uuid>> {
        self.spawn_write(move |conn| Self::revoke_grant_sync(conn, id))
            .await
    }

    fn revoke_grant_sync(conn: &mut Connection, id: Uuid) -> Result<Vec<Uuid>> {
        let mut revoked = Vec::new();
        {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE grants SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
                params![now, id.to_string()],
            )?;
            revoked.push(id);
            let children: Vec<String> = conn
                .prepare("SELECT id FROM grants WHERE parent_grant_id = ? AND revoked_at IS NULL")?
                .query_map(params![id.to_string()], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            for child_str in children {
                if let Ok(child_id) = Uuid::parse_str(&child_str) {
                    let mut child_revoked = Self::revoke_grant_sync(conn, child_id)?;
                    revoked.append(&mut child_revoked);
                }
            }
        }
        Ok(revoked)
    }

    // ── Token management ───────────────────────────────────────────

    pub async fn record_token(
        &self,
        jti: &str,
        grant_id: Option<Uuid>,
        agent_id: Uuid,
        scopes: &[String],
        audience: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let jti = jti.to_string();
        let scopes = scopes.to_vec();
        let audience = audience.to_string();
        self.spawn_write(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO tokens (id, grant_id, agent_id, scopes, audience, issued_at, expires_at, revoked, dpop_bound, key_thumbprint)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, NULL)",
                params![
                    jti,
                    grant_id.map(|u| u.to_string()),
                    agent_id.to_string(),
                    serde_json::to_string(&scopes).unwrap_or_default(),
                    audience,
                    now,
                    expires_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn revoke_token(&self, jti: &str) -> Result<()> {
        let jti = jti.to_string();
        self.spawn_write(move |conn| {
            conn.execute("UPDATE tokens SET revoked = 1 WHERE id = ?", params![jti])?;
            Ok(())
        })
        .await
    }

    pub async fn is_token_revoked(&self, jti: &str) -> Result<bool> {
        let jti = jti.to_string();
        self.spawn_read(move |conn| {
            let revoked: i64 = conn
                .query_row(
                    "SELECT revoked FROM tokens WHERE id = ?",
                    params![jti],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(revoked != 0)
        })
        .await
    }

    pub async fn revoke_agent_tokens(&self, agent_id: Uuid) -> Result<usize> {
        self.spawn_write(move |conn| {
            conn.execute(
                "UPDATE tokens SET revoked = 1 WHERE agent_id = ? AND revoked = 0",
                params![agent_id.to_string()],
            )?;
            Ok(conn.changes() as usize)
        })
        .await
    }

    // ── Approval management ────────────────────────────────────────

    pub async fn create_approval_request(
        &self,
        agent_id: Uuid,
        principal_id: Option<Uuid>,
        resource_id: Option<Uuid>,
        action: &str,
        requested_scopes: &[String],
        ttl_seconds: u64,
    ) -> Result<crate::approval::ApprovalRequest> {
        let action = action.to_string();
        let requested_scopes = requested_scopes.to_vec();
        self.spawn_write(move |conn| {
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
                    resource_id.map(|u| u.to_string()),
                    action,
                    serde_json::to_string(&requested_scopes).unwrap_or_default(),
                    expires_at.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )?;
            Self::get_approval_request_sync(conn, id)
        })
        .await
    }

    pub async fn get_approval_request(&self, id: Uuid) -> Result<crate::approval::ApprovalRequest> {
        self.spawn_read(move |conn| Self::get_approval_request_sync(conn, id))
            .await
    }

    fn get_approval_request_sync(
        conn: &Connection,
        id: Uuid,
    ) -> Result<crate::approval::ApprovalRequest> {
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, resource_id, action, requested_scopes, status, approver_id, reason, approval_token, expires_at, created_at, resolved_at
             FROM approval_requests WHERE id = ?",
        )?;
        let req = stmt.query_row(params![id.to_string()], |row| {
            let principal_id_str: Option<String> = row.get(2)?;
            let resource_id_str: Option<String> = row.get(3)?;
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
                resource_id: resource_id_str
                    .and_then(|s| Uuid::parse_str(&s).ok())
                    .unwrap_or_default(),
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
                resolved_at: resolved_at_str
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
            })
        })?;
        Ok(req)
    }

    pub async fn list_pending_approvals(&self) -> Result<Vec<crate::approval::ApprovalRequest>> {
        self.spawn_read(Self::list_pending_approvals_sync).await
    }

    fn list_pending_approvals_sync(
        conn: &Connection,
    ) -> Result<Vec<crate::approval::ApprovalRequest>> {
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, principal_id, resource_id, action, requested_scopes, status, approver_id, reason, approval_token, expires_at, created_at, resolved_at
             FROM approval_requests WHERE status = 'pending' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let principal_id_str: Option<String> = row.get(2)?;
            let resource_id_str: Option<String> = row.get(3)?;
            let approver_id_str: Option<String> = row.get(7)?;
            let scopes_str: String = row.get(5)?;
            let resolved_at_str: Option<String> = row.get(12)?;
            Ok(crate::approval::ApprovalRequest {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                agent_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                principal_id: principal_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                resource_id: resource_id_str
                    .and_then(|s| Uuid::parse_str(&s).ok())
                    .unwrap_or_default(),
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
                resolved_at: resolved_at_str
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub async fn resolve_approval_request(
        &self,
        id: Uuid,
        approver_id: Uuid,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<crate::approval::ApprovalRequest> {
        let reason = reason.map(|r| r.to_string());
        self.spawn_write(move |conn| {
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
                return Err(PatroclusError::Database(
                    "approval request not found or already resolved".to_string(),
                ));
            }
            Self::get_approval_request_sync(conn, id)
        })
        .await
    }

    // ── Vault credential management ────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub async fn store_vault_credential(
        &self,
        principal_id: Uuid,
        provider: &str,
        encrypted_token: &[u8],
        nonce: &[u8],
        encryption_key_id: &str,
        scopes: &[String],
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Uuid> {
        let provider = provider.to_string();
        let encrypted_token = encrypted_token.to_vec();
        let nonce = nonce.to_vec();
        let encryption_key_id = encryption_key_id.to_string();
        let scopes = scopes.to_vec();
        self.spawn_write(move |conn| {
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
                    serde_json::to_string(&scopes).unwrap_or_default(),
                    expires_at.map(|t| t.to_rfc3339()),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )?;
            Ok(id)
        })
        .await
    }

    pub async fn get_vault_credential(
        &self,
        principal_id: Uuid,
        provider: &str,
    ) -> Result<Option<VaultCredentialRecord>> {
        let provider = provider.to_string();
        self.spawn_read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, principal_id, provider, encrypted_token, nonce, encryption_key_id, scopes, expires_at, created_at, updated_at
                 FROM vault_credentials WHERE principal_id = ? AND provider = ? ORDER BY updated_at DESC LIMIT 1",
            )?;
            let result = stmt.query_row(params![principal_id.to_string(), provider], |row| {
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
                    expires_at: expires_str
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|d| d.with_timezone(&Utc)),
                    created_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(8)?,
                    )
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(9)?,
                    )
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(Utc::now()),
                })
            });
            match result {
                Ok(record) => Ok(Some(record)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
    }
}

/// Read every audit row in insertion order from an already-open connection.
///
/// Used by the `verify-chain` CLI, which opens the database read-only so
/// tamper inspection can never mutate evidence.
pub fn read_audit_entries_for_verification(conn: &rusqlite::Connection) -> Result<Vec<AuditEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, prev_hash, row_hash, agent_id, principal_id, action, resource, decision, reason, delegation_chain, token_jti, dry_run, timestamp
         FROM audit_log ORDER BY id ASC",
    )?;
    let entries = stmt.query_map([], |row| {
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
            dry_run: row.get::<_, i64>(11)? != 0,
            timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
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
