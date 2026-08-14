# Patroclus — End-to-End Architecture

## Complete Agent Authorization Flow

```
                         ┌─────────────────────────────────────────────────────────────────────┐
                         │                           USER / HUMAN                                │
                         │                     (approves, delegates, manages)                    │
                         └───────────┬───────────────────────┬──────────────────────┬──────────┘
                                     │                       │                      │
                          delegates  │            approves/  │         manages      │
                          scoped     │            denies     │         policies     │
                          perms      │            approvals  │                      │
                                     ▼                       ▼                      ▼
┌──────────────┐   request   ┌──────────────┐    ┌──────────────────┐    ┌─────────────────┐
│              │   access    │              │    │                  │    │                 │
│   Agent      │────────────▶│   Patroclus  │    │  Approval        │    │  Policy Store   │
│   Runtime    │             │   Gateway    │    │  Service         │    │  (YAML/OPA)     │
│   (Hive)     │◀────────────│              │    │                  │    │                 │
│              │   token or  │  ┌──────────┐│    │  - pending list  │    │  - rules        │
│              │   deny      │  │  Policy  ││    │  - approve/deny  │    │  - hot-reload   │
│              │             │  │  Engine  ││    │  - notify human  │    │  - versioned    │
│              │             │  └────┬─────┘│    └──────────────────┘    └─────────────────┘
│              │             │       │      │
│              │             │       ▼      │
│              │             │  ┌──────────┐│
│              │             │  │  Token   ││    ┌──────────────────┐
│              │             │  │  Issuer  ││    │  Session Store   │
│              │             │  │  (JWT)   ││    │                  │
│              │             │  └────┬─────┘│    │  - trajectory    │
│              │             │       │      │    │  - rate limits   │
│              │             │       ▼      │    │  - budget caps   │
│              │             │  ┌──────────┐│    │  - trust decay   │
│              │             │  │  Audit   ││    │  - kill switch   │
│              │             │  │  Log     ││    │                  │
│              │             │  │  (SHA256)││    └──────────────────┘
│              │             │  └──────────┘│
└──────────────┘             └──────┬───────┘
       │                            │
       │  uses token to             │ vend credential
       │  access resource           │ (if vault enabled)
       ▼                            ▼
┌──────────────┐             ┌──────────────┐
│              │             │              │
│   Relay      │────────────▶│  Credential  │
│   (MCP       │   exchange  │  Vault       │
│   Proxy)     │   for       │  (AES-256)   │
│              │  scoped     │              │
│  - intercept │  token      │  - GitHub    │
│  - authorize │             │  - Google    │
│  - forward   │             │  - Slack     │
│              │             │              │
└──────┬───────┘             └──────────────┘
       │
       │  forward to
       │  upstream
       ▼
┌──────────────┐
│  Upstream    │
│  Resources   │
│              │
│  - MCP       │
│    servers   │
│  - APIs      │
│  - databases │
│  - cloud     │
│              │
└──────────────┘
```

## E2E Scenario: Agent reads dev database

```
Step 1: Setup (one-time)
─────────────────────────
Human → POST /v1/admin/principals  → Creates Alice (alice@example.com)
Human → POST /v1/admin/agents      → Creates agent-001 (delegated, owned by Alice)
Human → POST /v1/admin/policies    → Creates policy: allow reads on dev-*
Human → POST /v1/principal/delegate → Alice delegates [db:read] to agent-001 for 1h

Step 2: Agent requests access
──────────────────────────────
Agent → POST /v1/agent/request-access
         { agent_id: agent-001, action: "read", resource: "dev-db/users",
           requested_scopes: ["db:read"], context: { session_id: "sess-1" } }

         ↓ Patroclus internal flow:

         1. Get agent from DB → agent-001 (active, owned by Alice)
         2. Get principal from DB → Alice
         3. Get/create session → sess-1 (trust=1.0, actions=0)
         4. Build PolicyContext { agent, principal, action, resource, scopes, session_id, trajectory }
         5. Policy Engine evaluates:
            - Rule "allow-dev-reads" matches: action=read, resource=dev-* → ALLOW
            - Temporal checks pass (not killed, under rate limit, trust OK)
         6. Token Issuer mints JWT:
            { sub: "user:alice@example.com", act: { sub: "agent:agent-001", depth: 0 },
              scope: "db:read", aud: "dev-db/users", exp: +15min, jti: "01J5..." }
         7. Record token in DB (for revocation tracking)
         8. Record action in session trajectory
         9. Create audit entry (hash-chained)

         ↓ Response:

         { decision: "allow",
           token: { jwt: "eyJ...", jti: "01J5...", scopes: ["db:read"], expires_at: "..." },
           reason: "Dev read access permitted" }

Step 3: Agent uses token (via Relay)
─────────────────────────────────────
Agent → Relay MCP endpoint
         (passes JWT in authorization header)

         ↓ Relay internal flow:

         1. Relay receives MCP tools/call
         2. Relay calls Patroclus: POST /v1/agent/check
            { agent_id, action: "call", resource: "dev-db/query", scopes: ["db:read"] }
         3. Patroclus returns ALLOW
         4. Relay forwards to upstream MCP server
         5. Relay returns result to agent

Step 4: Agent tries unauthorized action
────────────────────────────────────────
Agent → POST /v1/agent/request-access
         { agent_id: agent-001, action: "delete", resource: "prod-db/users",
           requested_scopes: ["db:delete"] }

         → Policy Engine: rule "deny-prod-deletes" matches → DENY
         → Response: { decision: "deny", reason: "Production deletes forbidden" }
         → Audit log records denial

Step 5: Agent hits rate limit
──────────────────────────────
Agent makes 6th API call in a minute (limit: 5/min)

         → Policy Engine: rule "rate-limited-api" matches
         → Temporal check: rate_limit_per_minute = 5, current count = 6
         → DENY: "Rate limit exceeded"
         → Session records the denied attempt in trajectory

Step 6: Emergency stop
──────────────────────
Human → POST /v1/admin/agents/{agent-001}/kill

         → All sessions for agent-001 set killed=true
         → All tokens for agent-001 revoked in DB + in-memory
         → Subsequent request_access returns: "session has been killed"

Step 7: Audit verification
──────────────────────────
Human → GET /v1/admin/audit

         → Returns hash-chained entries:
           [allow] read → dev-db/users  (token jti: 01J5...)
           [deny]  delete → prod-db/users
           [deny]  call → api-github (rate limited)
           [deny]  read → dev-db/users (killed)
         → Each entry's prev_hash matches prior entry's row_hash
```

## Multi-Agent Delegation Flow

```
Alice (human)
  │
  │ delegate [calendar:read, calendar:write, email:send] for 1h
  ▼
Agent-001 (orchestrator)
  │
  │ sub-delegate [calendar:read] for 10min      ← subset ✓
  ├──▶ Agent-002 (worker)
  │
  │ sub-delegate [calendar:read, calendar:write] for 5min  ← subset ✓
  ├──▶ Agent-003 (worker)
  │
  │ sub-delegate [calendar:read, admin:all]     ← REJECTED (admin:all not in parent)
  └──▶ Agent-004 → 400 ScopeEscalation

Revoke Agent-001:
  → Agent-002's grant revoked (cascade)
  → Agent-003's grant revoked (cascade)
  → All tokens for all three agents invalidated
```

## Components in the Ecosystem

```
┌─────────────────────────────────────────────────────────────┐
│                    AGENT GOVERNANCE ECOSYSTEM                │
│                                                             │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌──────────┐│
│  │   Hive    │  │ Patroclus │  │   Relay   │  │  Miser   ││
│  │           │  │           │  │           │  │          ││
│  │ Agent     │  │ Authz     │  │ MCP       │  │ Cost     ││
│  │ Runtime   │  │ Infra     │  │ Gateway   │  │ Control  ││
│  │           │  │           │  │           │  │          ││
│  │ - Execute │  │ - Policy  │  │ - Proxy   │  │ - Budget ││
│  │ - Plan    │  │ - Token   │  │ - Filter  │  │ - Track  ││
│  │ - Tools   │  │ - Approve │  │ - Audit   │  │ - Alert  ││
│  │           │  │ - Vault   │  │ - Route   │  │ - Limit  ││
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘  └────┬─────┘│
│        │              │              │              │      │
│        └──────────────┴──────────────┴──────────────┘      │
│                           │                                 │
│                    Shared ecosystem                         │
│                    - Audit trail                            │
│                    - Session state                          │
│                    - Policy enforcement                     │
│                    - Cost tracking                          │
└─────────────────────────────────────────────────────────────┘
```
