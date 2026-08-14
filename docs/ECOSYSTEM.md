# Ecosystem: How It All Works Together

## The Six-Project Agent Governance Stack

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      AGENT GOVERNANCE ECOSYSTEM                          │
│                                                                         │
│    Hive          Patroclus       Relay          Miser                   │
│    ─────         ─────────       ─────          ─────                   │
│    Agent         Authz           MCP Proxy      Cost                    │
│    Runtime &     Infrastructure  & Tool         Optimization            │
│    Orchestration                  Gateway                               │
│                                                                         │
│    Sentiel        Aegis                                                 │
│    ───────        ─────                                                 │
│    Observability  Network                                               │
│    DLP &          Enforcement                                           │
│    Compliance     & Attestation                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

| Component | Port | Role | Repo |
|-----------|------|------|------|
| Hive | 8000 | Agent runtime & orchestration | [rShetty/hive](https://github.com/rShetty/hive) |
| Patroclus | 8484 | Authorization infrastructure | [rShetty/patroclus](https://github.com/rShetty/patroclus) |
| Relay | 8090 | MCP gateway & tool proxy | [rShetty/relay](https://github.com/rShetty/relay) |
| Miser | 8787 | LLM cost optimization | [rShetty/miser](https://github.com/rShetty/miser) |
| Sentiel | 8585 | Observability, DLP, compliance | [rShetty/sentiel](https://github.com/rShetty/sentiel) |
| Aegis | 8686 | Network egress & attestation | [rShetty/Aegis](https://github.com/rShetty/Aegis) |

## Quick Start

```bash
# Start all 6 services
~/patroclus/scripts/start-ecosystem.sh start

# Check status
~/patroclus/scripts/start-ecosystem.sh status

# Run e2e test
python3 ~/patroclus/scripts/e2e_test.py

# Stop everything
~/patroclus/scripts/start-ecosystem.sh stop
```

## Dashboards

- **Sentiel**: http://localhost:8585 — Real-time agent governance dashboard
- **Hive**: http://localhost:8000 — Agent marketplace
- **Patroclus**: http://localhost:8484/health — Authorization health
- **Aegis**: http://localhost:8686/health — Network enforcement health

## How They Connect

### 1. Hive ↔ Patroclus: Agent Registration
When an agent is registered in Hive, Hive auto-registers it with Patroclus
(principal + agent + default policy with rate limits and approval requirements).

### 2. Relay ↔ Patroclus: Per-Tool Authorization
Relay calls Patroclus `check_access()` before every MCP tool dispatch.
Fail-closed: if Patroclus is unreachable, tool calls are denied.

### 3. Hive Agent ↔ Miser: LLM Cost Optimization
Hive agents route LLM calls through Miser via `OPENROUTER_BASE_URL` env var.
Miser classifies complexity and routes to the cheapest capable model.

### 4. All Components → Sentiel: Telemetry & DLP
Each component sends events to Sentiel:
- **Patroclus**: authorization decisions (allow/deny/approval)
- **Relay**: tool call inputs/outputs (DLP-inspected)
- **Miser**: LLM cost events (model, tokens, cost)
- **Hive**: agent activity (delegation, heartbeat, completion)

Sentiel correlates by `session_id`, detects anomalies (spending spikes,
high denial rates, DLP violations), and generates compliance reports
(SOC2, GDPR, EU AI Act, HIPAA).

### 5. Aegis ↔ Agent Processes: Network Enforcement
Aegis acts as an HTTP proxy for agent processes. It:
- Blocks egress to unauthorized destinations (default-deny)
- Verifies agent runtime integrity via SHA-256 binary attestation
- Enforces data residency (block requests to non-compliant regions)
- Logs all network requests for audit

Agents are configured with:
```bash
export HTTP_PROXY=http://localhost:8686
export HTTPS_PROXY=http://localhost:8686
```

### 6. Patroclus ↔ Sentiel: Compliance Evidence
Patroclus's hash-chained audit log feeds into Sentiel's compliance reports.
Sentiel maps audit entries to compliance controls:
- SOC2 CC6.1 (Access Controls) ← Patroclus authz decisions
- SOC2 CC7.1 (System Monitoring) ← All component events
- SOC2 CC8.1 (Change Management) ← Patroclus approval workflow
- GDPR Art.30 (Records of Processing) ← All agent actions logged
- EU AI Act Art.14 (Human Oversight) ← Patroclus require_approval decisions
- HIPAA 164.312(b) (Audit Controls) ← Hash-chained audit trail

## End-to-End Flow

```
1. User submits task to Hive
2. Hive dispatches to agent (registers with Patroclus if new)
3. Agent makes LLM call → Miser (classifies, routes, caches, reports cost to Sentiel)
4. Agent makes tool call → Relay
   → Relay checks with Patroclus (allow/deny/approval)
   → Relay inspects output with Sentiel DLP engine
   → If allowed, Relay forwards to upstream
   → Relay sends tool call event to Sentiel
5. Agent network traffic → Aegis (egress policy check, geo check)
6. All events → Sentiel (correlated by session_id)
7. Sentiel dashboard shows real-time activity
8. Sentiel generates compliance reports on demand
9. Patroclus audit trail (hash-chained) provides tamper-evidence
10. Kill switch (Patroclus) → blocks all future access
    Aegis → blocks all network egress
    Sentiel → alerts on the kill event
```

## E2E Test Results (Verified)

| Test | Result |
|------|--------|
| All 6 services healthy | ✓ |
| Hive agent → Patroclus auto-registration | ✓ |
| Patroclus policy auto-created | ✓ |
| Read access ALLOWED with JWT | ✓ |
| Production deploy → REQUIRE_APPROVAL | ✓ |
| Human approval workflow | ✓ |
| Rate limiting enforced | ✓ |
| Audit trail hash-chained | ✓ |
| Session tracking (trajectory, trust) | ✓ |
| Kill switch blocks access | ✓ |
| Relay connected to Patroclus | ✓ |
| Miser ready for LLM routing | ✓ |
| Sentiel DLP detects SSN in tool output | ✓ |
| Sentiel anomaly alert on DLP violation | ✓ |
| Sentiel SOC2 compliance report generated | ✓ |
| Aegis default-deny blocks unknown host | ✓ |
| Aegis allow policy permits authorized host | ✓ |
| Aegis attestation verifies binary hash | ✓ |

## What Each Component Prevents

| Threat | Prevented By |
|--------|-------------|
| Agent calls unauthorized tool | Patroclus (policy deny) + Relay (intercepts) |
| Agent accesses production data | Patroclus (require_approval) |
| Agent exfiltrates data to external API | Aegis (egress deny) |
| Agent makes too many calls | Patroclus (rate limiting) |
| Agent overspends on LLM | Patroclus (budget cap) + Miser (cheap routing) |
| Agent runs with stale credentials | Patroclus (token expiry + revocation) |
| Agent runtime tampered | Aegis (attestation hash mismatch) |
| Sensitive data in tool output | Sentiel (DLP detection + redaction) |
| Data sent to non-compliant region | Aegis (geo enforcement) |
| Agent stuck in denial loop | Sentiel (anomaly alert) |
| No audit trail for compliance | Patroclus (hash-chained) + Sentiel (reports) |
| Agent goes rogue | Patroclus (kill switch) + Aegis (egress block) |
