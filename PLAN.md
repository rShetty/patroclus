# Agent Permission Infrastructure — Planning Document

## 1. Problem Statement

AI agents today need dynamic, scoped, time-bounded access to third-party tools, MCP servers, APIs, databases, and cloud resources. Current approaches rely on static API keys, long-lived tokens, and over-broad service accounts — none of which are appropriate for ephemeral, autonomous agents that:

- Need **just-in-time** access to specific resources for specific actions
- Act on **delegated behalf** of human users (with scoping)
- Have their **own identity** (not just shared credentials)
- Require **human approval** for sensitive or out-of-policy actions
- Need credentials that **expire automatically** (5–15 min) and are **revocable**
- May **delegate to sub-agents** with narrowed scope (monotonic attenuation)

The goal: build a robust, self-hostable infrastructure layer where agents can dynamically request access to resources, policies are evaluated in real-time, and either a short-lived scoped token is issued or a human approval workflow is triggered.

---

## 2. Landscape Review — Existing Systems & Open Source Projects

### 2.1 Delegated Authorization Protocols for Agents

| Project | Language | Key Idea | Delegation | Policy Engine | Token Format |
|---|---|---|---|---|---|
| **[Grantex](https://github.com/mishrasanjeev/grantex)** | TS/Python/Go | "OAuth for agents" — delegated auth protocol with scoped, revocable, time-limited authority | Multi-agent, monotonic narrowing, cascade revocation | Built-in + OPA + Cedar | JWT (RS256) with agent claims |
| **[Legant](https://github.com/legant-dev/legant)** | Go | RFC 8693 token exchange, composite `sub`/`act` tokens, offline constraint enforcement | Multi-hop, monotonic attenuation | Embedded constraint PDP | JWT (RS256) with `act` chain |
| **[Authplane](https://github.com/authplane/authserver)** | Go | OAuth 2.1 AS specifically for MCP, federation to existing IdPs | Agent-to-agent with `act` chains | External (you keep access policy) | OAuth 2.1 tokens |
| **[AIP](https://github.com/sunilp/aip)** | Python | Agent Identity Protocol — Ed25519 + Biscuit, IETF draft | Chained tokens, scope attenuation | Datalog (Biscuit) | JWT + Biscuit |
| **[APOA](https://github.com/agenticpoa/apoa-mcp)** | TypeScript | Per-tool-call scoping for MCP servers | Delegation chains with attenuation | Hard/soft rules engine | JWT |
| **[Agent Passport System](https://github.com/aeoess/agent-passport-system)** | TypeScript | Open protocol for agent accountability — signed receipts for every action | Scoped, monotonic narrowing, cascade revocation | Embedded policy engine | Ed25519 + PASETO |

### 2.2 Control Planes & Gateways

| Project | Language | Key Idea | JIT Access | Approval Workflow | Audit |
|---|---|---|---|---|---|
| **[Agent Zero](https://github.com/casepilot/agent-zero)** | Python/TS | JIT access broker for AWS — LLM reviewer + deterministic validator, STS AssumeRole with inline session policy | Yes — per-request scoped STS credentials | Human approval for sensitive | CloudTrail |
| **[P0](https://p0.dev)** | SaaS | AuthZ control plane — MCP tool filtering, data-layer access, JIT with human approval | Yes — "requestable" resources | Yes — routed to human approvers | Yes |
| **[Permit.io MCP Gateway](https://www.permit.io)** | SaaS | Fine-grained authz (RBAC/ABAC/ReBAC) as MCP proxy, OAuth proxy + vault pattern | Token brokering | Consent service | Yes |
| **[OpenAgent-Control](https://pypi.org/project/openagent-control/)** | Python | MCP proxy with OPA policy, SPIFFE identity, token exchange, signed receipts | Credential brokering on ALLOW | Not in v1 | Ed25519 hash-chained |
| **[Agent-Safe](https://github.com/sahb4k/agent-safe)** | Python | Governance layer — action registry, PDP, execution tickets, credential gating | Yes — signed execution tickets → JIT credentials | Yes — webhook/Slack notifications | Hash-chained JSON |
| **[Agent Control Plane](https://github.com/ryanwi/agent-control-plane)** | Python | Policy engine, approval gates, budget tracking, kill switches, event sourcing | Budget-gated | Yes — `ApprovalGate` with scoped tickets | Event store (durable) |
| **[OpenLeash](https://github.com/openleash/openleash)** | TypeScript | Local authorization layer — YAML policies, `ALLOW`/`DENY`/`REQUIRE_APPROVAL`/`REQUIRE_STEP_UP` | Short-lived PASETO proof tokens | Yes — owner portal approval | Signed receipts |

### 2.3 MCP-Authorization-Specific Tools

| Project | Key Idea |
|---|---|
| **[mcp-authz](https://github.com/soumyasagiri/mcp-authz)** | 3-layer proxy: delegation chain validation (RFC 8693), tool policy engine (OPA), anomaly detection |
| **[Charon](https://github.com/NinadRao0707/charon)** | SPIFFE-based control plane — JWT-SVIDs, per-tool MCP authz, DPoP, reaper for idle agents |

### 2.4 Enterprise Frameworks & Vendor Approaches

| Project | Key Idea |
|---|---|
| **[Microsoft Agent Governance Toolkit](https://github.com/microsoft/agent-governance-toolkit)** | Full-stack governance: policy engine (YAML/OPA/Cedar), DID identity, execution rings, kill switch, OWASP Top 10 coverage |
| **[Alibaba Open Agent Auth](https://github.com/alibaba/open-agent-auth)** | Three-layer identity binding (ID Token → WIT → AOAT), WIMSE, OPA/RAM/ACL, W3C VC audit trails |
| **[Cloudflare AAM](https://blog.cloudflare.com/the-agent-access-model/)** | Agent Access Model — task-scoped access engine, capability ceilings, trust ratchet, harness + network enforcement |
| **[Amazon Bedrock AgentCore](https://aws.amazon.com/blogs/machine-learning/securing-ai-agents-with-temporal-policies-in-amazon-bedrock-agentcore/)** | Temporal policies — trajectory-aware authorization, workflow sequencing, cumulative budget caps, progressive trust decay |
| **[1Password](https://1password.com/blog/ai-agent-identity-architectures)** | Agent identity architectures (delegated/bounded/autonomous), Workload Identity Broker, RFC 8693, continuous credential rotation |

### 2.5 Research & Standards

| Paper/Standard | Key Idea |
|---|---|
| **[PAuth (Microsoft Research)](https://www.microsoft.com/en-us/research/publication/pauth-precise-task-scoped-authorization-for-agents/)** | Precise task-scoped implicit authorization — NL tasks implicitly authorize only concrete operations needed |
| **[PortAuth (Kyndryl)](https://github.com/kyndryl-open-source/aiagent-portable-authorization)** | Policy-embedded credentials — signed credentials with machine-evaluable constraints, offline verification |
| **MCP Authorization Spec (2025-11-25)** | OAuth 2.1 mandatory, resource indicators (RFC 8707), DCR (RFC 7591), audience-bound tokens, no passthrough |
| **IETF Draft: Agent Operation Authorization** | Three-layer binding: user identity → workload identity → authorization token |
| **RFC 8693 (Token Exchange)** | Delegated identity — composite `sub`/`act` tokens for on-behalf-of flows |
| **SPIFFE/SPIRE** | Workload identity framework — JWT-SVIDs for service-to-service auth |

---

## 3. Key Patterns Extracted

From the landscape review, these patterns are universal across mature implementations:

### 3.1 Identity
- **Agent has its own identity** — DID, SPIFFE SVID, or Ed25519 keypair
- **Delegation chain is explicit** — `sub` (human) + `act` (agent chain) in JWT
- **Identity is cryptographically verifiable** — JWKS published, offline verification

### 3.2 Scoping & Attenuation
- **Per-operation scoping** not per-agent scoping (e.g., `calendar:create_event` not `calendar:all`)
- **Monotonic attenuation** — sub-agent scopes must be strict subset of parent
- **Sub-agent expiry** = `min(parent_expiry, requested_expiry)`
- **Cascade revocation** — revoking root grant cascades to all descendants

### 3.3 Token Lifecycle
- **Short-lived by default** — 5–15 minute TTL
- **Just-in-time issuance** — credential for specific operation injected only when needed
- **Event-driven rotation** — on deployment, scope change, capability expansion
- **No long-lived credentials on agent workloads**

### 3.4 Policy Evaluation
- **Default-deny** — if no policy matches, DENY
- **Per-call evaluation** — every tool call checked against policy in real-time
- **Trajectory-aware** (temporal policies) — consider prior actions in session
- **Context-aware** — action + target + caller + time + environment
- **Policy engines**: OPA/Rego, Cedar, YAML rules, embedded engines

### 3.5 Approval Workflows
- **Three-state decisions**: `ALLOW`, `DENY`, `REQUIRE_APPROVAL`
- **Step-up authorization** — 403 with `WWW-Authenticate` identifying required scope
- **Human-in-the-loop** — approval routed to resource owner or admin
- **Approval tokens** — single-use, action-scoped, time-limited
- **Progressive trust decay** — auto-tighten permissions after inactivity

### 3.6 Enforcement Points
- **Harness/gateway interception** — tool calls intercepted before execution
- **Network-layer controls** — egress filtering, destination restrictions
- **Token containment** — don't pass agent tokens to upstream APIs; obtain fresh credentials
- **Per-tool `tools/list` filtering** — only show tools the credential can invoke

### 3.7 Audit
- **Hash-chained, tamper-evident** audit logs
- **Every decision logged** — ALLOW and DENY with full context
- **Attribution-complete** — every action traces to workload identity → human or system authority
- **Delegation chain in the log** — not reconstructed after the fact

### 3.8 Credential Brokering
- **Agents never hold target credentials** — gateway/vault brokers JIT credentials
- **Upstream tokens vaulted** — encrypted at rest, fresh tokens vended per request
- **Execution tickets** — signed, single-use tokens that bridge advisory to enforceable authz

---

## 4. Proposed Architecture

### 4.1 High-Level Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                     AGENT PERMISSION INFRASTRUCTURE                  │
│                                                                      │
│  ┌──────────┐   ┌──────────────┐   ┌─────────────┐   ┌──────────┐ │
│  │  Agent    │──▶│  Request     │──▶│  Policy     │──▶│ Decision │ │
│  │  Runtime  │   │  Gateway     │   │  Engine     │   │  Router  │ │
│  └──────────┘   └──────────────┘   └─────────────┘   └─────┬────┘ │
│                       │                                      │      │
│                       │              ┌────────────┐    ┌─────▼────┐ │
│                       │              │  Approval  │    │  Token   │ │
│                       │              │  Service   │◀──│  Issuer  │ │
│                       │              └──────┬─────┘    └─────┬────┘ │
│                       │                     │                │      │
│  ┌──────────┐    ┌────▼─────┐   ┌───────────┴──┐    ┌───────▼────┐│
│  │  Human   │◀───│  Notify  │   │  Audit Log    │   │  Credential││
│  │  Approver│───▶│  Service │   │  (Hash-chain) │   │  Vault     ││
│  └──────────┘    └──────────┘   └──────────────┘   └────────────┘│
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    Policy Store (YAML/Rego/Cedar)             │  │
│  └──────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    Resource Registry                           │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.2 Component Details

#### 4.2.1 Request Gateway
- Intercepts every agent tool call / resource access request
- Authenticates agent identity (JWT/DID/SPIFFE verification)
- Extracts: agent_id, human_principal (delegation chain), action, resource, context
- Forwards to Policy Engine for decision
- Enforces decision: allow, deny, or route to approval

**Inspired by**: mcp-authz (3-layer proxy), OpenAgent-Control (MCP proxy), Permit.io (MCP gateway)

#### 4.2.2 Policy Engine
- Evaluates `(agent, human_principal, action, resource, context)` → `ALLOW | DENY | REQUIRE_APPROVAL`
- Supports pluggable backends: built-in YAML rules, OPA/Rego, Cedar
- Trajectory-aware: considers prior actions in session (temporal policies)
- Default-deny: no matching policy = DENY
- Stateless for horizontal scaling (state in external stores)

**Inspired by**: Microsoft AGT (policy engine), Amazon Bedrock AgentCore (temporal policies), Agent-Safe (PDP)

#### 4.2.3 Decision Router
- On `ALLOW`: triggers Token Issuer to mint scoped, short-lived credential
- On `DENY`: returns denial with reason, logs to audit
- On `REQUIRE_APPROVAL`: creates approval request, notifies human approver, pauses agent
- Supports step-up authorization: return required scope, let agent request escalation

**Inspired by**: OpenLeash (decision states), P0 (requestable resources), Agent Zero (LLM reviewer)

#### 4.2.4 Token Issuer
- Mints short-lived JWTs (5–15 min TTL) with:
  - `sub`: human principal (who delegated)
  - `act`: agent identity + delegation chain
  - `scope`: approved scopes (subset of what was requested)
  - `aud`: target resource/server (audience-bound, RFC 8707)
  - `exp`: short expiry
  - `jti`: unique token ID for replay protection + revocation
- Supports DPoP binding (RFC 9449) for proof-of-possession
- Revocation: immediate (introspection) or signed revocation feed (offline verification)
- Cascade revocation: revoking parent invalidates all child tokens

**Inspired by**: Grantex (JWT with agent claims), Legant (RFC 8693 + constraints), Charon (JWT-SVID + DPoP)

#### 4.2.5 Credential Vault
- Stores upstream provider credentials (GitHub, Slack, Google, AWS, etc.) encrypted at rest
- Vends fresh, scoped access tokens via RFC 8693 token exchange
- Agents never see raw upstream credentials
- Per-user, per-agent, per-resource consent enforced at every vend

**Inspired by**: Authplane (upstream provider vaulting), Permit.io (OAuth proxy + vault), Agent-Safe (credential gating)

#### 4.2.6 Approval Service
- Manages approval lifecycle: request → review → approve/deny → notify
- Routes approvals to correct human: resource owner, service admin, or delegated approver
- Approval tokens: single-use, action-scoped, time-limited
- Notification channels: webhook, Slack, email, in-app dashboard
- Approval policies: who can approve what, quorum requirements, escalation rules

**Inspired by**: OpenLeash (approval workflow), P0 (JIT with human approvers), Agent Control Plane (ApprovalGate)

#### 4.2.7 Audit Log
- Hash-chained, tamper-evident log of every decision
- Captures: agent_id, human_principal, action, resource, decision, reason, timestamp, delegation chain
- Attribution-complete: every action → workload identity → human/system authority
- Queryable for compliance reports, anomaly detection, policy review
- Exportable for SIEM integration

**Inspired by**: APOA (hash-chained audit), OpenAgent-Control (Ed25519 signed receipts), Microsoft AGT (Merkle audit)

#### 4.2.8 Policy Store
- Versioned policy definitions (YAML/Rego/Cedar)
- Policies define: who (agent/human), what (action), where (resource), when (time/context), how (conditions)
- Policy templates for common agent task patterns
- Git-synced for review and rollback
- Hot-reloadable without restart

**Inspired by**: Agent-Safe (YAML policies), Microsoft AGT (YAML/OPA/Cedar), Grantex (OPA/Cedar backends)

#### 4.2.9 Resource Registry
- Catalog of all protected resources (APIs, MCP servers, databases, cloud services)
- Each resource declares: available actions, required scopes, sensitivity level, owner/approver
- Resource-specific credential brokering configs (OAuth provider, vault path, exchange flow)
- Dynamic discovery: agents can query what's available and what scopes they need

**Inspired by**: Agent Zero (resource catalog), Agent-Safe (target inventory), Cloudflare AAM (task templates)

---

## 5. Core Flows

### 5.1 Flow 1: Agent Requests Access to a Resource

```
Agent ──"I need to read from Postgres prod-db, table users"──▶ Request Gateway
                                                                    │
                                    ┌───────────────────────────────┘
                                    ▼
                             Policy Engine
                    Evaluate: agent_001, human=alice,
                    action=read, resource=prod-db/users,
                    context={session_id, task="reconcile"}
                                    │
                         ┌──────────┴──────────┐
                         │                     │
                    ALLOW                REQUIRE_APPROVAL
                         │                     │
                         ▼                     ▼
                   Token Issuer         Approval Service
                    Mint JWT:           Create request, notify
                    - sub: alice        db_owner@company.com
                    - act: agent_001         │
                    - scope: db:read         │ (human approves)
                    - aud: prod-db           │
                    - exp: +15min            ▼
                    - jti: uuid         Token Issuer (mint)
                         │                     │
                         ▼                     ▼
                   Credential Vault    Credential Vault
                    Exchange JWT for    Exchange JWT for
                    scoped DB creds     scoped DB creds
                         │                     │
                         ▼                     ▼
                    Agent gets           Agent gets
                    short-lived          short-lived
                    DB credentials       DB credentials
                         │                     │
                         ▼                     ▼
                    Audit Log ◀──── both decisions logged ────▶
```

### 5.2 Flow 2: Human Delegates Permissions to Agent

```
Human (Alice) ──"I authorize agent_001 to manage my calendar for 1 hour"──▶ Approval Service
                                                                              │
                                                                    ┌─────────┘
                                                                    ▼
                                                             Scope Selector
                                                  Alice narrows scope to:
                                                  - calendar:create_event
                                                  - calendar:read
                                                  (NOT calendar:delete)
                                                                    │
                                                                    ▼
                                                             Token Issuer
                                                  Mint delegation grant:
                                                  - sub: alice
                                                  - act: agent_001
                                                  - scope: [calendar:create_event, calendar:read]
                                                  - exp: +1hour
                                                  - delegable: true
                                                                    │
                                                                    ▼
                                                  Agent receives scoped
                                                  delegation token. Can
                                                  only do what Alice allowed.
                                                  Can sub-delegate narrower.
```

### 5.3 Flow 3: Multi-Agent Delegation (Monotonic Attenuation)

```
Root Agent (agent_001, scopes=[db:read, db:write, email:send])
  │
  ├─▶ Sub-Agent A (agent_002)
  │    scopes = [db:read]           ← subset of parent ✓
  │    expiry = min(parent_exp, +10min)
  │
  ├─▶ Sub-Agent B (agent_003)
  │    scopes = [db:read, db:write]  ← subset of parent ✓
  │    expiry = min(parent_exp, +5min)
  │
  └─▶ Sub-Agent C tries [db:read, email:send, admin:all]
       scopes ⊄ parent scopes        ← REJECTED (scope escalation)

Revoking agent_001 → cascades to agent_002, agent_003 atomically.
```

---

## 6. Technology Choices

### 6.1 Language: Go

**Rationale:**
- Single binary deployment (like Legant, Authplane, Charon)
- Excellent crypto/JWT libraries
- Strong concurrency model for gateway/proxy workload
- Good ecosystem for policy evaluation (OPA is Go-native)
- Easy to containerize and deploy as sidecar

### 6.2 Token Format: JWT (RS256) + Optional DPoP

**Rationale:**
- Universal verification (any service can verify via JWKS)
- Standard claims (`sub`, `act`, `scope`, `aud`, `exp`, `jti`)
- RFC 8693 token exchange for delegation
- RFC 9449 DPoP for proof-of-possession (stolen token useless without key)
- Aligns with MCP OAuth 2.1 spec

### 6.3 Policy Engine: Pluggable (built-in YAML + OPA + Cedar)

**Rationale:**
- YAML for simple use cases (no external dependency)
- OPA/Rego for complex policy logic (industry standard)
- Cedar for AWS-integrated environments
- All three proven in production by existing projects

### 6.4 Storage: PostgreSQL + Redis

**Rationale:**
- PostgreSQL: policy store, audit log, approval state, resource registry
- Redis: token cache, replay protection (jti store), rate limiting
- Both battle-tested, widely deployed

### 6.5 Protocol Surface: MCP + REST API

**Rationale:**
- MCP gateway for tool-call interception (industry standard)
- REST API for agent SDKs, admin dashboard, approval workflows
- Well-known OAuth endpoints for discovery and interoperability

---

## 7. Proposed Data Model

### 7.1 Core Entities

```sql
-- Agent identity registration
agents (
  id              UUID PRIMARY KEY,
  name            VARCHAR(255),
  principal_type  VARCHAR(50),   -- 'service' | 'delegated' | 'autonomous'
  public_key      TEXT,           -- Ed25519/RSA public key
  did             VARCHAR(255),   -- DID identifier
  owner_id        UUID,           -- human or org that owns this agent
  status          VARCHAR(50),    -- 'active' | 'suspended' | 'decommissioned'
  created_at      TIMESTAMP,
  updated_at      TIMESTAMP
)

-- Human principals (delegators)
principals (
  id              UUID PRIMARY KEY,
  external_id     VARCHAR(255),   -- maps to IdP user ID
  idp_provider    VARCHAR(100),   -- 'okta' | 'azuread' | 'google' | 'local'
  email           VARCHAR(255),
  display_name    VARCHAR(255),
  created_at      TIMESTAMP
)

-- Protected resources
resources (
  id              UUID PRIMARY KEY,
  name            VARCHAR(255),
  type            VARCHAR(50),    -- 'mcp_server' | 'api' | 'database' | 'cloud_service'
  uri             TEXT,           -- resource identifier / endpoint
  actions         JSONB,          -- available actions and required scopes
  sensitivity     VARCHAR(50),    -- 'low' | 'medium' | 'high' | 'critical'
  owner_id        UUID,           -- resource owner (for approvals)
  credential_config JSONB,        -- vault path, OAuth provider, exchange flow
  created_at      TIMESTAMP
)

-- Policies (versioned)
policies (
  id              UUID PRIMARY KEY,
  name            VARCHAR(255),
  version         INTEGER,
  engine          VARCHAR(50),    -- 'yaml' | 'opa' | 'cedar'
  definition      TEXT,           -- policy content
  status          VARCHAR(50),    -- 'active' | 'draft' | 'deprecated'
  created_at      TIMESTAMP,
  updated_at      TIMESTAMP
)

-- Active delegation grants
grants (
  id              UUID PRIMARY KEY,
  agent_id        UUID REFERENCES agents(id),
  principal_id    UUID REFERENCES principals(id),
  parent_grant_id UUID,           -- for sub-delegation chains
  scopes          JSONB,          -- approved scopes
  constraints     JSONB,          -- max_amount, time_window, resource_paths, etc.
  expires_at      TIMESTAMP,
  revocable       BOOLEAN DEFAULT TRUE,
  revoked_at      TIMESTAMP,
  created_at      TIMESTAMP
)

-- Issued tokens (for revocation tracking)
tokens (
  id              UUID PRIMARY KEY,  -- = jti
  grant_id        UUID REFERENCES grants(id),
  agent_id        UUID REFERENCES agents(id),
  scopes          JSONB,
  audience        TEXT,
  issued_at       TIMESTAMP,
  expires_at      TIMESTAMP,
  revoked         BOOLEAN DEFAULT FALSE,
  dpop_bound      BOOLEAN DEFAULT FALSE,
  key_thumbprint  VARCHAR(255)    -- cnf.jkt if DPoP-bound
)

-- Approval requests
approval_requests (
  id              UUID PRIMARY KEY,
  agent_id        UUID REFERENCES agents(id),
  principal_id    UUID REFERENCES principals(id),
  resource_id     UUID REFERENCES resources(id),
  action          VARCHAR(255),
  requested_scopes JSONB,
  status          VARCHAR(50),    -- 'pending' | 'approved' | 'denied' | 'expired'
  approver_id     UUID,           -- who approved/denied
  reason          TEXT,
  approval_token_id UUID,         -- single-use token issued on approval
  expires_at      TIMESTAMP,      -- approval request expiry
  created_at      TIMESTAMP,
  resolved_at     TIMESTAMP
)

-- Audit log (hash-chained)
audit_log (
  id              BIGSERIAL PRIMARY KEY,
  prev_hash       VARCHAR(64),    -- SHA-256 of previous row
  row_hash        VARCHAR(64),    -- SHA-256 of this row's content
  agent_id        UUID,
  principal_id    UUID,
  action          VARCHAR(255),
  resource_id     UUID,
  decision        VARCHAR(50),    -- 'allow' | 'deny' | 'require_approval'
  reason          TEXT,
  delegation_chain JSONB,         -- full act chain
  token_jti       VARCHAR(255),
  timestamp       TIMESTAMP
)

-- Credential vault (encrypted upstream credentials)
vault_credentials (
  id              UUID PRIMARY KEY,
  principal_id    UUID REFERENCES principals(id),
  provider        VARCHAR(100),   -- 'github' | 'google' | 'slack' | 'aws' etc.
  encrypted_token BYTEA,          -- encrypted refresh token
  encryption_key_id VARCHAR(255), -- KMS key ID used
  scopes          JSONB,          -- scopes granted by user
  expires_at      TIMESTAMP,
  created_at      TIMESTAMP,
  updated_at      TIMESTAMP
)
```

### 7.2 JWT Token Structure

```json
{
  "header": {
    "alg": "RS256",
    "kid": "key-2026-01",
    "typ": "JWT"
  },
  "payload": {
    "iss": "https://auth.infra.example.com",
    "sub": "user:alice@example.com",
    "act": {
      "sub": "agent:agent_001",
      "delegation_chain": [
        {"sub": "user:alice@example.com", "act": "agent:agent_001"}
      ],
      "delegation_depth": 0
    },
    "scope": "db:read:prod-db/users calendar:create_event",
    "aud": "resource:prod-db",
    "exp": 1723643820,
    "iat": 1723642920,
    "jti": "01J5Q3Z...",
    "constraints": {
      "max_rows": 1000,
      "time_window": "weekdays 08:00-18:00 CST",
      "resource_paths": ["/users/*", "/orders/*"]
    },
    "cnf": {
      "jkt": "0ZcOCORZ..."  // DPoP key thumbprint (if bound)
    }
  }
}
```

---

## 8. API Surface

### 8.1 Agent-Facing API

```
POST /v1/agent/request-access
  Body: { agent_id, action, resource, requested_scopes, context, delegation_token? }
  Response: {
    decision: "allow" | "deny" | "require_approval",
    token?: { jwt, expires_at, scopes },
    approval?: { request_id, status, approver_notified },
    reason?: string
  }

POST /v1/agent/delegate
  Body: { parent_grant_token, sub_agent_id, scopes, expires_in }
  Response: { delegated_token, expires_at }
  // Enforces: scopes ⊆ parent scopes, expiry ≤ parent expiry

GET /v1/agent/approval-status/:request_id
  Response: { status, token?, reason? }

POST /v1/agent/check
  Body: { agent_id, action, resource, scopes }
  Response: { allowed: bool, required_scopes?: [], requires_approval?: bool }
  // Dry-run check — no token issued
```

### 8.2 Human/Principal-Facing API

```
POST /v1/principal/delegate
  Body: { agent_id, scopes, constraints, expires_in }
  Response: { grant_id, delegation_token }

GET /v1/principal/grants
  Response: [{ grant_id, agent_id, scopes, status, expires_at }]

DELETE /v1/principal/grants/:grant_id
  // Revokes grant + cascades to all child grants + invalidates tokens

GET /v1/principal/approvals
  Response: [{ request_id, agent_id, action, resource, status }]

POST /v1/principal/approvals/:request_id/approve
POST /v1/principal/approvals/:request_id/deny
```

### 8.3 Admin/Policy API

```
POST   /v1/admin/policies
GET    /v1/admin/policies
PUT    /v1/admin/policies/:id
DELETE /v1/admin/policies/:id

POST   /v1/admin/resources
GET    /v1/admin/resources
PUT    /v1/admin/resources/:id

POST   /v1/admin/agents
GET    /v1/admin/agents
DELETE /v1/admin/agents/:id    // decommission

POST   /v1/admin/credentials/vault
  // Store upstream provider credential (encrypted)
  Body: { principal_id, provider, refresh_token, scopes }

GET    /v1/admin/audit
  Query: ?agent_id=&from=&to=&decision=
```

### 8.4 MCP Gateway (Proxy)

```
POST /mcp/:server_id/tools/list
  // Returns only tools the agent's credential can invoke (filtered)

POST /mcp/:server_id/tools/call
  // Intercepts, authorizes per-tool, strips token, forwards to upstream
  // On DENY: returns 403 with reason
  // On REQUIRE_APPROVAL: returns 403 with approval request info
```

### 8.5 OAuth 2.1 / Well-Known Endpoints

```
GET  /.well-known/oauth-authorization-server    (RFC 8414)
GET  /.well-known/jwks.json                      (signing keys)
POST /oauth/token                                 (RFC 8693 token exchange)
POST /oauth/revoke
POST /oauth/introspect
POST /oauth/register                              (RFC 7591 DCR)
```

---

## 9. Implementation Phases

### Phase 1: Core (Weeks 1–4)
**Goal**: Agent can request access, policy evaluates, token is issued or denied.

- [ ] Agent identity registration + JWT issuance/verification
- [ ] Policy engine (YAML rules only, built-in)
- [ ] Request gateway (REST API, no MCP yet)
- [ ] Token issuer (short-lived JWT with `sub`/`act`/`scope`/`aud`)
- [ ] Basic audit log (hash-chained)
- [ ] Resource registry (CRUD)
- [ ] Policy store (CRUD, hot-reload)

**Deliverable**: An agent can call `POST /v1/agent/request-access` and receive a scoped JWT or denial.

### Phase 2: Delegation & Approval (Weeks 5–8)
**Goal**: Humans can delegate scoped permissions; approvals work end-to-end.

- [ ] Delegation flow (`POST /v1/agent/delegate`) with monotonic attenuation
- [ ] Cascade revocation
- [ ] Approval service (create, notify, approve/deny)
- [ ] Notification service (webhook + email)
- [ ] Human-facing delegation API (`POST /v1/principal/delegate`)
- [ ] Approval token issuance (single-use, action-scoped)
- [ ] Step-up authorization (return required scope on 403)

**Deliverable**: Alice can delegate `calendar:read` to agent_001 for 1 hour. Agent can sub-delegate narrower scope. Sensitive actions trigger human approval.

### Phase 3: MCP Gateway (Weeks 9–12)
**Goal**: MCP tool calls are intercepted, authorized, and filtered.

- [ ] MCP proxy gateway (intercepts `tools/call` and `tools/list`)
- [ ] Per-tool authorization (tool name → scope mapping)
- [ ] `tools/list` filtering (only show authorized tools)
- [ ] Token stripping (agent token never reaches upstream MCP server)
- [ ] MCP OAuth 2.1 compliance (discovery, DCR, resource indicators)
- [ ] SSE streaming support

**Deliverable**: An MCP client connects through the gateway. Only authorized tools are visible. Unauthorized calls are blocked before reaching the MCP server.

### Phase 4: Credential Vault & Brokering (Weeks 13–16)
**Goal**: Agents get JIT credentials for upstream providers without ever seeing raw secrets.

- [ ] Encrypted credential storage (AES-256 at rest)
- [ ] OAuth provider integrations (GitHub, Google, Slack, AWS STS)
- [ ] RFC 8693 token exchange (exchange agent JWT for provider-scoped token)
- [ ] Credential vending on ALLOW decision
- [ ] Per-user consent enforcement at vend time
- [ ] DPoP binding support (RFC 9449)

**Deliverable**: Agent requests access to GitHub repos. Gateway checks policy, mints agent JWT, vault exchanges it for a scoped GitHub token, agent uses it for 15 minutes.

### Phase 5: Advanced Policies & Hardening (Weeks 17–20)
**Goal**: Production-grade policy evaluation and security hardening.

- [ ] OPA/Rego policy backend
- [ ] Cedar policy backend
- [ ] Temporal/trajectory-aware policies (session state, prior actions)
- [ ] Rate limiting per agent/principal/resource
- [ ] Budget tracking (spend caps, token usage caps)
- [ ] Progressive trust decay (auto-tighten after inactivity)
- [ ] Anomaly detection (behavioral baseline deviation)
- [ ] Replay protection (jti tracking with Redis)
- [ ] Kill switch (emergency agent termination)
- [ ] Signed revocation feed (offline verification)

**Deliverable**: Full production-grade authorization infrastructure with temporal policies, anomaly detection, and kill switches.

### Phase 6: SDKs & Developer Experience (Weeks 21–24)
**Goal**: Easy integration for agent developers.

- [ ] Go SDK
- [ ] TypeScript/Node SDK
- [ ] Python SDK
- [ ] Agent framework adapters (LangChain, CrewAI, OpenAI Agents SDK, Google ADK)
- [ ] MCP client integration
- [ ] Admin dashboard (React/Next.js)
- [ ] CLI tool
- [ ] Helm chart + Docker Compose for self-hosting
- [ ] Documentation + quickstart guides

**Deliverable**: `pip install agent-auth-sdk` → 5 lines of code → agent's tool calls are governed.

---

## 10. Differentiation from Existing Projects

| Feature | Our System | Grantex | Legant | Authplane | Agent-Safe | P0 | Permit.io |
|---|---|---|---|---|---|---|---|
| Self-hostable | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ (SaaS) | ❌ (SaaS) |
| JIT credential brokering | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |
| Human approval workflow | ✅ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Multi-agent delegation | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Temporal/trajectory policies | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| MCP gateway | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| Credential vault | ✅ | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ |
| Pluggable policy engines | ✅ | ✅ (OPA/Cedar) | ❌ (embedded) | External | ❌ (YAML) | External | ✅ (OPA) |
| Budget/spend tracking | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Kill switch | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Anomaly detection | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Progressive trust decay | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

**Key differentiators:**
1. **Temporal/trajectory-aware policies** — only Amazon Bedrock AgentCore has this, and it's proprietary
2. **Full credential vault** — most projects either do tokens OR vaulting, not both
3. **Human approval workflow** — many projects mention it, few implement it fully
4. **Progressive trust decay** — novel, inspired by AgentCore but self-hostable
5. **Budget tracking + kill switch** — operational safety beyond just authz
6. **All-in-one** — delegation + MCP gateway + vault + approval + temporal policies in one system

---

## 11. Key Design Principles

1. **Default-deny** — no policy match = DENY, always
2. **No long-lived credentials on agents** — only short-lived, scoped tokens
3. **Monotonic attenuation** — delegation can only narrow, never widen
4. **Offline-verifiable** — services verify tokens via JWKS without calling back
5. **Token containment** — agent tokens never reach upstream APIs
6. **Attribution-complete** — every action traces to human or system authority
7. **Policy as code** — versioned, reviewable, hot-reloadable
8. **Fail closed** — if the control plane is down, deny everything
9. **Zero standing permissions** — access derived JIT, nothing pre-granted
10. **Human-in-the-loop by default** for sensitive/out-of-policy actions

---

## 12. Open Questions

1. **Identity provider integration** — should we build our own IdP or federate to existing (Okta, Azure AD, Google)? Recommend: federate via OIDC, support multiple IdPs.

2. **Multi-tenancy** — should the system support multiple organizations? Recommend: yes, from the start (tenant_id on all entities).

3. **Token format** — JWT only, or also support PASETO (OpenLeash uses PASETO v4.public)? Recommend: JWT for ecosystem compatibility, PASETO optional.

4. **Policy language** — which to default to? Recommend: YAML for simple, OPA for complex, Cedar as optional.

5. **Deployment model** — sidecar, standalone service, or both? Recommend: standalone service with optional sidecar mode for latency-sensitive use cases.

6. **Agent attestation** — how do we verify an agent is what it claims to be? SPIFFE/SPIRE? Join tokens? Recommend: pluggable attestation (join token for dev, SPIRE for prod).

7. **Consent UI** — where does the human approve? Dedicated dashboard, embedded widget, Slack bot? Recommend: dashboard + Slack integration + API for custom UIs.

---

## 13. References

### Open Source Projects
- [Grantex](https://github.com/mishrasanjeev/grantex) — Delegated auth protocol for agents
- [Legant](https://github.com/legant-dev/legant) — RFC 8693 delegated authorization, Go
- [Authplane](https://github.com/authplane/authserver) — OAuth 2.1 AS for MCP
- [mcp-authz](https://github.com/soumyasagiri/mcp-authz) — MCP authorization proxy
- [Charon](https://github.com/NinadRao0707/charon) — SPIFFE-based agent control plane
- [AIP](https://github.com/sunilp/aip) — Agent Identity Protocol (IETF draft)
- [APOA](https://github.com/agenticpoa/apoa-mcp) — Per-tool-call MCP scoping
- [Agent Passport System](https://github.com/aeoess/agent-passport-system) — Agent accountability protocol
- [Agent Zero](https://github.com/casepilot/agent-zero) — JIT access broker for AWS
- [OpenLeash](https://github.com/openleash/openleash) — Local authorization layer
- [Agent-Safe](https://github.com/sahb4k/agent-safe) — Governance and policy enforcement
- [Agent Control Plane](https://github.com/ryanwi/agent-control-plane) — Production governance control plane
- [OpenAgent-Control](https://pypi.org/project/openagent-control/) — MCP proxy with OPA
- [Microsoft Agent Governance Toolkit](https://github.com/microsoft/agent-governance-toolkit) — Full-stack agent governance
- [Alibaba Open Agent Auth](https://github.com/alibaba/open-agent-auth) — Enterprise agent auth framework
- [PortAuth](https://github.com/kyndryl-open-source/aiagent-portable-authorization) — Policy-embedded credentials

### Blog Posts & Articles
- [Cloudflare: The Agent Access Model](https://blog.cloudflare.com/the-agent-access-model/)
- [1Password: Agent Identity Architectures](https://1password.com/blog/ai-agent-identity-architectures)
- [Amazon: Temporal Policies in Bedrock AgentCore](https://aws.amazon.com/blogs/machine-learning/securing-ai-agents-with-temporal-policies-in-amazon-bedrock-agentcore/)
- [Permit.io: OAuth on MCP](https://www.permit.io/blog/oauth-on-mcp)
- [Permit.io: MCP Auth vs Agent Authorization](https://www.permit.io/blog/mcp-auth-vs-agent-authorization)
- [P0: AuthZ Control Plane for Agents](https://p0.dev/blog/technical-deep-dive-authz-control-plane-for-agents/)
- [Agent Identity and Delegated Authorization](https://tianpan.co/blog/2026-04-18-agent-identity-delegated-authorization-oauth-agentic-actions)

### Standards & RFCs
- RFC 8693 — OAuth 2.0 Token Exchange (delegation)
- RFC 8707 — Resource Indicators for OAuth 2.0 (audience binding)
- RFC 9449 — DPoP (proof-of-possession)
- RFC 7591 — Dynamic Client Registration
- RFC 8414 — OAuth 2.0 Authorization Server Metadata
- RFC 9728 — OAuth 2.0 Protected Resource Metadata
- MCP Authorization Specification (2025-11-25)
- SPIFFE/SPIRE — Workload identity framework
- WIMSE — Workload Identity in Multi-Service Environments
- IETF Draft: Agent Operation Authorization
- IETF Draft: AIP (Agent Identity Protocol)

### Research Papers
- [PAuth: Precise Task-Scoped Authorization for Agents (Microsoft Research)](https://www.microsoft.com/en-us/research/publication/pauth-precise-task-scoped-authorization-for-agents/)
- [Digital Identity for Agentic Systems (arXiv:2605.11487)](https://arxiv.org/pdf/2605.11487)
- [OWASP Top 10 for Agentic Applications 2026](https://owasp.org/www-project-agentic-ai/)
