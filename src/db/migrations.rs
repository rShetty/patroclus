use rusqlite::{Connection, Result};

pub fn run(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            principal_type TEXT NOT NULL,
            public_key TEXT,
            did TEXT,
            owner_id TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS principals (
            id TEXT PRIMARY KEY,
            external_id TEXT NOT NULL,
            idp_provider TEXT NOT NULL,
            email TEXT NOT NULL,
            display_name TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS resources (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            uri TEXT NOT NULL,
            actions TEXT NOT NULL,
            sensitivity TEXT NOT NULL DEFAULT 'medium',
            owner_id TEXT,
            credential_config TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS policies (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            engine TEXT NOT NULL DEFAULT 'yaml',
            definition TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS grants (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            principal_id TEXT NOT NULL,
            parent_grant_id TEXT,
            scopes TEXT NOT NULL,
            constraints TEXT,
            expires_at TEXT NOT NULL,
            revocable INTEGER NOT NULL DEFAULT 1,
            revoked_at TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (agent_id) REFERENCES agents(id),
            FOREIGN KEY (principal_id) REFERENCES principals(id),
            FOREIGN KEY (parent_grant_id) REFERENCES grants(id)
        );

        CREATE TABLE IF NOT EXISTS tokens (
            id TEXT PRIMARY KEY,
            grant_id TEXT,
            agent_id TEXT NOT NULL,
            scopes TEXT NOT NULL,
            audience TEXT NOT NULL,
            issued_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revoked INTEGER NOT NULL DEFAULT 0,
            dpop_bound INTEGER NOT NULL DEFAULT 0,
            key_thumbprint TEXT,
            FOREIGN KEY (agent_id) REFERENCES agents(id),
            FOREIGN KEY (grant_id) REFERENCES grants(id)
        );

        CREATE TABLE IF NOT EXISTS approval_requests (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            principal_id TEXT,
            resource_id TEXT,
            action TEXT NOT NULL,
            requested_scopes TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            approver_id TEXT,
            reason TEXT,
            approval_token TEXT,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            resolved_at TEXT,
            FOREIGN KEY (agent_id) REFERENCES agents(id)
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prev_hash TEXT NOT NULL,
            row_hash TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            principal_id TEXT,
            action TEXT NOT NULL,
            resource TEXT NOT NULL,
            decision TEXT NOT NULL,
            reason TEXT NOT NULL,
            delegation_chain TEXT,
            token_jti TEXT,
            dry_run INTEGER NOT NULL DEFAULT 0,
            timestamp TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS vault_credentials (
            id TEXT PRIMARY KEY,
            principal_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            encrypted_token BLOB NOT NULL,
            nonce BLOB NOT NULL DEFAULT x'',
            encryption_key_id TEXT NOT NULL,
            scopes TEXT NOT NULL,
            expires_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (principal_id) REFERENCES principals(id)
        );

        CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
        CREATE INDEX IF NOT EXISTS idx_grants_agent ON grants(agent_id);
        CREATE INDEX IF NOT EXISTS idx_grants_parent ON grants(parent_grant_id);
        CREATE INDEX IF NOT EXISTS idx_tokens_agent ON tokens(agent_id);
        CREATE INDEX IF NOT EXISTS idx_tokens_revoked ON tokens(revoked);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_approvals_status ON approval_requests(status);
        CREATE INDEX IF NOT EXISTS idx_vault_principal ON vault_credentials(principal_id);
        ",
    )?;

    // Column additions for existing databases. SQLite raises
    // "duplicate column name" when the column already exists — ignore it.
    let column_additions = [
        "ALTER TABLE agents ADD COLUMN client_key_hash TEXT",
        "ALTER TABLE audit_log ADD COLUMN dry_run INTEGER NOT NULL DEFAULT 0",
    ];
    for stmt in column_additions {
        if let Err(rusqlite::Error::SqliteFailure(err, _)) = conn.execute(stmt, []) {
            // SQLITE_ERROR(1) with extended code covering duplicate columns.
            if err.extended_code != rusqlite::ffi::SQLITE_CONSTRAINT
                && err.extended_code != rusqlite::ffi::SQLITE_ERROR
            {
                return Err(rusqlite::Error::SqliteFailure(err, None));
            }
        }
    }

    Ok(())
}
