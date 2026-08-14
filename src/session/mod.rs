use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{PatroclusError, Result};
use crate::policy::{Decision, TrajectoryEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub agent_id: Uuid,
    pub principal_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub trajectory: Vec<TrajectoryEvent>,
    pub actions_count: u64,
    pub spend_total: f64,
    pub tokens_used: u64,
    pub trust_level: f64,
    pub killed: bool,
}

impl SessionState {
    pub fn new(session_id: String, agent_id: Uuid, principal_id: Option<Uuid>) -> Self {
        let now = Utc::now();
        SessionState {
            session_id,
            agent_id,
            principal_id,
            created_at: now,
            last_activity: now,
            trajectory: Vec::new(),
            actions_count: 0,
            spend_total: 0.0,
            tokens_used: 0,
            trust_level: 1.0,
            killed: false,
        }
    }

    pub fn record_action(&mut self, event: TrajectoryEvent) {
        self.last_activity = Utc::now();
        self.actions_count += 1;
        self.trajectory.push(event);
        if self.trajectory.len() > 1000 {
            let drop_count = self.trajectory.len() - 1000;
            self.trajectory.drain(0..drop_count);
        }
    }

    pub fn record_spend(&mut self, amount: f64) {
        self.spend_total += amount;
    }

    pub fn record_tokens(&mut self, count: u64) {
        self.tokens_used += count;
    }

    pub fn minutes_since_last_activity(&self) -> i64 {
        (Utc::now() - self.last_activity).num_minutes()
    }

    pub fn apply_trust_decay(&mut self, decay_threshold_minutes: i64, decay_rate: f64) {
        let idle_minutes = self.minutes_since_last_activity();
        if idle_minutes > decay_threshold_minutes {
            let decay_periods = (idle_minutes - decay_threshold_minutes) / decay_threshold_minutes;
            self.trust_level = (1.0 - decay_periods as f64 * decay_rate).max(0.0);
        }
    }

    pub fn is_allowed_by_trust(&self, min_trust: f64) -> bool {
        self.trust_level >= min_trust
    }

    pub fn recent_actions(&self, window_minutes: i64) -> &[TrajectoryEvent] {
        let cutoff = Utc::now() - Duration::minutes(window_minutes);
        let start = self
            .trajectory
            .iter()
            .position(|e| e.timestamp >= cutoff)
            .unwrap_or(self.trajectory.len());
        &self.trajectory[start..]
    }

    pub fn count_actions_in_window(&self, window_minutes: i64) -> usize {
        self.recent_actions(window_minutes).len()
    }

    pub fn cumulative_spend(&self) -> f64 {
        self.spend_total
    }
}

pub struct SessionStore {
    sessions: RwLock<HashMap<String, SessionState>>,
    rate_limits: RwLock<HashMap<String, RateLimitState>>,
}

#[derive(Debug, Clone)]
struct RateLimitState {
    window_start: DateTime<Utc>,
    count: u64,
}

impl SessionStore {
    pub fn new() -> Self {
        SessionStore {
            sessions: RwLock::new(HashMap::new()),
            rate_limits: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_or_create_session(
        &self,
        session_id: &str,
        agent_id: Uuid,
        principal_id: Option<Uuid>,
    ) -> SessionState {
        let mut sessions = self.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            return s.clone();
        }
        let state = SessionState::new(session_id.to_string(), agent_id, principal_id);
        sessions.insert(session_id.to_string(), state.clone());
        state
    }

    pub fn get_session(&self, session_id: &str) -> Option<SessionState> {
        self.sessions.read().get(session_id).cloned()
    }

    pub fn record_action(&self, session_id: &str, event: TrajectoryEvent) {
        let mut sessions = self.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            s.record_action(event);
        }
    }

    pub fn record_spend(&self, session_id: &str, amount: f64) {
        let mut sessions = self.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            s.record_spend(amount);
        }
    }

    pub fn record_tokens(&self, session_id: &str, count: u64) {
        let mut sessions = self.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            s.record_tokens(count);
        }
    }

    pub fn kill_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            s.killed = true;
            return true;
        }
        false
    }

    pub fn is_killed(&self, session_id: &str) -> bool {
        self.sessions
            .read()
            .get(session_id)
            .map(|s| s.killed)
            .unwrap_or(false)
    }

    pub fn check_rate_limit(&self, key: &str, max_per_minute: u64) -> Result<()> {
        let mut limits = self.rate_limits.write();
        let now = Utc::now();
        let window = Duration::minutes(1);

        match limits.get_mut(key) {
            Some(state) => {
                if now - state.window_start > window {
                    state.window_start = now;
                    state.count = 1;
                    Ok(())
                } else {
                    state.count += 1;
                    if state.count > max_per_minute {
                        Err(PatroclusError::PolicyDenied {
                            reason: format!(
                                "Rate limit exceeded for {} ({} calls/min, limit {})",
                                key, state.count, max_per_minute
                            ),
                        })
                    } else {
                        Ok(())
                    }
                }
            }
            None => {
                limits.insert(
                    key.to_string(),
                    RateLimitState {
                        window_start: now,
                        count: 1,
                    },
                );
                Ok(())
            }
        }
    }

    pub fn apply_trust_decay_all(&self, decay_threshold_minutes: i64, decay_rate: f64) {
        let mut sessions = self.sessions.write();
        for s in sessions.values_mut() {
            s.apply_trust_decay(decay_threshold_minutes, decay_rate);
        }
    }

    pub fn list_sessions(&self) -> Vec<SessionState> {
        self.sessions.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_records_actions() {
        let mut session = SessionState::new("s1".to_string(), Uuid::now_v7(), None);
        let event = TrajectoryEvent {
            action: "read".to_string(),
            resource: "db/users".to_string(),
            decision: Decision::Allow,
            timestamp: Utc::now(),
        };
        session.record_action(event);
        assert_eq!(session.actions_count, 1);
        assert_eq!(session.trajectory.len(), 1);
    }

    #[test]
    fn test_rate_limiting_blocks() {
        let store = SessionStore::new();
        for _ in 0..5 {
            assert!(store.check_rate_limit("agent-1:read", 5).is_ok());
        }
        let result = store.check_rate_limit("agent-1:read", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_rate_limit_window_resets() {
        let store = SessionStore::new();
        for _ in 0..3 {
            store.check_rate_limit("key", 10).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(store.check_rate_limit("key", 10).is_ok());
    }

    #[test]
    fn test_kill_session() {
        let store = SessionStore::new();
        store.get_or_create_session("s1", Uuid::now_v7(), None);
        assert!(!store.is_killed("s1"));
        assert!(store.kill_session("s1"));
        assert!(store.is_killed("s1"));
    }

    #[test]
    fn test_trust_decay_after_inactivity() {
        let mut session = SessionState::new("s1".to_string(), Uuid::now_v7(), None);
        session.last_activity = Utc::now() - Duration::minutes(30);
        session.apply_trust_decay(15, 0.2);
        assert!(session.trust_level < 1.0);
    }

    #[test]
    fn test_spend_tracking() {
        let mut session = SessionState::new("s1".to_string(), Uuid::now_v7(), None);
        session.record_spend(10.0);
        session.record_spend(5.5);
        assert_eq!(session.cumulative_spend(), 15.5);
    }

    #[test]
    fn test_recent_actions_window() {
        let mut session = SessionState::new("s1".to_string(), Uuid::now_v7(), None);
        for i in 0..10 {
            session.record_action(TrajectoryEvent {
                action: "read".to_string(),
                resource: format!("db/{}", i),
                decision: Decision::Allow,
                timestamp: Utc::now(),
            });
        }
        assert_eq!(session.count_actions_in_window(60), 10);
    }
}
