# Local Integration Plan: Running the Full Ecosystem

## Goal

Run all four projects (Hive, Patroclus, Relay, Miser) locally and demonstrate
an end-to-end agent task where:
1. A user submits a task to Hive
2. Hive dispatches to an agent
3. The agent's LLM calls route through Miser (cost optimization)
4. The agent's tool calls route through Relay (MCP gateway)
5. Relay checks with Patroclus before each tool call (authorization)
6. Patroclus enforces policies (allow/deny/rate-limit/approval)
7. The agent completes the task and Hive settles the token cost

## Architecture

```
localhost:8000      localhost:8484      localhost:8000      localhost:8787
┌──────────┐       ┌──────────┐       ┌──────────┐       ┌──────────┐
│   Hive   │       │Patroclus │       │  Relay   │       │  Miser   │
│          │       │          │       │          │       │          │
│  Agent   │──────▶│          │◀──────│  MCP     │       │  LLM     │
│  Market  │       │  Authz   │       │  Proxy   │       │  Router  │
│          │       │          │       │          │       │          │
└────┬─────┘       └──────────┘       └──────────┘       └──────────┘
     │
     │ agent subprocess (port 9xxx)
     ▼
┌──────────────────┐
│  Agent Runtime    │
│  (Hive-managed)  │
│                  │
│  LLM → Miser     │
│  Tools → Relay   │
│  Authz → Patroclus│
└──────────────────┘
```

## Ports

| Service  | Port  | Notes |
|----------|-------|-------|
| Hive     | 8000  | Agent marketplace + API |
| Patroclus| 8484  | Authorization server |
| Relay    | 8000* | Conflicts with Hive — change to 8090 |
| Miser    | 8787  | LLM cost gateway |
| Agent    | 9xxx  | Hive-spawned subprocess per agent |

*Relay defaults to 8000, same as Hive. We'll configure Relay to use 8090.

## Step-by-Step Plan

### Step 1: Start Patroclus (already built)

```bash
cd ~/patroclus
rm -f patroclus.db
./target/release/patroclus serve --config config.toml
# Health: http://localhost:8484/health
```

No changes needed — Patroclus is ready.

### Step 2: Start Miser

```bash
cd ~/miser
cargo build --release
./target/release/miser  # or however miser starts
# Health: http://localhost:8787/health/live
```

Create a Miser API key for the agent to use:
```bash
curl -X POST http://localhost:8787/admin/keys \
  -H "Authorization: Bearer $MISER_ADMIN_KEY" \
  -d '{"name": "hive-agent"}'
# Returns: miser_<key>
```

### Step 3: Start Relay (port 8090)

```bash
cd ~/relay
# Set env vars
export RELAY_SERVER__PORT=8090
export PATROCLUS_ENABLED=true
export PATROCLUS_URL=http://localhost:8484
export OAUTH__JWT_SECRET_KEY=your-secret
export RELAY_ALLOW_DEFAULT_SECRET=1

python -m gateway.server http
# Health: http://localhost:8090/patroclus/status
```

### Step 4: Start Hive

```bash
cd ~/hive/backend
# The .env already has OPENROUTER_API_KEY
# We need to point the agent's LLM at Miser instead of OpenRouter directly

# For now, start Hive normally:
pip install -r requirements.txt
uvicorn main:app --reload --port 8000
```

### Step 5: Integration Code Changes

#### 5.1: Hive Agent → Miser (LLM routing)

File: `~/hive/docker/agent_app/main.py`, function `_resolve_llm()` (line ~220)

Current:
```python
if _secret("OPENROUTER_API_KEY"):
    return {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key": _secret("OPENROUTER_API_KEY"),
        "model": os.getenv("OPENROUTER_MODEL", "openai/gpt-4o-mini"),
    }
```

Change: Add env var override for base_url:
```python
if _secret("OPENROUTER_API_KEY"):
    return {
        "base_url": os.getenv("OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1"),
        "api_key": _secret("OPENROUTER_API_KEY"),
        "model": os.getenv("OPENROUTER_MODEL", "openai/gpt-4o-mini"),
    }
```

Then in Hive's `.env` (or agent env):
```env
OPENROUTER_BASE_URL=http://localhost:8787/v1
OPENROUTER_MODEL=auto
```

This makes the agent's LLM calls go through Miser, which classifies and
routes to the cheapest capable model.

#### 5.2: Hive Agent → Relay (MCP tool routing)

When registering an MCP server in Hive, set the URL to Relay's per-user
MCP endpoint:

```bash
# Register MCP server in Hive, pointing at Relay
curl -X POST http://localhost:8000/api/mcp-servers \
  -H "Authorization: Bearer <hive-jwt>" \
  -d '{
    "name": "github-relay",
    "url": "http://localhost:8090/user-mcp/relay_key/github/mcp",
    "transport": "streamable_http"
  }'
```

Then grant this MCP server to the agent. The agent will call Relay,
which checks with Patroclus before forwarding to GitHub.

#### 5.3: Hive → Patroclus (agent registration + delegation)

Add a Patroclus client to Hive's agent registration flow:

File: `~/hive/backend/routers/agent_api.py`, in the register endpoint

```python
# After Hive registers the agent, also register with Patroclus
import httpx
patroclus = httpx.Client("http://localhost:8484")

# Register principal (if not exists)
patroclus.post("/v1/admin/principals", json={
    "external_id": user["email"],
    "idp_provider": "local",
    "email": user["email"],
    "display_name": user["username"],
})

# Register agent
patroclus_resp = patroclus.post("/v1/admin/agents", json={
    "name": agent.name,
    "principal_type": "delegated",
    "owner_id": principal_id,
})
agent.patroclus_id = patroclus_resp.json()["id"]
```

#### 5.4: Hive → Patroclus (policy creation per agent)

When an agent is registered, create a default policy allowing it to
call tools through Relay:

```python
patroclus.post("/v1/admin/policies", json={
    "name": f"agent-{agent.slug}-default",
    "engine": "yaml",
    "definition": f"""
- name: allow-tool-calls
  actions: ["call", "read", "query"]
  resources: ["github/*", "slack/*"]
  scopes: ["*"]
  decision: allow
  reason: Default tool access for {agent.name}

- name: rate-limited
  actions: ["call"]
  resources: ["api-*"]
  scopes: ["*"]
  decision: allow
  rate_limit_per_minute: 10
  reason: Rate limited API access
""",
})
```

#### 5.5: Hive → Patroclus (delegation on task dispatch)

When Hive dispatches a task to an agent, create a Patroclus delegation:

```python
# In _execute_delegation_task():
patroclus.post("/v1/principal/delegate", json={
    "agent_id": agent.patroclus_id,
    "scopes": ["github:read", "slack:read"],
    "expires_in_seconds": task.timeout_seconds,
    "constraints": {
        "max_spend": task.max_tokens * 0.01  # $0.01 per token
    },
})
```

### Step 6: E2E Test Script

```python
"""
E2E test: Submit a task to Hive, verify the agent:
1. Routes LLM through Miser
2. Checks with Patroclus before tool calls
3. Gets allow/deny decisions
4. Completes the task
"""
import httpx
import time

HIVE_URL = "http://localhost:8000"
PATROCLUS_URL = "http://localhost:8484"
RELAY_URL = "http://localhost:8090"
MISER_URL = "http://localhost:8787"

# 1. Verify all services are up
assert httpx.get(f"{PATROCLUS_URL}/health").json()["status"] == "ok"
assert httpx.get(f"{MISER_URL}/health/live").status_code == 200
assert httpx.get(f"{RELAY_URL}/patroclus/status").json()["connected"] is True

# 2. Register on Hive
hive = httpx.Client(base_url=HIVE_URL)
# ... register user, get JWT, register agent ...

# 3. Check Patroclus has the agent registered
agents = patroclus.get("/v1/admin/agents").json()
assert any(a["name"] == "test-agent" for a in agents)

# 4. Submit task to Hive
task = hive.post("/api/delegate/user-request", json={
    "target_agent_id": agent_id,
    "task_description": "List all GitHub repos in rShetty org",
    "max_tokens": 100,
})
delegation_id = task.json()["delegation_id"]

# 5. Stream progress
async with httpx.AsyncClient() as client:
    async with client.stream("GET", f"{HIVE_URL}/api/delegate/{delegation_id}/user-stream?token={jwt}") as resp:
        async for line in resp.aiter_lines():
            print(line)
            if "completed" in line.lower():
                break

# 6. Verify Patroclus audit log has entries
audit = patroclus.get("/v1/admin/audit").json()
assert len(audit) > 0
assert any(a["decision"] == "allow" for a in audit)

# 7. Verify Miser has cost tracking
# (Check Miser's session cost endpoint)

# 8. Verify Relay has tool call logs
# (Check Relay's audit log)
```

### Step 7: Verify the Integration

Run the e2e script and verify:
- [ ] Hive task is submitted and delegated
- [ ] Agent receives and executes the task
- [ ] LLM calls show up in Miser's logs (with tier routing)
- [ ] Tool calls go through Relay
- [ ] Relay checks with Patroclus (visible in Patroclus audit log)
- [ ] Patroclus returns allow/deny decisions
- [ ] Agent completes task
- [ ] Hive settles the delegation (token cost from Miser)
- [ ] Patroclus audit trail shows all decisions
- [ ] Kill switch stops the agent

## What We Need to Build

### In Hive (code changes):
1. Add `OPENROUTER_BASE_URL` env var support in `agent_app/main.py` `_resolve_llm()`
2. Add Patroclus client integration in agent registration (`routers/agent_api.py`)
3. Add Patroclus delegation on task dispatch (`services/delegation.py`)
4. Add Patroclus policy creation per agent

### In Relay (already done in Phase 3):
- Patroclus integration is already built — Relay calls `check_access()` before
  every tool dispatch

### In Patroclus (already done):
- All APIs are ready: agent registration, policy creation, delegation,
  request-access, audit, sessions, kill switch

### In Miser (already done):
- Miser is a transparent proxy — just needs to be running with a valid API key

## Execution Order

1. Start Patroclus → create policies → verify health
2. Start Miser → create API key → verify health
3. Start Relay (port 8090) → verify Patroclus connection
4. Make Hive code changes (env var + Patroclus integration)
5. Start Hive → register user → register agent (verify in Patroclus)
6. Register MCP server in Hive pointing at Relay
7. Submit task to Hive
8. Watch the flow: Hive → Agent → Miser (LLM) → Relay (tools) → Patroclus (authz)
9. Check audit trails in all components
10. Test kill switch
