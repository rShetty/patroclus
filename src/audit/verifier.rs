//! Offline verification of the audit hash chain.
//!
//! Every audit row commits to its full payload (see
//! [`AuditEntry::compute_hash`](super::AuditEntry::compute_hash)) and to its
//! predecessor's `row_hash`. The verifier replays both commitments over the
//! rows in insertion order and reports the first row that fails, so an
//! operator can answer "has the audit log been altered?" with a single
//! command.

use serde::Serialize;

use super::AuditEntry;

/// Outcome of verifying an audit chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainVerification {
    /// Number of rows examined.
    pub entries_checked: usize,
    /// `None` when every row is intact.
    pub first_broken_link: Option<BrokenLink>,
}

impl ChainVerification {
    pub fn is_valid(&self) -> bool {
        self.first_broken_link.is_none()
    }
}

/// The first row whose stored commitments do not match a recomputation.
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLink {
    /// `audit_log.id` of the offending row.
    pub row_id: i64,
    /// How the row failed its commitments.
    pub reason: BrokenLinkReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokenLinkReason {
    /// The stored `row_hash` does not match the recomputed hash of the row
    /// payload (the row was edited, or its hash was replaced).
    RowHashMismatch,
    /// The stored `prev_hash` does not equal the previous row's `row_hash`
    /// (rows were deleted, inserted, or reordered).
    PrevHashMismatch,
}

/// Recompute the SHA-256 chain over `entries` (in ascending insertion order)
/// and report the first broken link.
///
/// The genesis row must carry the well-known zero prev_hash; every later row
/// must chain to its predecessor's stored `row_hash`, and every row must
/// match its own recomputed payload hash.
pub fn verify_chain(entries: &[AuditEntry]) -> ChainVerification {
    const GENESIS_PREV_HASH: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";

    let mut prev_stored_hash: Option<&str> = None;
    for (index, entry) in entries.iter().enumerate() {
        let expected_prev = match prev_stored_hash {
            None => GENESIS_PREV_HASH,
            Some(prev) => prev,
        };
        if entry.prev_hash != expected_prev {
            return ChainVerification {
                entries_checked: index + 1,
                first_broken_link: Some(BrokenLink {
                    row_id: entry.id,
                    reason: BrokenLinkReason::PrevHashMismatch,
                }),
            };
        }

        if entry.compute_hash() != entry.row_hash {
            return ChainVerification {
                entries_checked: index + 1,
                first_broken_link: Some(BrokenLink {
                    row_id: entry.id,
                    reason: BrokenLinkReason::RowHashMismatch,
                }),
            };
        }

        prev_stored_hash = Some(entry.row_hash.as_str());
    }

    ChainVerification {
        entries_checked: entries.len(),
        first_broken_link: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::CreateAuditEntry;
    use crate::policy::Decision;
    use chrono::Utc;

    fn sample_entry(prev_hash: &str, id: i64, action: &str) -> AuditEntry {
        let mut entry = AuditEntry {
            id,
            prev_hash: prev_hash.to_string(),
            row_hash: String::new(),
            agent_id: uuid::Uuid::now_v7(),
            principal_id: None,
            action: action.to_string(),
            resource: "dev-db".to_string(),
            decision: "allow".to_string(),
            reason: "test".to_string(),
            delegation_chain: None,
            token_jti: None,
            dry_run: false,
            timestamp: Utc::now(),
        };
        entry.row_hash = entry.compute_hash();
        entry
    }

    fn create(action: &str) -> CreateAuditEntry {
        CreateAuditEntry {
            agent_id: uuid::Uuid::now_v7(),
            principal_id: None,
            action: action.to_string(),
            resource: "dev-db".to_string(),
            decision: Decision::Allow,
            reason: "test".to_string(),
            delegation_chain: None,
            token_jti: None,
            dry_run: false,
        }
    }

    #[test]
    fn empty_chain_is_valid() {
        let result = verify_chain(&[]);
        assert!(result.is_valid());
        assert_eq!(result.entries_checked, 0);
    }

    #[test]
    fn valid_multi_row_chain_passes() {
        let e1 = sample_entry(
            "0000000000000000000000000000000000000000000000000000000000000000",
            1,
            "read",
        );
        let e2 = sample_entry(&e1.row_hash, 2, "write");
        let e3 = sample_entry(&e2.row_hash, 3, "delete");

        let result = verify_chain(&[e1, e2, e3]);
        assert!(result.is_valid());
        assert_eq!(result.entries_checked, 3);
    }

    #[test]
    fn tampered_payload_is_detected() {
        let e1 = sample_entry(
            "0000000000000000000000000000000000000000000000000000000000000000",
            1,
            "read",
        );
        let mut e2 = sample_entry(&e1.row_hash, 2, "write");
        // Attacker edits the action after the fact without fixing the hash.
        e2.action = "exfiltrate".to_string();
        let e3 = sample_entry(&e2.row_hash, 3, "delete");

        let result = verify_chain(&[e1, e2, e3]);
        assert!(!result.is_valid());
        let broken = result.first_broken_link.unwrap();
        assert_eq!(broken.row_id, 2);
        assert_eq!(broken.reason, BrokenLinkReason::RowHashMismatch);
    }

    #[test]
    fn deleted_row_breaks_linkage() {
        let e1 = sample_entry(
            "0000000000000000000000000000000000000000000000000000000000000000",
            1,
            "read",
        );
        let e2 = sample_entry(&e1.row_hash, 2, "write");
        let e3 = sample_entry(&e2.row_hash, 3, "delete");

        // Attacker deletes row 2 — row 3 no longer chains to row 1.
        let e3_id = e3.id;
        let result = verify_chain(&[e1, e3]);
        assert!(!result.is_valid());
        let broken = result.first_broken_link.unwrap();
        assert_eq!(broken.row_id, e3_id);
        assert_eq!(broken.reason, BrokenLinkReason::PrevHashMismatch);
    }

    #[test]
    fn genesis_prev_hash_is_enforced() {
        let mut e1 = sample_entry("deadbeef", 1, "read");
        e1.row_hash = e1.compute_hash();
        let result = verify_chain(&[e1]);
        assert!(!result.is_valid());
        assert_eq!(
            result.first_broken_link.unwrap().reason,
            BrokenLinkReason::PrevHashMismatch
        );
    }

    #[test]
    fn dry_run_flag_is_hashed() {
        let mut honest = sample_entry(
            "0000000000000000000000000000000000000000000000000000000000000000",
            1,
            "read",
        );
        honest.row_hash = honest.compute_hash();

        // Flipping dry_run after hashing must invalidate the row hash.
        let mut forged = honest.clone();
        forged.dry_run = !forged.dry_run;
        assert_ne!(forged.compute_hash(), forged.row_hash);

        // Sanity: the honest entry still verifies.
        assert!(verify_chain(std::slice::from_ref(&honest)).is_valid());
    }

    #[test]
    fn db_roundtrip_preserves_chain() {
        // Build a chain through the real DB layer and verify it end-to-end.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let db = crate::db::Database::new(":memory:").unwrap();
            let mut rows = Vec::new();
            for action in ["read", "write", "delete"] {
                rows.push(db.create_audit_entry(&create(action)).await.unwrap());
            }

            let stored = db.list_audit_entries(100).await.unwrap();
            // list_audit_entries returns DESC; the verifier needs insert order.
            let mut ordered = stored;
            ordered.reverse();
            assert_eq!(ordered.len(), 3);

            let result = verify_chain(&ordered);
            assert!(result.is_valid(), "fresh chain must verify");
            assert_eq!(result.entries_checked, 3);
        });
    }
}
