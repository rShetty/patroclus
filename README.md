# Patroclus

**Scoped, time-limited authorization infrastructure for AI agents.**

In the Iliad, Achilles delegated his armor and authority to Patroclus with explicit
scope: drive the Trojans from the ships, then return. Patroclus exceeded his scope,
pursued too far, and the consequences were catastrophic. Patroclus (the project)
ensures that never happens to your AI agents.

---

## What is this?

Patroclus is a self-hostable authorization control plane for AI agents. It sits
between agents and the resources they need to access — APIs, MCP servers,
databases, cloud services — and enforces scoped, time-limited, revocable
permissions with human-in-the-loop approval workflows.

Agents today are handed static API keys and long-lived tokens. Patroclus replaces
that with **just-in-time access**: an agent requests access to a specific resource
for a specific action, policies are evaluated in real-time, and either a
short-lived scoped credential is issued or a human approval is triggered.

## Core Capabilities

- **Just-in-time credential issuance** — agents get scoped, short-lived tokens
  (5–15 min) only when a policy allows it. No standing permissions.
- **Delegated authorization** — humans delegate scoped authority to agents, with
  monotonic attenuation (sub-agents can only receive narrower scopes, never wider).
- **Human approval workflows** — when policy requires it, approval requests are
  routed to the resource owner or admin. Approval tokens are single-use and
  action-scoped.
- **Per-call policy enforcement** — every tool call is evaluated against policy
  before it reaches the target. Default-deny: no matching policy means deny.
- **MCP gateway** — intercepts MCP `tools/call` and filters `tools/list` based on
  the agent's credential. Agent tokens never reach upstream MCP servers.
- **Credential vault** — upstream provider credentials (GitHub, Slack, AWS, etc.)
  are encrypted at rest. Agents receive fresh, scoped tokens via RFC 8693 token
  exchange. Agents never see raw secrets.
- **Temporal/trajectory-aware policies** — evaluate requests in the context of
  prior actions in the session. Enforce workflow sequencing, cumulative budget
  caps, and progressive trust decay.
- **Tamper-evident audit trail** — hash-chained log of every decision (allow, deny,
  require-approval) with full delegation chain attribution.
- **Cascade revocation** — revoking a parent grant atomically invalidates all
  child grants and issued tokens.
- **Kill switch** — emergency agent termination and credential revocation.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        PATROCLUS                                     │
│                                                                      │
│  ┌──────────┐   ┌──────────────┐   ┌─────────────┐   ┌──────────┐  │
│  │  Agent    │──▶│  Request     │──▶│  Policy     │──▶│ Decision │  │
│  │  Runtime  │   │  Gateway     │   │  Engine     │   │  Router  │  │
│  └──────────┘   └──────────────┘   └─────────────┘   └─────┬────┘  │
│                       │                                      │       │
│                       │              ┌────────────┐    ┌─────▼────┐  │
│                       │              │  Approval  │    │  Token   │  │
│                       │              │  Service   │◀──│  Issuer  │  │
│                       │              └──────┬─────┘    └─────┬────┘  │
│                       │                     │                │       │
│  ┌──────────┐    ┌────▼─────┐   ┌───────────┴──┐    ┌───────▼────┐ │
│  │  Human   │◀───│  Notify  │   │  Audit Log    │   │  Credential│ │
│  │  Approver│───▶│  Service │   │  (Hash-chain) │   │  Vault     │ │
│  └──────────┘    └──────────┘   └──────────────┘   └────────────┘ │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │              Policy Store (YAML / OPA / Cedar)               │   │
│  └──────────────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    Resource Registry                          │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

## Token Model

Patroclus issues JWTs (RS256) with agent-specific claims:

```json
{
  "iss": "https://patroclus.example.com",
  "sub": "user:alice@example.com",
  "act": {
    "sub": "agent:agent_001",
    "delegation_depth": 0
  },
  "scope": "db:read:prod-db/users",
  "aud": "resource:prod-db",
  "exp": 1723643820,
  "iat": 1723642920,
  "jti": "01J5Q3Z...",
  "constraints": {
    "max_rows": 1000,
    "time_window": "weekdays 08:00-18:00 CST"
  }
}
```

- `sub` — the human principal who delegated authority
- `act` — the agent identity and delegation chain
- `scope` — approved scopes (always a subset of what was requested)
- `aud` — audience-bound to the target resource (RFC 8707)
- `exp` — short expiry (5–15 minutes by default)
- `jti` — unique token ID for replay protection and revocation
- `constraints` — machine-evaluable limits enforced at the resource

## Getting Started

```bash
# Build
cargo build --release

# Run
./target/release/patroclus serve --config config.toml

# Initialize default config
./target/releases/patroclus init
```

## Status

**Early development.** See [PLAN.md](./PLAN.md) for the full architecture and
implementation roadmap.

## License

MIT
