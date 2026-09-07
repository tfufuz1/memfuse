// FILE-CONTEXT
// ZWECK: Bi-temporaler Validity-Filter für Post-RRF Fusion-Ergebnisse in Retrieval-Pipelines.
// INVARIANTEN: Bi-temporale UND-Verknüpfung (System- & Businesszeit); [valid_from, valid_until) Intervallgrenzen; Fail-Open für Alt-Daten ohne Metadaten.
// STAND: TS:2026-09-07T12:00:00Z

//! Bi-temporal validity filtering module for search & fusion results.
//!
//! Evaluates candidate validity against both transaction/system time (`TxId` / MVCC)
//! and business time (`[valid_from, valid_until)` interval semantics per ADR-033/ADR-038).

use memfuse_core::TxId;
use serde_json::Value;

/// Type alias for search & fusion results in post-retrieval pipelines.
pub type FusionResult = crate::SearchResult;

/// Extracted bi-temporal validity window representation for a document or search candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidityWindow {
    /// Start of transaction validity (system time / MVCC).
    pub tx_valid_from: Option<TxId>,
    /// End of transaction validity (system time / MVCC, exclusive upper bound).
    pub tx_valid_to: Option<TxId>,
    /// Start of business validity (Unix timestamp in ms/seconds).
    pub business_valid_from: Option<i64>,
    /// End of business validity (Unix timestamp in ms/seconds, exclusive upper bound).
    pub business_valid_to: Option<i64>,
}

impl ValidityWindow {
    /// Returns `true` if no validity window metadata constraints are set.
    ///
    /// Candidates without validity window metadata (legacy data) are treated as always valid
    /// (Fail-Open behavior for backward compatibility).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tx_valid_from.is_none()
            && self.tx_valid_to.is_none()
            && self.business_valid_from.is_none()
            && self.business_valid_to.is_none()
    }

    /// Evaluates bi-temporal validity using strict AND logic across system transaction time and business time.
    ///
    /// Intervall-Semantik: `[valid_from, valid_until)` (inclusive Start, exclusive Ende).
    pub fn is_valid_bitemporal(&self, as_of_tx: TxId, as_of_business: Option<u64>) -> bool {
        if self.is_empty() {
            return true;
        }

        // 1. Transaction / System time check (ADR-033: tx_valid_from <= as_of_tx < tx_valid_to)
        if let Some(vf) = self.tx_valid_from {
            if as_of_tx < vf {
                return false;
            }
        }
        if let Some(vt) = self.tx_valid_to {
            if as_of_tx >= vt {
                return false;
            }
        }

        // 2. Business time check (business_valid_from <= as_of_business < business_valid_to)
        if let Some(b_as_of) = as_of_business {
            let b_as_of = b_as_of as i64;
            if let Some(bvf) = self.business_valid_from {
                if b_as_of < bvf {
                    return false;
                }
            }
            if let Some(bvt) = self.business_valid_to {
                if b_as_of >= bvt {
                    return false;
                }
            }
        }

        true
    }
}

/// Helper function to parse a numeric or string value as a `u64`.
fn parse_u64_val(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        Some(n)
    } else if let Some(n) = v.as_i64() {
        if n >= 0 {
            Some(n as u64)
        } else {
            None
        }
    } else if let Some(s) = v.as_str() {
        s.trim().parse::<u64>().ok()
    } else {
        None
    }
}

/// Helper function to parse a numeric or string value as an `i64`.
fn parse_i64_val(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        Some(n)
    } else if let Some(n) = v.as_u64() {
        i64::try_from(n).ok()
    } else if let Some(s) = v.as_str() {
        s.trim().parse::<i64>().ok()
    } else {
        None
    }
}

/// Extracts bi-temporal validity window metadata from candidate document metadata.
pub fn extract_validity_window(metadata: Option<&Value>) -> ValidityWindow {
    let Some(meta) = metadata else {
        return ValidityWindow::default();
    };
    let Some(obj) = meta.as_object() else {
        return ValidityWindow::default();
    };

    // Explicit system transaction validity fields (ADR-033)
    let tx_vf = obj
        .get("tx_valid_from")
        .and_then(parse_u64_val)
        .map(TxId::new);

    let tx_vt = obj
        .get("tx_valid_to")
        .or_else(|| obj.get("tx_valid_until"))
        .and_then(parse_u64_val)
        .map(TxId::new);

    // Business validity fields (accepting explicit business_valid_* and generic valid_from/valid_to/valid_until)
    let bus_vf = obj
        .get("business_valid_from")
        .or_else(|| obj.get("valid_from"))
        .and_then(parse_i64_val);

    let bus_vt = obj
        .get("business_valid_to")
        .or_else(|| obj.get("business_valid_until"))
        .or_else(|| obj.get("valid_to"))
        .or_else(|| obj.get("valid_until"))
        .and_then(parse_i64_val);

    ValidityWindow {
        tx_valid_from: tx_vf,
        tx_valid_to: tx_vt,
        business_valid_from: bus_vf,
        business_valid_to: bus_vt,
    }
}

/// Applies bi-temporal validity filtering to candidates post-RRF fusion.
///
/// Filters out candidate results that are invalid relative to the current transaction `current_tx`
/// or business timestamp `query_timestamp`.
/// Candidates without validity window metadata (legacy data) are retained (Fail-Open).
pub fn apply_temporal_validity_filter(
    results: Vec<crate::SearchResult>,
    current_tx: TxId,
    query_timestamp: Option<u64>,
) -> Vec<crate::SearchResult> {
    results
        .into_iter()
        .filter(|res| {
            let window = extract_validity_window(res.metadata.as_ref());
            window.is_valid_bitemporal(current_tx, query_timestamp)
        })
        .collect()
}

/// Applies historical "as-of" bi-temporal validity filtering to candidates post-RRF fusion.
///
/// Evaluates candidate validity at historical point-in-time `as_of_timestamp` (analogous to Graphitis Episode-Pinning).
/// A candidate is retained ONLY IF `as_of_timestamp` lies within `[valid_from, valid_until)` AND
/// transaction validity (`tx_valid_to`, ADR-033) is active at that historical timestamp.
/// Candidates without validity window metadata (legacy data) are retained (Fail-Open for backward compatibility).
pub fn apply_temporal_validity_filter_at(
    results: Vec<crate::SearchResult>,
    as_of_timestamp: u64,
) -> Vec<crate::SearchResult> {
    let as_of_tx = TxId::new(as_of_timestamp);
    results
        .into_iter()
        .filter(|res| {
            let window = extract_validity_window(res.metadata.as_ref());
            window.is_valid_bitemporal(as_of_tx, Some(as_of_timestamp))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_candidate(id: &str, metadata: Option<serde_json::Value>) -> crate::SearchResult {
        crate::SearchResult {
            id: id.to_string(),
            score: 0.9,
            metadata,
            matched_signals: vec!["test".to_string()],
            provenance: None,
        }
    }

    #[test]
    fn test_temporal_filter_valid_until_in_past_filtered_out() {
        let candidate_a = make_candidate(
            "doc-expired",
            Some(json!({
                "valid_until": 100
            })),
        );
        let candidate_b = make_candidate(
            "doc-valid",
            Some(json!({
                "valid_until": 200
            })),
        );

        let results = vec![candidate_a, candidate_b];
        let current_tx = TxId::new(1);
        let query_timestamp = Some(150);

        let filtered = apply_temporal_validity_filter(results, current_tx, query_timestamp);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "doc-valid");
    }

    #[test]
    fn test_temporal_filter_fail_open_without_metadata() {
        let candidate_no_meta = make_candidate("doc-no-meta", None);
        let candidate_other_meta = make_candidate(
            "doc-other-meta",
            Some(json!({ "topic": "rust", "author": "alice" })),
        );

        let results = vec![candidate_no_meta, candidate_other_meta];
        let current_tx = TxId::new(10);
        let query_timestamp = Some(500);

        let filtered = apply_temporal_validity_filter(results, current_tx, query_timestamp);
        assert_eq!(
            filtered.len(),
            2,
            "Candidates without validity metadata must fail-open (retained)"
        );
        assert_eq!(filtered[0].id, "doc-no-meta");
        assert_eq!(filtered[1].id, "doc-other-meta");
    }

    #[test]
    fn test_apply_temporal_validity_filter_at_historical_superseded_chunk() {
        // Old chunk A: valid for business/system time [10, 50)
        let old_chunk = make_candidate(
            "chunk-v1-old",
            Some(json!({
                "tx_valid_from": 10,
                "tx_valid_to": 50,
                "valid_from": 10,
                "valid_until": 50
            })),
        );

        // New chunk B: valid for business/system time [50, None) - supersedes chunk A at t=50
        let new_chunk = make_candidate(
            "chunk-v2-new",
            Some(json!({
                "tx_valid_from": 50,
                "valid_from": 50
            })),
        );

        let results = vec![old_chunk, new_chunk];

        // Query historical point in time at t = 30 (when old chunk was active, before supersedes at t=50)
        let historical_results = apply_temporal_validity_filter_at(results, 30);
        assert_eq!(
            historical_results.len(),
            1,
            "At historical t=30, only old chunk must be returned"
        );
        assert_eq!(
            historical_results[0].id, "chunk-v1-old",
            "The old (now superseded) chunk must be returned for historical point-in-time query"
        );
    }

    #[test]
    fn test_bitemporal_and_logic_business_valid_tx_invalid() {
        // Candidate refers to a historical business event in window [10, 100),
        // but was only ingested into the system at transaction tx_valid_from = 50.
        let candidate = make_candidate(
            "doc-late-ingested",
            Some(json!({
                "business_valid_from": 10,
                "business_valid_to": 100,
                "tx_valid_from": 50
            })),
        );

        let results = vec![candidate];

        // Query at as_of_timestamp = 30.
        // Business check: 10 <= 30 < 100 -> PASS.
        // Transaction check: 30 < tx_valid_from (50) -> FAIL.
        // Bi-temporal AND logic must filter out this candidate!
        let filtered = apply_temporal_validity_filter_at(results, 30);
        assert!(
            filtered.is_empty(),
            "Candidate with valid business window but invalid transaction window at query time must be filtered out"
        );
    }

    #[test]
    fn test_validity_window_exact_boundary_semantics() {
        // [10, 100)
        let candidate = make_candidate(
            "doc-boundary",
            Some(json!({
                "valid_from": 10,
                "valid_until": 100
            })),
        );

        // Before start (< 10) -> Invalid
        let res_before = apply_temporal_validity_filter_at(vec![candidate.clone()], 9);
        assert!(res_before.is_empty());

        // Exact start (10) -> Valid (inclusive lower bound)
        let res_start = apply_temporal_validity_filter_at(vec![candidate.clone()], 10);
        assert_eq!(res_start.len(), 1);

        // Inside window (50) -> Valid
        let res_mid = apply_temporal_validity_filter_at(vec![candidate.clone()], 50);
        assert_eq!(res_mid.len(), 1);

        // One step before end (99) -> Valid
        let res_before_end = apply_temporal_validity_filter_at(vec![candidate.clone()], 99);
        assert_eq!(res_before_end.len(), 1);

        // Exact end (100) -> Invalid (exclusive upper bound)
        let res_end = apply_temporal_validity_filter_at(vec![candidate.clone()], 100);
        assert!(res_end.is_empty());

        // After end (> 100) -> Invalid
        let res_after = apply_temporal_validity_filter_at(vec![candidate.clone()], 101);
        assert!(res_after.is_empty());
    }
}
