# Patroclus — High-Level Design (HLD)

## 1. Overview

Patroclus is a scoped, time-limited authorization infrastructure for AI agents.
It sits between agents and the resources they need to access — APIs, MCP servers,
databases, cloud services — and enforces policy-driven, just-in-time access with
human-in-the-loop approval workflows.

**Origin**: Named after Patroclus from the Iliad, who was delegated scoped authority
by Achilles (drive the Trojans from the ships, then return). Patroclus exceeded his
scope, pursued too far, and the consequences were catastrophic. This system ensures
that never happens to your AI agents.

## 2. Problem Statement

AI agents today are handed static API keys and long-lived tokens. This is
inappropriate for ephemeral, autonomous agents that:

- Need **just-in-time** access to specific resources for specific actions
- Act on **delegated behalf** of human users (with scoping)
- Have their **own identity** (not just shared credentials)
- Require **human approval** for sensitive or out-of-policy actions
- Need credentials that **expire automatically** (5–15 min) and are **revocable**
- May **delegate to sub-agents** with narrowed scope (monotonic attenuation)

## 3. System Context

```
┌─────────────────────────────────────────────────────────────────┐
│                        AGENT ECOSYSTEM                          │
│                                                                 │
│  ┌─────────┐     ┌──────────┐     ┌──────────┐     ┌────────┐ │
│  │  Agent   │────▶│  Relay   │────▶│Patroclus │────▶│Resource│ │
│  │ Runtime  │     │  (MCP    │     │  (Authz) │     │  (API/ │ │
│  │ (Hive)   │     │  Proxy)  │     │          │     │  DB)   │ │
│  └─────────┘     └──────────┘     └──────────┘     └────────┘ │
│       │               │                │                  │     │
│       │               │                ▼                  │     │
│       │               │         ┌────────────┐           │     │
│       │               │         │  Policy    │           │     │
│       │               │         │  Engine    │           │     │
│       │               │         │  (YAML/OPA)│           │     │
│       │               │         └────────────┘           │     │
│       │               │                │                  │     │
│       │               │                ▼                  │     │
│       │               │         ┌────────────┐           │     │
│       │               │         │  Audit Log │           │     │
│       │               │         │  (Hash     │           │     │
│       │               │         │  Chained)  │           │     │
│       │               │         └────────────┘           │     │
│       │               │                                  │     │
│       │          ┌────▼─────┐                           │     │
│       │          │ Credential│                           │     │
│       │          │  Vault    │                           │     │
│       │          │ (AES-256) │                           │     │     │
│       │          └──────────┘                            │     │
│       │                                                   │     │
│  ┌────▼──────────────┐                                  │     │
│  │  Miser            │  Budget & spend tracking          │     │
│  │  (Cost Control)   │  for agent operations             │     │
│  └───────────────────┘                                   │     │
└─────────────────────────────────────────────────────────┘
```

## 4. Core Principles

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

## 5. Major Components

### 5.1 Request Gateway
Intercepts every agent tool call / resource access request. Authenticates agent
identity, extracts context, forwards to Policy Engine.

### 5.2 Policy Engine
Evaluates `(agent, human_principal, action, resource, context)` →
`ALLOW | DENY | REQUIRE_APPROVAL`. Supports YAML rules with temporal conditions
(rate limiting, budget caps, trust decay, workflow sequencing, max actions).

### 5.3 Token Issuer
Mints short-lived JWTs (RS256) with `sub` (human), `act` (agent + delegation chain),
`scope`, `aud`, `exp`, `jti`. Supports DPoP binding and revocation.

### 5.4 Credential Vault
AES-256-GCM encrypted storage for upstream provider credentials (GitHub, Google,
Slack). Vends fresh scoped tokens via OAuth refresh token exchange. Agents never
see raw secrets.

### 5.5 Approval Service
Manages approval lifecycle: request → review → approve/deny → notify. Approval
tokens are single-use, action-scoped, time-limited.

### 5.6 Session Store
In-memory per-session tracking: trajectory, action count, spend, token usage,
trust level, killed flag. Enables temporal policies and kill switch.

### 5.7 Audit Log
Hash-chained, tamper-evident log of every decision. Attribution-complete with
full delegation chain.

### 5.8 Policy Store
Versioned policy definitions (YAML). Hot-reloadable. Stored in SQLite.

### 5.9 Resource Registry
Catalog of all protected resources with actions, required scopes, sensitivity,
and owner (for approvals).

## 6. Token Model

```json
{
  "iss": "https://patroclus.example.com",
  "sub": "user:alice@example.com",
  "act": {
    "sub": "agent:agent_001",
    "delegation_depth": 0,
    "delegation_chain": [...]
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

## 7. Delegation Model

```
Human (Alice) delegates [calendar:read, calendar:write, email:send] to Agent_001
  │
  ├─▶ Sub-Agent A gets [calendar:read]           ← subset ✓
  │    expiry = min(parent_exp, +10min)
  │
  ├─▶ Sub-Agent B gets [calendar:read, calendar:write] ← subset ✓
  │
  └─▶ Sub-Agent C tries [admin:all]              ← REJECTED (escalation)
```

Revoking Agent_001 → cascades to all descendants atomically.

## 8. Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Web framework | Axum 0.8 |
| Database | SQLite (rusqlite, bundled) |
| Token format | JWT RS256 (jsonwebtoken) |
| Encryption | AES-256-GCM (aes-gcm) |
| Policy engine | YAML rules (pluggable: OPA/Cedar ready) |
| Key generation | RSA 2048 (rsa crate) |
| Python SDK | httpx, pip-installable |

## 9. Deployment Models

- **Standalone**: Single binary, SQLite, ephemeral keys (dev)
- **Docker**: Multi-stage build, docker-compose with Relay
- **Production**: Binary + PostgreSQL + Redis + persistent keys (roadmap)
