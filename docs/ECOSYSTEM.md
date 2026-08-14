# Ecosystem: How It All Works Together

## The Four-Project Agent Governance Stack

```
┌─────────────────────────────────────────────────────────────────────┐
│                     AGENT GOVERNANCE ECOSYSTEM                      │
│                                                                     │
│    Hive          Patroclus         Relay           Miser           │
│    ─────         ─────────         ─────           ─────           │
│    Agent         Authorization     MCP Proxy       Cost            │
│    Runtime &     Infrastructure    & Tool          Optimization    │
│    Orchestration                    Gateway                        │
│                                                                     │
│    "Which agent   "Is the agent   "Route the      "Which model    │
│     does the       allowed to do    agent's tool    is cheapest    │
│     work?"         this?"           call?"          for this?"     │
└─────────────────────────────────────────────────────────────────────┘
```

## Quick Start: Run Everything Locally

```bash
# 1. Start all four services
~/patroclus/scripts/start-ecosystem.sh start

# 2. Verify all services are up
~/patroclus/scripts/start-ecosystem.sh status

# 3. Run the e2e test
python3 ~/patroclus/scripts/e2e_test.py

# 4. Stop everything
~/patroclus/scripts/start-ecosystem.sh stop
```

## Service Ports

| Service  | Port  | Health Check |
|----------|-------|-------------|
| Hive     | 8000  | `GET /` returns HTML marketplace |
| Patroclus| 8484  | `GET /health` returns JSON |
| Relay    | 8090  | `GET /patroclus/status` returns connection status |
| Miser    | 8787  | `GET /health/live` returns JSON |

## How They Connect

### 1. Hive ↔ Patroclus: Agent Registration

When a user registers an agent in Hive, Hive automatically:

1. **Registers a principal** (the human owner) in Patroclus
   ```
   POST http://localhost:8484/v1/admin/principals
   ```

2. **Registers the agent** in Patroclus as a delegated agent owned by the principal
   ```
   POST http://localhost:8484/v1/admin/agents
   ```

3. **Creates a default policy** in Patroclus for the agent:
   - Allow tool calls, reads, queries on all resources
   - Rate limit API calls (10/min)
   - Require human approval for production writes/deletes/deploys
   ```
   POST http://localhost:8484/v1/admin/policies
   ```

This means every agent in Hive is automatically governed by Patroclus from
the moment it's registered.

### 2. Relay ↔ Patroclus: Per-Tool Authorization

Relay is the MCP gateway. When an agent makes a tool call through Relay:

```
Agent → Relay MCP endpoint → Relay checks with Patroclus → (if allowed) → upstream service
```

Relay calls `POST http://localhost:8484/v1/agent/check` before every tool
dispatch. If Patroclus returns `deny`, Relay blocks the call. If `allow`,
Relay forwards to the upstream MCP server or API.

This is configured via environment variables in Relay:
```env
PATROCLUS_ENABLED=true
PATROCLUS_URL=http://localhost:8484
```

### 3. Hive Agent ↔ Miser: LLM Cost Optimization

Hive agents make LLM calls via the OpenAI-compatible API. By setting
`OPENROUTER_BASE_URL` in Hive's `.env`, all agent LLM calls route through
Miser instead of going directly to OpenRouter:

```env
OPENROUTER_BASE_URL=http://localhost:8787/v1
OPENROUTER_MODEL=auto
```

Miser then:
1. Classifies the request complexity (trivial/simple/standard/hard/reasoning)
2. Routes to the cheapest capable model
3. Caches exact-match responses (5-min TTL)
4. Returns with cost metadata headers

### 4. Miser ↔ Patroclus: Budget Enforcement (Planned)

Miser reports the actual cost of each LLM call to Patroclus. Patroclus's
`max_spend` policy constraint blocks further LLM calls when the session
budget is exceeded:

```
Agent makes LLM call → Miser routes → Miser reports cost to Patroclus →
Patroclus checks budget → if over budget, next request is denied
```

## End-to-End Flow

Here's what happens when a user submits a task to an agent in Hive:

```
1. User submits task to Hive
   POST http://localhost:8000/api/delegate/user-request

2. Hive dispatches task to agent
   → Agent subprocess receives the task

3. Agent needs LLM inference
   → Agent calls http://localhost:8787/v1/chat/completions (Miser)
   → Miser classifies complexity, routes to cheapest model
   → Miser returns LLM response to agent

4. Agent decides to call a tool (e.g., read GitHub repos)
   → Agent calls Relay MCP endpoint
   → Relay calls Patroclus: "Is this agent allowed to call github/list_repos?"
   → Patroclus evaluates policy:
     - If ALLOW → Relay forwards to GitHub API, returns result to agent
     - If DENY → Relay blocks, returns error to agent
     - If REQUIRE_APPROVAL → Relay blocks, returns approval request to agent

5. Agent completes task
   → Agent calls back to Hive with result
   → Hive settles token cost (using Miser's cost tracking)

6. All decisions logged
   → Patroclus audit log (hash-chained)
   → Relay audit log
   → Hive delegation log
   → Miser cost tracking
   All correlated by session_id
```

## E2E Test Results (Verified)

The `scripts/e2e_test.py` script verifies:

| Test | Result |
|------|--------|
| All 4 services healthy | ✓ |
| Hive agent registration → Patroclus auto-registration | ✓ |
| Patroclus policy auto-created per agent | ✓ |
| Read access ALLOWED with scoped JWT | ✓ |
| Production deploy → REQUIRE_APPROVAL | ✓ |
| Human approval workflow (approve → token issued) | ✓ |
| Rate limiting enforced | ✓ |
| Audit trail hash-chained and intact | ✓ |
| Session tracking (actions, trust, trajectory) | ✓ |
| Kill switch blocks all subsequent access | ✓ |
| Relay connected to Patroclus | ✓ |
| Miser ready for LLM routing | ✓ |

## Configuration

### Hive `.env`
```env
PATROCLUS_URL=http://localhost:8484
OPENROUTER_BASE_URL=http://localhost:8787/v1
OPENROUTER_MODEL=auto
RELAY_URL=http://localhost:8090
```

### Relay Environment
```env
RELAY_SERVER__PORT=8090
PATROCLUS_ENABLED=true
PATROCLUS_URL=http://localhost:8484
```

### Patroclus `config.toml`
```toml
[server]
host = "0.0.0.0"
port = 8484

[database]
path = "patroclus.db"

[token]
issuer = "http://localhost:8484"
default_ttl_seconds = 900  # 15 minutes

[policy]
engine = "yaml"
default_decision = "deny"
max_delegation_depth = 3
```

### Miser
```toml
# Miser runs on port 8787 by default
# No special configuration needed for ecosystem integration
```

## Architecture Diagram

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
│   Hive       │────────────▶│   Patroclus  │    │  Approval        │    │  Policy Store   │
│   (Agent     │             │   Gateway    │    │  Service         │    │  (YAML/OPA)     │
│   Market)    │◀────────────│              │    │                  │    │                 │
│              │   token or  │  ┌──────────┐│    │  - pending list  │    │  - rules        │
│              │   deny      │  │  Policy  ││    │  - approve/deny  │    │  - hot-reload   │
│ ┌──────────┐ │             │  │  Engine  ││    │  - notify human  │    │  - versioned    │
│ │  Agent   │ │             │  └────┬─────┘│    └──────────────────┘    └─────────────────┘
│ │ Runtime  │ │             │       │      │
│ │          │ │             │       ▼      │
│ │ LLM →    │ │             │  ┌──────────┐│    ┌──────────────────┐
│ │ Miser    │ │             │  │  Token   ││    │  Session Store   │
│ │          │ │             │  │  Issuer  ││    │                  │
│ │ Tools →  │ │             │  │  (JWT)   ││    │  - trajectory    │
│ │ Relay    │ │             │  └────┬─────┘│    │  - rate limits   │
│ │          │ │             │       │      │    │  - budget caps   │
│ └─────┬────┘ │             │       ▼      │    │  - trust decay   │
│       │      │             │  ┌──────────┐│    │  - kill switch   │
│       │      │             │  │  Audit   ││    │                  │
│       │      │             │  │  Log     ││    └──────────────────┘
│       │      │             │  │  (SHA256)││
│       │      │             │  └──────────┘│
└───────┼──────┘             └──────┬───────┘
        │                           │
        │ LLM call                  │ vend credential
        ▼                           ▼
┌──────────────┐             ┌──────────────┐
│   Miser      │             │  Credential  │
│   (Cost      │             │  Vault       │
│   Router)    │             │  (AES-256)   │
│              │             │              │
│  - Classify  │             │  - GitHub    │
│  - Route     │             │  - Google    │
│  - Cache     │             │  - Slack     │
│  - Cost      │             │              │
└──────────────┘             └──────────────┘

┌──────────────┐
│   Relay      │
│   (MCP       │
│   Proxy)     │
│              │
│  - Intercept │
│  - Authorize │─── checks with Patroclus
│  - Forward   │
│  - Audit     │
└──────┬───────┘
       │
       │  forward to
       ▼
┌──────────────┐
│  Upstream    │
│  Resources   │
│  - MCP       │
│  - APIs      │
│  - databases │
└──────────────┘
```

## Session ID: The Correlation Key

All four components share a `session_id` that enables cross-component correlation:

- **Hive**: delegation session → `session_id`
- **Patroclus**: policy evaluation → same `session_id`
- **Relay**: tool call audit → same `session_id`
- **Miser**: LLM request → same `session_id`

This means you can ask: "Show me everything that happened in session abc-123"
and get:
- Hive: delegations, agent actions, token settlement
- Patroclus: policy decisions, tokens issued/denied, approvals
- Relay: tool calls, upstream requests
- Miser: LLM requests, model routing, cost

## Independent Deployment

Each component can be used independently:

- **Patroclus alone**: Use as authorization API for any agent
- **Relay alone**: Use as MCP proxy (without Patroclus, it passes through)
- **Miser alone**: Use as LLM cost optimizer for any OpenAI-compatible tool
- **Hive alone**: Use as agent marketplace without governance

The ecosystem emerges when you combine all four.
