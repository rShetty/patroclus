//! Prometheus metrics for the Patroclus control plane.
//!
//! Exposed at `GET /metrics` in the Prometheus text exposition format.
//! Metric families:
//!
//! * `patroclus_authz_decisions_total` — counter of authorization decisions,
//!   labelled by `outcome` (`allow`, `deny`, `require_approval`) and the
//!   requested `action`.
//! * `patroclus_request_duration_seconds` — histogram of HTTP request
//!   latency across all routes.
//! * `patroclus_active_sessions` — gauge of live agent sessions.
//! * `patroclus_approval_queue_depth` — gauge of pending approval requests.
//! * `patroclus_tokens_issued_total` — counter of issued access tokens.

use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry,
    TextEncoder,
};

/// Container for all metric families plus their registry.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    /// Authorization decisions by outcome/action.
    pub authz_decisions: IntCounterVec,
    /// HTTP request latency by method/path label.
    pub request_duration: HistogramVec,
    /// Live agent sessions tracked by the session store.
    pub active_sessions: IntGauge,
    /// Approval requests currently pending a principal decision.
    pub approval_queue_depth: IntGauge,
    /// Access tokens issued via request-access / delegation flows.
    pub tokens_issued: IntCounter,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Register every family with an anonymous (process-local) registry.
    pub fn new() -> Self {
        let registry = Registry::new();

        let authz_decisions = IntCounterVec::new(
            Opts::new(
                "patroclus_authz_decisions_total",
                "Authorization decisions evaluated by the policy engine",
            ),
            &["outcome", "action"],
        )
        .expect("valid authz decision counter definition");
        registry
            .register(Box::new(authz_decisions.clone()))
            .expect("authz decisions registration");

        let request_duration = HistogramVec::new(
            HistogramOpts::new(
                "patroclus_request_duration_seconds",
                "HTTP request latency in seconds",
            )
            .buckets(vec![
                0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ]),
            &["method", "path"],
        )
        .expect("valid request duration histogram definition");
        registry
            .register(Box::new(request_duration.clone()))
            .expect("request duration registration");

        let active_sessions =
            IntGauge::new("patroclus_active_sessions", "Currently live agent sessions")
                .expect("valid sessions gauge definition");
        registry
            .register(Box::new(active_sessions.clone()))
            .expect("active sessions registration");

        let approval_queue_depth = IntGauge::new(
            "patroclus_approval_queue_depth",
            "Approval requests waiting on a principal decision",
        )
        .expect("valid approval queue gauge definition");
        registry
            .register(Box::new(approval_queue_depth.clone()))
            .expect("approval queue depth registration");

        let tokens_issued = IntCounter::new(
            "patroclus_tokens_issued_total",
            "Access tokens issued by the control plane",
        )
        .expect("valid token issuance counter definition");
        registry
            .register(Box::new(tokens_issued.clone()))
            .expect("tokens issued registration");

        Metrics {
            registry,
            authz_decisions,
            request_duration,
            active_sessions,
            approval_queue_depth,
            tokens_issued,
        }
    }

    /// Record one authorization outcome (`allow` | `deny` | `require_approval`).
    pub fn record_decision(&self, outcome: &str, action: &str) {
        self.authz_decisions
            .with_label_values(&[outcome, action])
            .inc();
    }

    /// Render all metrics in the Prometheus text exposition format.
    pub fn gather(&self) -> String {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder
            .encode(&self.registry.gather(), &mut buffer)
            .expect("metrics encoding cannot fail for TextEncoder");
        String::from_utf8(buffer).expect("text format is UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_increment_on_decisions() {
        let metrics = Metrics::new();

        metrics.record_decision("allow", "read");
        metrics.record_decision("allow", "read");
        metrics.record_decision("deny", "write");

        let rendered = metrics.gather();
        assert!(rendered.contains("patroclus_authz_decisions_total"));
        assert!(
            rendered.contains(r#"action="read",outcome="allow"} 2"#),
            "allow/read should be 2:\n{rendered}"
        );
        assert!(
            rendered.contains(r#"action="write",outcome="deny"} 1"#),
            "deny/write should be 1:\n{rendered}"
        );
    }

    #[test]
    fn gauges_and_token_counter_render() {
        let metrics = Metrics::new();
        metrics.active_sessions.set(3);
        metrics.approval_queue_depth.set(5);
        metrics.tokens_issued.inc_by(7);

        let rendered = metrics.gather();
        assert!(rendered.contains("patroclus_active_sessions 3"));
        assert!(rendered.contains("patroclus_approval_queue_depth 5"));
        assert!(rendered.contains("patroclus_tokens_issued_total 7"));
    }
}
