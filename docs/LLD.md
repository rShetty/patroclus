# Patroclus — Low-Level Design (LLD)

## 1. Module Architecture

```
src/
├── lib.rs                    # Module declarations
├── bin/patroclus.rs          # CLI entry (init, serve, generate-keys)
├── config/mod.rs             # TOML configuration structs
├── crypto/mod.rs             # KeyPair — RSA key generation/loading
├── db/
│   ├── mod.rs                # Database wrapper (SQLite + parking_lot::Mutex)
│   └── migrations.rs         # Schema (9 tables, 8 indexes)
├── errors/mod.rs             # PatroclusError enum + IntoResponse
├── identity/mod.rs           # Agent, Principal, AgentType, AgentStatus
├── resource/mod.rs           # Resource, ResourceType, Sensitivity
├── policy/
│   ├── mod.rs                # PolicyEngine trait, Decision, PolicyContext
│   └── yaml_engine.rs        # YamlEngine — rule matching + temporal conditions
├── session/mod.rs            # SessionState, SessionStore — rate limiting, trust decay
├── token/
│   ├── mod.rs                # AgentClaims, ActClaim, IssueTokenParams
│   ├── issuer.rs             # TokenIssuer — RS256 JWT minting
│   └── verifier.rs           # TokenVerifier — validation + revocation
├── gateway/mod.rs            # AccessRequest/Response, DelegateRequest/Response
├── approval/mod.rs           # ApprovalRequest, ApprovalStatus, ApprovalDecision
├── audit/mod.rs              # AuditEntry — hash-chained SHA-256
├── vault/
│   ├── mod.rs                # Vault — AES-256-GCM encrypt/decrypt
│   └── providers.rs          # GitHub/Google/Slack token exchange
└── api/
    ├── mod.rs                # Module declarations
    ├── server.rs             # Axum router setup
    ├── state.rs              # AppState — shared state with hot-reload
    └── routes.rs             # All HTTP route handlers
```

## 2. Database Schema

### 2.1 agents
```sql
CREATE TABLE agents (
    id TEXT PRIMARY KEY,          -- UUID v7
    name TEXT NOT NULL,
    principal_type TEXT NOT NULL, -- service | delegated | autonomous
    public_key TEXT,              -- Ed25519/RSA public key
    did TEXT,                     -- DID identifier
    owner_id TEXT,                -- FK → principals.id
    status TEXT NOT NULL DEFAULT 'active', -- active | suspended | decommissioned
    created_at TEXT NOT NULL,     -- RFC 3339
    updated_at TEXT NOT NULL
);
```

### 2.2 principals
```sql
CREATE TABLE principals (
    id TEXT PRIMARY KEY,
    external_id TEXT NOT NULL,    -- maps to IdP user ID
    idp_provider TEXT NOT NULL,   -- okta | azuread | google | local
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
```

### 2.3 resources
```sql
CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    resource_type TEXT NOT NULL,  -- mcp_server | api | database | cloud_service
    uri TEXT NOT NULL,
    actions TEXT NOT NULL,        -- JSON: available actions and required scopes
    sensitivity TEXT NOT NULL,    -- low | medium | high | critical
    owner_id TEXT,                -- FK → principals.id (for approvals)
    credential_config TEXT,       -- JSON: vault path, OAuth provider config
    created_at TEXT NOT NULL
);
```

### 2.4 policies
```sql
CREATE TABLE policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    engine TEXT NOT NULL DEFAULT 'yaml', -- yaml | opa | cedar
    definition TEXT NOT NULL,      -- policy content (YAML/Rego/Cedar)
    status TEXT NOT NULL DEFAULT 'active', -- active | deprecated
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### 2.5 grants
```sql
CREATE TABLE grants (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,        -- FK → agents.id
    principal_id TEXT NOT NULL,    -- FK → principals.id
    parent_grant_id TEXT,          -- FK → grants.id (self-referential for delegation)
    scopes TEXT NOT NULL,          -- JSON array of scope strings
    constraints TEXT,              -- JSON: max_amount, time_window, etc.
    expires_at TEXT NOT NULL,
    revocable INTEGER NOT NULL DEFAULT 1,
    revoked_at TEXT,               -- NULL if not revoked
    created_at TEXT NOT NULL
);
```

### 2.6 tokens
```sql
CREATE TABLE tokens (
    id TEXT PRIMARY KEY,           -- = jti (JWT ID)
    grant_id TEXT,                 -- FK → grants.id
    agent_id TEXT NOT NULL,
    scopes TEXT NOT NULL,          -- JSON array
    audience TEXT NOT NULL,        -- audience-bound (RFC 8707)
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    dpop_bound INTEGER NOT NULL DEFAULT 0,
    key_thumbprint TEXT            -- cnf.jkt if DPoP-bound
);
```

### 2.7 approval_requests
```sql
CREATE TABLE approval_requests (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    principal_id TEXT,
    resource_id TEXT NOT NULL,
    action TEXT NOT NULL,
    requested_scopes TEXT NOT NULL,  -- JSON array
    status TEXT NOT NULL DEFAULT 'pending', -- pending | approved | denied | expired
    approver_id TEXT,
    reason TEXT,
    approval_token TEXT,             -- single-use token issued on approval
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    resolved_at TEXT
);
```

### 2.8 audit_log
```sql
CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    prev_hash TEXT NOT NULL,        -- SHA-256 of previous row
    row_hash TEXT NOT NULL,         -- SHA-256 of this row's content
    agent_id TEXT NOT NULL,
    principal_id TEXT,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    decision TEXT NOT NULL,         -- allow | deny | require_approval
    reason TEXT NOT NULL,
    delegation_chain TEXT,          -- JSON
    token_jti TEXT,
    timestamp TEXT NOT NULL
);
```

### 2.9 vault_credentials
```sql
CREATE TABLE vault_credentials (
    id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    provider TEXT NOT NULL,         -- github | google | slack
    encrypted_token BLOB NOT NULL,  -- AES-256-GCM ciphertext
    nonce BLOB NOT NULL,            -- 12-byte GCM nonce
    encryption_key_id TEXT NOT NULL,
    scopes TEXT NOT NULL,           -- JSON array
    expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## 3. API Specification

### 3.1 Agent-Facing

| Method | Path | Purpose |
|---|---|---|
| POST | `/v1/agent/request-access` | Request access — issues token on ALLOW |
| POST | `/v1/agent/check` | Dry-run policy check (no token issued) |
| POST | `/v1/agent/delegate` | Sub-delegate narrowed scope to sub-agent |
| GET | `/v1/agent/approval-status/{id}` | Get approval request status |

### 3.2 Principal-Facing

| Method | Path | Purpose |
|---|---|---|
| POST | `/v1/principal/delegate` | Human delegates scoped permissions to agent |
| GET | `/v1/principal/grants` | List all grants |
| POST | `/v1/principal/grants/{id}/revoke` | Revoke grant (cascades to children) |
| GET | `/v1/principal/approvals` | List pending approval requests |
| POST | `/v1/principal/approvals/{id}/approve` | Approve a request |
| POST | `/v1/principal/approvals/{id}/deny` | Deny a request |

### 3.3 Admin

| Method | Path | Purpose |
|---|---|---|
| POST/GET | `/v1/admin/agents` | Create/list agents |
| GET | `/v1/admin/agents/{id}` | Get agent by ID |
| POST | `/v1/admin/principals` | Create principal |
| POST/GET | `/v1/admin/resources` | Create/list resources |
| POST/GET | `/v1/admin/policies` | Create/list policies (hot-reload) |
| GET | `/v1/admin/audit` | Get audit trail |
| POST | `/v1/admin/tokens/{jti}/revoke` | Revoke token by JTI |
| POST | `/v1/admin/agents/{id}/kill` | Kill switch — kill all sessions + revoke tokens |
| POST | `/v1/admin/agents/{id}/spend` | Record spend for budget tracking |

### 3.4 Vault

| Method | Path | Purpose |
|---|---|---|
| POST | `/v1/vault/credentials` | Store encrypted credential |
| POST | `/v1/vault/vend` | Exchange refresh token for scoped access token |
| POST | `/v1/vault/generate-key` | Generate vault encryption key |

### 3.5 Sessions

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/sessions` | List all active sessions |
| POST | `/v1/sessions/{id}/kill` | Kill specific session |

## 4. Policy Engine

### 4.1 YAML Rule Structure

```yaml
- name: rule-name
  agent_types: ["delegated", "autonomous"]  # optional: filter by agent type
  actions: ["read", "query"]                 # optional: filter by action
  resources: ["dev-*", "test-*"]             # optional: glob patterns
  scopes: ["db:*"]                           # optional: scope patterns
  decision: allow | deny | require_approval
  reason: "Human-readable explanation"
  
  # Temporal conditions (Phase 5):
  rate_limit_per_minute: 5                   # max calls per minute
  max_spend: 100.0                           # max cumulative spend in session
  min_trust_level: 0.5                       # minimum trust level (after decay)
  require_prior_action: "load_profile"       # required prior action in trajectory
  max_actions_in_session: 10                 # max total actions in session
  
  constraints:                               # embedded in issued token
    - key: max_rows
      value: 1000
    - key: time_window
      value: "weekdays 08:00-18:00 CST"
```

### 4.2 Evaluation Flow

```
1. For each rule in order:
   a. Check agent_type match
   b. Check action match
   c. Check resource pattern match (glob: *, prefix*, prefix-*, prefix:*, prefix/*)
   d. Check scope pattern match
   e. If all match → evaluate temporal conditions:
      - Check session killed flag
      - Check max_actions_in_session
      - Check max_spend (cumulative)
      - Check min_trust_level (after decay)
      - Check require_prior_action (in trajectory)
      - Check rate_limit_per_minute
   f. If temporal check fails → return Deny with reason
   g. Return decision (allow/deny/require_approval) with approved scopes

2. If no rule matches → return Deny "No matching policy found (default deny)"
```

### 4.3 Hot-Reload

Policy engine is wrapped in `Arc<RwLock<Arc<dyn PolicyEngine>>>`. When a new policy
is created via `POST /v1/admin/policies`, the engine is rebuilt from the active
policy in the DB and atomically swapped. No server restart required.

## 5. Session State

```rust
struct SessionState {
    session_id: String,
    agent_id: Uuid,
    principal_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    trajectory: Vec<TrajectoryEvent>,  // capped at 1000 entries
    actions_count: u64,
    spend_total: f64,
    tokens_used: u64,
    trust_level: f64,                  // starts at 1.0, decays after idle
    killed: bool,                      // set by kill switch
}
```

### Trust Decay
```rust
fn apply_trust_decay(&mut self, threshold_minutes: i64, decay_rate: f64) {
    let idle = minutes_since_last_activity();
    if idle > threshold {
        let periods = (idle - threshold) / threshold;
        trust_level = (1.0 - periods * decay_rate).max(0.0);
    }
}
```

### Rate Limiting
Sliding window per `(session_id, action, resource)` key. If count exceeds
`rate_limit_per_minute` in the current window, deny.

## 6. Token Lifecycle

```
Issue:
  1. Policy engine evaluates → ALLOW
  2. TokenIssuer.issue() creates JWT with:
     - sub: human principal (user:email)
     - act: agent identity + delegation chain
     - scope: approved scopes (subset of requested)
     - aud: target resource (RFC 8707 audience binding)
     - exp: now + default_ttl_seconds (900s = 15min)
     - jti: UUID v7 (unique, for revocation)
     - constraints: from policy rule
  3. Token recorded in DB (tokens table) for revocation tracking
  4. Token returned to agent

Verify:
  1. Decode JWT, verify RS256 signature against JWKS
  2. Check issuer, audience (if specified)
  3. Check expiry
  4. Check jti against in-memory revocation store
  5. Return AgentClaims

Revoke:
  1. POST /v1/admin/tokens/{jti}/revoke
  2. DB: UPDATE tokens SET revoked = 1
  3. In-memory: add jti to TokenVerifier's revocation set
  4. Subsequent verify() calls return RevokedToken error

Kill Switch:
  1. POST /v1/admin/agents/{id}/kill
  2. Kill all sessions for agent (set killed = true)
  3. Revoke all tokens for agent (DB + in-memory)
  4. Subsequent request_access() returns "session killed" denial
```

## 7. Audit Log Integrity

```
Entry N:
  prev_hash = SHA-256(entry N-1's row_hash)
  row_hash  = SHA-256(prev_hash + agent_id + principal_id + action +
                      resource + decision + reason + timestamp +
                      delegation_chain + token_jti)

First entry: prev_hash = "0000...0000" (64 zeros)

Verification: Recompute all hashes in sequence. Any tampering breaks the chain.
```

## 8. Credential Vault

### Encryption
```
Vault::new(key_material):
  1. Derive AES-256 key: SHA-256(key_material) → 32 bytes
  2. Store key_id = hex(first 8 bytes of key)

Vault::encrypt(plaintext):
  1. Generate random 12-byte nonce
  2. AES-256-GCM encrypt(nonce, plaintext)
  3. Return (ciphertext, nonce)

Vault::decrypt(ciphertext, nonce):
  1. AES-256-GCM decrypt(nonce, ciphertext)
  2. Return plaintext string
```

### Token Exchange (Vending)
```
1. Agent requests access → ALLOW → gets Patroclus JWT
2. Agent calls POST /v1/vault/vend with:
   - principal_id, provider, requested_scopes, agent_token_jti
3. Vault retrieves encrypted credential from DB
4. Vault decrypts → gets refresh_token
5. Provider.exchange_refresh(refresh_token, requested_scopes):
   - POST to provider's token_url
   - Returns scoped access_token
6. Return access_token to agent (never the refresh_token)
```

## 9. Concurrency Model

- SQLite access: `Arc<parking_lot::Mutex<Connection>>` — single writer, multiple readers via WAL mode
- Policy engine: `Arc<RwLock<Arc<dyn PolicyEngine>>>` — readers don't block each other, writer is exclusive but atomic
- Session store: `parking_lot::RwLock<HashMap<...>>` — concurrent reads, exclusive writes
- Token verifier revocation: `parking_lot::RwLock<HashSet<String>>` — concurrent reads for verify, exclusive for revoke

## 10. Error Handling

```rust
enum PatroclusError {
    AgentNotFound(String),           // → 404
    PrincipalNotFound(String),       // → 404
    ResourceNotFound(String),        // → 404
    PolicyDenied { reason },         // → 403
    ApprovalRequired { reason },     // → 403
    InvalidToken(String),            // → 401
    ExpiredToken,                    // → 401
    RevokedToken(String),            // → 401
    ScopeEscalation { requested, parent }, // → 400
    DelegationDepthExceeded { max, actual }, // → 400
    Vault(String),                   // → 500
    Database(String),                // → 500
    Config(String),                  // → 500
    Crypto(String),                  // → 500
    NotImplemented(String),          // → 500
}
```

All errors implement `IntoResponse` for Axum, returning JSON `{"error": "message"}` with appropriate HTTP status.
