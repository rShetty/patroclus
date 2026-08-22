# Prometheus metrics

Patroclus exposes Prometheus metrics at **`GET /metrics`** in the standard
text exposition format (`Content-Type: text/plain; version=0.0.4`).

The endpoint is intentionally unauthenticated and returns no sensitive data
(only aggregate counters/gauges/histograms), matching the convention of the
unauthenticated `/health` endpoints. If you must protect it, do so at the
reverse proxy / network-policy layer, since the in-process auth middleware
treats it as a public path.

## Metric families

| Metric | Type | Labels | Description |
| --- | --- | --- | --- |
| `patroclus_authz_decisions_total` | counter | `outcome`, `action` | Authorization decisions evaluated by the policy engine. `outcome` is `allow`, `deny` or `require_approval`; `action` is the requested action string. |
| `patroclus_request_duration_seconds` | histogram | `method`, `path` | HTTP request latency in seconds for every route. The `path` label is the matched route *template* (e.g. `/v1/admin/agents/{id}`), keeping cardinality bounded. |
| `patroclus_active_sessions` | gauge | — | Live agent sessions currently tracked by the session store. Sampled on scrape. |
| `patroclus_approval_queue_depth` | gauge | — | Approval requests waiting on a principal decision. Sampled on scrape. |
| `patroclus_tokens_issued_total` | counter | — | Access tokens issued by `/v1/agent/request-access` and delegation flows. |

## Example scrape

```prometheus
# HELP patroclus_authz_decisions_total Authorization decisions evaluated by the policy engine
# TYPE patroclus_authz_decisions_total counter
patroclus_authz_decisions_total{action="read",outcome="allow"} 41
patroclus_authz_decisions_total{action="write",outcome="deny"} 3
# HELP patroclus_request_duration_seconds HTTP request latency in seconds
# TYPE patroclus_request_duration_seconds histogram
patroclus_request_duration_seconds_bucket{method="POST",path="/v1/agent/request-access",le="0.005"} 39
...
# HELP patroclus_active_sessions Currently live agent sessions
# TYPE patroclus_active_sessions gauge
patroclus_active_sessions 7
# HELP patroclus_approval_queue_depth Approval requests waiting on a principal decision
# TYPE patroclus_approval_queue_depth gauge
patroclus_approval_queue_depth 1
# HELP patroclus_tokens_issued_total Access tokens issued by the control plane
# TYPE patroclus_tokens_issued_total counter
patroclus_tokens_issued_total 38
```

## Scraping with Prometheus

```yaml
scrape_configs:
  - job_name: patroclus
    scrape_interval: 15s
    static_configs:
      - targets: ["patroclus.internal:8484"]
    metrics_path: /metrics
```

## Useful queries

```promql
# Decision outcomes over the last 5 minutes, by outcome
sum by (outcome) (rate(patroclus_authz_decisions_total[5m]))

# Deny ratio per action
sum by (action) (rate(patroclus_authz_decisions_total{outcome="deny"}[5m]))
/
sum by (action) (rate(patroclus_authz_decisions_total[5m]))

# p95 request latency
histogram_quantile(0.95,
  sum by (le, path) (rate(patroclus_request_duration_seconds_bucket[5m])))

# Tokens issued per second
rate(patroclus_tokens_issued_total[5m])
```

## Implementation notes

* All families live in a process-local registry (`src/metrics/mod.rs`) that is
  shared through `AppState.metrics`; each `AppState::new`/`new_test` gets its
  own registry so tests are isolated.
* Gauges (`active_sessions`, `approval_queue_depth`) are computed at scrape
  time from live state rather than maintained incrementally.
* Counter/histogram behaviour is covered by unit tests in
  `src/metrics/mod.rs` and end-to-end tests in `tests/metrics.rs`
  (including assertions that allow/deny decisions increment the labelled
  counters exactly once).
