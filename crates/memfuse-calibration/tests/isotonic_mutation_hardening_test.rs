// FILE-CONTEXT
// STAND: 2026-09-11T00:00:00Z (SESSION: JULES-20260911-MUTATION-HARDENING)
// ZWECK: Mutation hardening test suite for IsotonicCalibrator (PAVA/ECE).
// INVARIANTEN: INV-CAL-1 (no fallback before warmup), INV-CAL-2 (fingerprint reset), P8 compliance.

use memfuse_calibration::IsotonicCalibrator;
use proptest::prelude::*;

// ============================================================================
// 1. VERIFICATION & DUPLICATE RAW SCORE DETERMINISTIC POOLING
// ============================================================================

/// VERIFICATION OF AGT-CALIBRATION-16f90c35:
/// In `isotonic.rs`, `rebuild_model()` pre-aggregates all observations with
/// identical `raw_score` inputs before running the PAVA block-merging loop.
/// This guarantees that conflicting outcomes recorded for identical raw scores
/// are combined into a single `(score, sum_label, count)` tuple regardless of
/// the insertion order or outcome ordering.
#[test]
fn test_pava_duplicate_raw_score_deterministic_pooling() {
    // Construct 5 observation pairs with identical raw scores and mixed outcomes.
    // Scores: 0.1, 0.3, 0.5, 0.7, 0.9.
    // Hand calculation of pre-aggregated blocks:
    // - Score 0.1: 2 obs (false, false) -> sum 0.0, count 2 -> avg 0.0
    // - Score 0.3: 2 obs (false, true)  -> sum 1.0, count 2 -> avg 0.5
    // - Score 0.5: 2 obs (true, true)   -> sum 2.0, count 2 -> avg 1.0
    // - Score 0.7: 2 obs (false, true)  -> sum 1.0, count 2 -> avg 0.5
    // - Score 0.9: 2 obs (true, true)   -> sum 2.0, count 2 -> avg 1.0
    //
    // PAVA block-merging trace:
    // 1. Block(0.1, sum=0.0, count=2) -> avg 0.0
    // 2. Block(0.3, sum=1.0, count=2) -> avg 0.5 (0.5 > 0.0 -> push)
    // 3. Block(0.5, sum=2.0, count=2) -> avg 1.0 (1.0 > 0.5 -> push)
    // 4. Block(0.7, sum=1.0, count=2) -> avg 0.5 (0.5 <= 1.0 -> MERGE with 0.5!)
    //    Merged Block(0.7, sum=3.0, count=4) -> avg 3/4 = 0.75 (0.75 > 0.5 -> stop)
    // 5. Block(0.9, sum=2.0, count=2) -> avg 1.0 (1.0 > 0.75 -> push)
    //
    // Hand-calculated expected step function probabilities:
    // - score <= 0.1: 0.0
    // - score in (0.1, 0.3]: 0.5
    // - score in (0.3, 0.7]: 0.75
    // - score in (0.7, 0.9]: 1.0

    let pairs = vec![
        (0.1, false, false),
        (0.3, false, true),
        (0.5, true, true),
        (0.7, false, true),
        (0.9, true, true),
    ];

    // Order A: Insert false first, then true for mixed pairs
    let mut cal_a = IsotonicCalibrator::new(10, 100);
    for &(score, o1, o2) in &pairs {
        cal_a.record_outcome(score, o1);
        cal_a.record_outcome(score, o2);
    }

    // Order B: Insert true first, then false for mixed pairs (reversed order)
    let mut cal_b = IsotonicCalibrator::new(10, 100);
    for &(score, o1, o2) in &pairs {
        cal_b.record_outcome(score, o2);
        cal_b.record_outcome(score, o1);
    }

    let test_queries = [0.1f32, 0.3, 0.5, 0.7, 0.9];
    let expected_probs = [0.0f32, 0.5, 0.75, 0.75, 1.0];

    for (idx, &query) in test_queries.iter().enumerate() {
        let prob_a = cal_a.calibrated_probability(query).unwrap();
        let prob_b = cal_b.calibrated_probability(query).unwrap();
        let expected = expected_probs[idx];

        assert_eq!(
            prob_a, prob_b,
            "Order A ({prob_a}) and Order B ({prob_b}) must be identical for query {query}"
        );
        assert!(
            (prob_a - expected).abs() < 1e-5,
            "Expected {expected} for query {query}, got Order A={prob_a}, Order B={prob_b}"
        );
    }
}

// ============================================================================
// 2. ECE BIN BOUNDARY & EXACT EDGE VALUES
// ============================================================================

/// Verifies `expected_calibration_error()` on exact bin boundary scores
/// (`0.1, 0.2, ..., 0.9`) and validates exact bin placement.
#[test]
fn test_ece_bin_boundary_exact_edge_values() {
    let mut cal = IsotonicCalibrator::new(10, 100);

    // Record 10 observations with raw_scores exactly on 10 bin boundaries:
    // 0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9
    // All outcomes = true.
    for i in 0..10 {
        let score = i as f32 / 10.0;
        cal.record_outcome(score, true);
    }

    assert!(cal.is_calibrated());

    // PAVA step function for all outcomes = true:
    // Single block with threshold 0.9 and calibrated prob = 1.0.
    let prob_0_5 = cal.calibrated_probability(0.5).unwrap();
    assert_eq!(prob_0_5, 1.0);

    // Hand-calculated ECE:
    // All 10 calibrated probabilities are 1.0.
    // In M=10 equal bins ([0.0, 0.1), [0.1, 0.2), ..., [0.9, 1.0]):
    // Prob 1.0 falls into Bin 9 ([0.9, 1.0]) because of the upper inclusive bound `prob <= hi`.
    // Bin 9 has bin_n = 10, avg_confidence = 1.0, avg_accuracy = 1.0.
    // ECE = (10/10) * |1.0 - 1.0| = 0.0.
    let ece = cal.expected_calibration_error().unwrap();
    assert!(
        (ece - 0.0).abs() < 1e-5,
        "Expected ECE 0.0 for perfectly accurate predictions, got {ece}"
    );

    // Test exact bin placement for prob = 0.5:
    // Construct a calibrator where calibrated_probability is exactly 0.5.
    let mut cal_half = IsotonicCalibrator::new(10, 100);
    for _ in 0..5 {
        cal_half.record_outcome(0.5, true);
        cal_half.record_outcome(0.5, false);
    }

    let prob_half = cal_half.calibrated_probability(0.5).unwrap();
    assert_eq!(prob_half, 0.5);

    // In ECE with M=10 bins [0.0, 0.1), [0.1, 0.2), ..., [0.4, 0.5), [0.5, 0.6), ...
    // For Bin 4 ([0.4, 0.5)): prob >= 0.4 && prob < 0.5 -> 0.5 < 0.5 is FALSE.
    // For Bin 5 ([0.5, 0.6)): prob >= 0.5 && prob < 0.6 -> 0.5 >= 0.5 && 0.5 < 0.6 is TRUE.
    // Thus prob = 0.5 lands in Bin 5 ([0.5, 0.6)), NOT Bin 4 ([0.4, 0.5)).
    // In Bin 5: 10 observations, avg_confidence = 0.5, avg_accuracy = 5/10 = 0.5.
    // ECE = 0.0.
    let ece_half = cal_half.expected_calibration_error().unwrap();
    assert!(
        (ece_half - 0.0).abs() < 1e-5,
        "Expected ECE 0.0 for half-probability calibration, got {ece_half}"
    );
}

// ============================================================================
// 3. SINGLE OBSERVATION EDGE CASE
// ============================================================================

/// Verifies behavior when exactly ONE observation is recorded.
#[test]
fn test_pava_single_observation_edge_case() {
    // Case A: Default warmup_required = 50. Single observation must return None (INV-CAL-1).
    let mut cal_default = IsotonicCalibrator::new(50, 100);
    cal_default.record_outcome(0.5, true);
    assert!(!cal_default.is_calibrated());
    assert!(cal_default.calibrated_probability(0.5).is_none());

    // Case B: Configured warmup_required = 1. Single observation reaches warmup.
    let mut cal_warmup1 = IsotonicCalibrator::new(1, 100);
    cal_warmup1.record_outcome(0.7, true);
    assert!(cal_warmup1.is_calibrated());

    let prob = cal_warmup1.calibrated_probability(0.7);
    assert!(prob.is_some());
    let val = prob.unwrap();
    assert!(!val.is_nan(), "Calibrated probability must not be NaN");
    assert_eq!(val, 1.0, "Single true outcome must yield probability 1.0");

    // Case C: Single false outcome
    let mut cal_warmup1_false = IsotonicCalibrator::new(1, 100);
    cal_warmup1_false.record_outcome(0.3, false);
    let val_false = cal_warmup1_false.calibrated_probability(0.3).unwrap();
    assert!(!val_false.is_nan());
    assert_eq!(
        val_false, 0.0,
        "Single false outcome must yield probability 0.0"
    );
}

// ============================================================================
// 4. EMPTY CALIBRATOR SAFETY
// ============================================================================

/// Verifies that an empty calibrator does not panic on `binary_search_by`.
#[test]
fn test_pava_empty_calibrator_returns_none_not_panic() {
    // Case A: Standard warmup_required = 10.
    let mut cal = IsotonicCalibrator::new(10, 100);
    assert_eq!(cal.observation_count(), 0);
    assert!(!cal.is_calibrated());
    assert!(cal.calibrated_probability(0.5).is_none());
    assert!(cal.expected_calibration_error().is_none());

    // Case B: Warmup required = 0 (edge configuration).
    let mut cal_zero = IsotonicCalibrator::new(0, 100);
    assert!(cal_zero.is_calibrated());
    // Must return Some(0.5) default without panic when binary searching empty cached model.
    let prob = cal_zero.calibrated_probability(0.5);
    assert_eq!(prob, Some(0.5));

    let ece = cal_zero.expected_calibration_error();
    assert_eq!(ece, Some(0.0));
}

// ============================================================================
// 5. PROPTEST: PAVA MONOTONICITY FOR ARBITRARY UNORDERED INPUT
// ============================================================================

proptest! {
    /// For any arbitrary sequence of (raw_score, outcome) pairs,
    /// the fitted PAVA step function MUST be monotonically non-decreasing
    /// across any sequence of query points.
    #[test]
    fn prop_pava_monotonicity_holds_for_arbitrary_unordered_input(
        inputs in prop::collection::vec(
            (0.0f32..1.0f32, prop::bool::ANY),
            10..100
        )
    ) {
        let mut cal = IsotonicCalibrator::new(10, 200);
        for &(score, outcome) in &inputs {
            cal.record_outcome(score, outcome);
        }

        prop_assert!(cal.is_calibrated());

        let mut query_scores = vec![0.0f32, 0.05, 0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 0.95, 1.0];
        query_scores.sort_by(|a, b| a.total_cmp(b));

        let mut prev_prob = -1.0f32;
        for &q in &query_scores {
            let prob = cal.calibrated_probability(q).unwrap();
            prop_assert!(prob >= 0.0 && prob <= 1.0, "Probability {} out of [0,1]", prob);
            prop_assert!(
                prob >= prev_prob - 1e-6,
                "PAVA monotonicity violated at query {}: prob {} < prev_prob {}",
                q, prob, prev_prob
            );
            prev_prob = prob;
        }
    }
}
