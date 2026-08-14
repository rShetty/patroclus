# Ecosystem Plan: Hive + Patroclus + Relay + Miser

## Vision

Four open-source projects forming a complete agent governance ecosystem:

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

## Component Roles

### Hive — Agent Runtime & Orchestration
- Agent marketplace: register, discover, delegate work
- Teams (LLM-planned fan-out) and Workflows (deterministic pipelines)
- Token economy with escrow and settlement
- Agent lifecycle: heartbeats, health checks, watchdog
- SSE streaming for real-time progress

### Patroclus — Authorization Infrastructure
- Policy engine: YAML rules with temporal conditions
- Token issuer: short-lived JWTs with delegation chains
- Approval workflow: human-in-the-loop for sensitive actions
- Credential vault: AES-256 encrypted storage + OAuth token exchange
- Session tracking: trajectory, rate limits, budget caps, trust decay
- Kill switch: emergency agent termination

### Relay — MCP Gateway & Tool Proxy
- MCP protocol proxy (Streamable HTTP, stdio, SSE)
- Per-tool-call authorization via Patroclus
- Per-user token management for third-party services
- Input validation, rate limiting, audit logging
- Connector system: GitHub, Slack, Linear, OpenAI, Anthropic

### Miser — Cost Optimization Gateway
- Per-request complexity classification (5 tiers)
- Routes 80%+ of requests to free open-weight models
- Exact-match caching (10K entries, 5-min TTL)
- Quality escalation: retries at higher tier if quality < threshold
- <1ms p99 gateway overhead

## Integration Architecture

```
User
  │
  │ 1. Submit task to Hive
  ▼
┌──────────┐
│   Hive   │  2. Hive delegates to agent(s)
│          │  3. Agent plans actions
│  (Agent  │
│  Runtime)│
└────┬─────┘
     │
     │ 4. Agent needs LLM inference
     ▼
┌──────────┐
│  Miser   │  5. Classify complexity → route to cheapest model
│  (Cost   │  6. Return LLM response
│  Gateway)│
└────┬─────┘
     │
     │ 7. Agent decides to call a tool
     ▼
┌──────────┐         ┌──────────┐
│  Relay   │───────▶│Patroclus │  8. Relay asks: "Is this allowed?"
│  (MCP    │         │  (Authz) │  9. Patroclus evaluates policy
│  Proxy)  │◀───────│          │ 10. Returns ALLOW/DENY/APPROVAL
└────┬─────┘         └──────────┘
     │
     │ 11. If allowed, Relay forwards to upstream
     ▼
┌──────────┐
│ Upstream │  12. GitHub API / Slack API / Database / MCP Server
│ Services │
└──────────┘
     │
     │ 13. Result flows back: Relay → Agent → Hive → User
     ▼
  User gets result

Side-channel (always running):
  - Patroclus audit log records every decision
  - Miser tracks token usage and cost
  - Hive tracks delegation chains and token escrow
  - Relay tracks tool call audit log
```

## Detailed Integration Points

### 1. Hive ↔ Patroclus: Agent Registration & Delegation

**Current state**: Hive has its own agent registration and delegation protocol
with token escrow. Patroclus has its own agent registration and delegation with
scoped JWTs.

**Integration plan**:
- When Hive registers an agent, it also registers it with Patroclus
- When Hive delegates work to an agent, it calls Patroclus to create a scoped
  delegation grant
- Hive's token escrow (budget for task) maps to Patroclus's `max_spend` constraint
- Hive's max delegation depth (5) aligns with Patroclus's `max_delegation_depth` (3,
  configurable — should be set to 5 to match Hive)
- Hive's team orchestration creates a Patroclus session per team run, enabling
  trajectory-aware policies across the fan-out

**Implementation**:
```python
# In Hive's agent registration flow:
patroclus = PatroclusClient(PATROCLUS_URL)
patroclus.register_agent(
    name=agent.slug,
    principal_type="delegated",
    owner_id=human_principal_id,
)

# In Hive's delegation flow:
result = patroclus.delegate_permissions(
    agent_id=agent.patroclus_id,
    scopes=agent.allowed_scopes,
    expires_in_seconds=task_timeout_seconds,
    constraints={"max_spend": task.token_budget},
)
```

### 2. Hive ↔ Miser: LLM Cost Control

**Current state**: Hive agents make LLM calls directly. Miser is a transparent
proxy that classifies and routes.

**Integration plan**:
- Hive agents use Miser as their LLM endpoint (`base_url=http://miser:8787/v1`)
- Miser's cost tracking feeds back to Hive's token economy:
  - Miser tracks actual token cost per request
  - Hive's token escrow settles based on Miser's reported cost, not the agent's
    self-reported `tokens_used`
- Miser's quality escalation (retry at higher tier) is transparent to Hive

**Implementation**:
```python
# In Hive's agent runtime config:
LLM_BASE_URL = "http://miser:8787/v1"  # Route through Miser

# In Hive's settlement (post-delegation):
miser_cost = miser_client.get_session_cost(session_id)
tokens_used = min(miser_cost, escrowed_tokens)
agent_payment = tokens_used - (tokens_used * PLATFORM_FEE)
```

### 3. Relay ↔ Patroclus: Per-Tool Authorization

**Current state**: Already integrated in Phase 3. Relay calls Patroclus
`check_access()` before every tool dispatch.

**Enhancement plan**:
- Relay should use Patroclus's `request_access()` (not just `check_access()`) to
  get a scoped JWT token for each tool call
- The JWT token can then be passed to the upstream MCP server for authentication
- Relay's per-user token vault complements Patroclus's credential vault:
  - Relay stores the OAuth tokens for third-party services (GitHub, Slack)
  - Patroclus's vault stores the refresh tokens and does the token exchange
  - On `ALLOW` from Patroclus, Relay calls Patroclus's `/v1/vault/vend` to get
    a scoped access token for the upstream service

### 4. Hive ↔ Relay: Tool Access Per Agent

**Current state**: Hive has its own MCP server registry. Relay is an MCP gateway.

**Integration plan**:
- Hive agents connect to Relay's MCP endpoints (not directly to MCP servers)
- Hive's MCP server grants (per-agent) map to Patroclus policies
- When Hive grants an agent access to an MCP server, it creates a Patroclus policy
  allowing that agent to call tools on that server

**Implementation**:
```python
# In Hive's MCP grant flow:
patroclus.create_policy(
    name=f"agent-{agent_id}-mcp-{server_name}",
    definition=f"""
- name: allow-{server_name}
  actions: ["call"]
  resources: ["{server_name}/*"]
  scopes: ["{server_name}:*"]
  decision: allow
  reason: "Granted by Hive MCP registry"
""",
)
```

### 5. Miser ↔ Patroclus: Budget Enforcement

**Current state**: Miser routes requests by complexity. Patroclus tracks spend
in sessions.

**Integration plan**:
- Miser calls Patroclus's `record_spend()` after each LLM request with the actual
  cost
- Patroclus's `max_spend` policy constraint blocks further LLM calls when budget
  is exceeded
- This creates a feedback loop: agent makes LLM call → Miser routes → Miser
  reports cost to Patroclus → Patroclus denies next request if over budget

**Implementation**:
```rust
// In Miser's response handler (after getting LLM response):
let cost = response.usage.cost;
patroclus_client.record_spend(agent_id, cost, session_id).await;

// In Patroclus policy:
// - name: budget-capped-llm
//   actions: ["inference"]
//   resources: ["llm-*"]
//   decision: allow
//   max_spend: 1.0  // $1.00 budget for LLM calls
```

## Ecosystem Data Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                        SHARED STATE                              │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│  │ Patroclus   │  │   Hive      │  │   Miser     │            │
│  │ Audit Log   │  │ Delegation  │  │ Cost        │            │
│  │ (hash-chain)│  │ Log         │  │ Tracking    │            │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘            │
│         │                │                │                    │
│         └────────────────┴────────────────┘                    │
│                          │                                       │
│                   Unified audit trail                           │
│                   (correlate by session_id)                     │
└──────────────────────────────────────────────────────────────────┘

Session ID flows through all four:
  Hive: delegation session → session_id
  Patroclus: policy evaluation → session_id (same)
  Relay: tool call audit → session_id (same)
  Miser: LLM request → session_id (same)

This enables cross-component correlation:
  "Show me everything that happened in session abc-123"
  → Hive: delegations, agent actions, token settlement
  → Patroclus: policy decisions, tokens issued/denied
  → Relay: tool calls, upstream requests
  → Miser: LLM requests, model routing, cost
```

## Phased Integration Roadmap

### Phase A: Hive ↔ Patroclus (Weeks 1-2)
- Register Hive agents with Patroclus on deployment
- Create Patroclus delegation grants when Hive delegates work
- Map Hive token escrow to Patroclus budget constraints
- Use Patroclus session_id = Hive delegation session_id

### Phase B: Hive ↔ Miser (Weeks 3-4)
- Route Hive agent LLM calls through Miser
- Feed Miser cost data back to Hive's token economy
- Use Miser's `@route:` override for agent-specified complexity

### Phase C: Relay ↔ Patroclus Enhancement (Weeks 5-6)
- Relay uses `request_access()` (not just `check_access()`) to get JWT tokens
- Relay calls Patroclus vault to vend scoped upstream credentials
- Relay passes JWT to upstream MCP servers for authentication

### Phase D: Miser ↔ Patroclus (Weeks 7-8)
- Miser reports cost to Patroclus after each request
- Patroclus `max_spend` policy blocks LLM calls when budget exceeded
- Unified session_id across all four components

### Phase E: Unified Dashboard (Weeks 9-10)
- Single dashboard showing:
  - Agent activity (from Hive)
  - Authorization decisions (from Patroclus)
  - Tool calls (from Relay)
  - LLM costs (from Miser)
- Correlated by session_id
- Kill switch from dashboard propagates to all components

## Docker Compose for Full Ecosystem

```yaml
version: "3.9"

services:
  hive:
    image: ghcr.io/rshetty/hive:latest
    ports: ["3000:3000"]
    environment:
      - PATROCLUS_URL=http://patroclus:8484
      - MISER_URL=http://miser:8787
      - RELAY_URL=http://relay:8000
    depends_on: [patroclus, miser, relay]

  patroclus:
    build: .
    ports: ["8484:8484"]
    volumes:
      - patroclus-data:/app/data
      - patroclus-keys:/app/keys

  relay:
    image: ghcr.io/rshetty/relay:latest
    ports: ["8000:8000"]
    environment:
      - PATROCLUS_ENABLED=true
      - PATROCLUS_URL=http://patroclus:8484
    depends_on: [patroclus]

  miser:
    image: ghcr.io/rshetty/miser:latest
    ports: ["8787:8787"]
    environment:
      - PATROCLUS_URL=http://patroclus:8484
    depends_on: [patroclus]

volumes:
  patroclus-data:
  patroclus-keys:
```

## Key Design Principles for the Ecosystem

1. **Session ID is the correlation key** — flows through all four components
2. **Patroclus is the policy authority** — all components check with it before acting
3. **Miser is the cost authority** — all LLM calls route through it
4. **Relay is the tool authority** — all tool calls route through it
5. **Hive is the orchestration authority** — all agent work flows through it
6. **Fail closed everywhere** — if Patroclus is down, deny. If Miser is down, error. If Relay is down, no tool calls.
7. **Each component is independently deployable** — can use any subset of the four
8. **Audit trail is unified** — same session_id, correlatable across all components
