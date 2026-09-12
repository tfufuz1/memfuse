// FILE-CONTEXT
// STAND: 2026-09-09T12:37:35Z (SESSION: 20c1aaf4)
// ZWECK: Deep integration, proptest, and adversarial test suite for memfuse-calibration.
// INVARIANTEN: INV-CAL-1 (no silent fallback before warmup), INV-CAL-2 (invalidation resets observations/weights), P8 compliance.

use memfuse_calibration::{ConfigFingerprint, IsotonicCalibrator, PidController, PlattScaler};
use proptest::prelude::*;

// ============================================================================
// 1. ISOTONIC CALIBRATOR TESTS (PAVA & ECE)
// ============================================================================

#[test]
fn test_isotonic_warmup_invariant_inv_cal1() {
    let mut cal = IsotonicCalibrator::new(20, 100);
    for i in 0..19 {
        cal.record_outcome(i as f32 / 20.0, i % 2 == 0);
    }
    // INV-CAL-1: Must return None before warmup threshold is reached.
    assert!(cal.calibrated_probability(0.5).is_none());
    assert!(!cal.is_calibrated());

    // Record 20th observation to reach warmup
    cal.record_outcome(0.95, true);
    assert!(cal.is_calibrated());
    assert!(cal.calibrated_probability(0.5).is_some());
}

#[test]
fn test_isotonic_invalidation_invariant_inv_cal2() {
    let mut cal = IsotonicCalibrator::new(5, 100);
    for i in 0..10 {
        cal.record_outcome(i as f32 / 10.0, true);
    }
    assert_eq!(cal.observation_count(), 10);
    assert!(cal.is_calibrated());

    let fp1 = ConfigFingerprint::new("model-1", "Q4_0", "tmpl-1", 0.7);
    cal.invalidate_on_config_change(fp1.clone());

    // INV-CAL-2: Observations cleared, calibration status lost
    assert_eq!(cal.observation_count(), 0);
    assert!(!cal.is_calibrated());
    assert!(cal.calibrated_probability(0.5).is_none());

    // Passing the exact same fingerprint must NOT invalidate
    for i in 0..5 {
        cal.record_outcome(i as f32 / 5.0, true);
    }
    assert_eq!(cal.observation_count(), 5);
    cal.invalidate_on_config_change(fp1);
    assert_eq!(cal.observation_count(), 5);
}

#[test]
fn test_isotonic_pava_monotonicity_and_bounds() {
    let mut cal = IsotonicCalibrator::new(10, 100);
    let inputs = vec![
        (0.1, false),
        (0.9, true),
        (0.2, true),
        (0.8, false),
        (0.3, false),
        (0.7, true),
        (0.4, true),
        (0.6, false),
        (0.5, true),
        (0.55, true),
    ];
    for (score, outcome) in inputs {
        cal.record_outcome(score, outcome);
    }

    let test_scores = vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
    let mut prev_prob = -1.0f32;

    for &score in &test_scores {
        let prob = cal.calibrated_probability(score).unwrap();
        assert!(
            prob >= 0.0 && prob <= 1.0,
            "Probability out of [0,1]: {prob}"
        );
        assert!(
            prob >= prev_prob - 1e-6,
            "PAVA monotonicity violated at score {score}: {prob} < {prev_prob}"
        );
        prev_prob = prob;
    }
}

#[test]
fn test_isotonic_ece_calculation() {
    let mut cal = IsotonicCalibrator::new(20, 200);

    // Before warmup ECE is None
    assert!(cal.expected_calibration_error().is_none());

    for i in 0..100 {
        let score = i as f32 / 100.0;
        let outcome = score > 0.5;
        cal.record_outcome(score, outcome);
    }

    let ece = cal.expected_calibration_error();
    assert!(ece.is_some());
    let ece_val = ece.unwrap();
    assert!(ece_val >= 0.0, "ECE must be non-negative");
    assert!(ece_val < 0.20, "ECE for well-behaved signal should be low");
}

#[test]
fn test_isotonic_nan_and_extreme_inputs() {
    let mut cal = IsotonicCalibrator::new(5, 50);
    for _ in 0..5 {
        cal.record_outcome(0.5, true);
    }

    // Lookup with NaN, infinity, or negative values
    let prob_nan = cal.calibrated_probability(f32::NAN);
    assert!(prob_nan.is_some());
    let prob_inf = cal.calibrated_probability(f32::INFINITY);
    assert!(prob_inf.is_some());
    let prob_neg_inf = cal.calibrated_probability(f32::NEG_INFINITY);
    assert!(prob_neg_inf.is_some());
}

#[test]
fn test_isotonic_duplicate_raw_score_pooling() {
    let mut cal = IsotonicCalibrator::new(4, 50);
    // Duplicate raw scores with conflicting outcomes
    cal.record_outcome(0.5, true);
    cal.record_outcome(0.5, false);
    cal.record_outcome(0.5, true);
    cal.record_outcome(0.5, false);

    // Warmup reached (4 obs)
    assert!(cal.is_calibrated());
    let prob = cal.calibrated_probability(0.5).unwrap();
    // Pre-aggregation merges all four score 0.5 obs into 1 block with avg 0.5
    assert!((prob - 0.5).abs() < 1e-5, "Expected 0.5, got {prob}");
}

// ============================================================================
// 2. PLATT SCALER TESTS
// ============================================================================

#[test]
fn test_platt_scaler_defaults_and_identity() {
    let scaler = PlattScaler::default();
    assert!(scaler.is_identity());
    assert!(!scaler.is_fitted());
    assert_eq!(scaler.params(), (1.0, 0.0));

    // sigmoid(0.0) = 0.5
    assert!((scaler.predict(0.0) - 0.5).abs() < 1e-6);
    assert!((scaler.apply(0.0) - 0.5).abs() < 1e-6);
    assert!((scaler.transform(0.0) - 0.5).abs() < 1e-6);
}

#[test]
fn test_platt_scaler_nan_logit_fallback() {
    let scaler = PlattScaler::new(2.0, 1.0);
    assert_eq!(scaler.transform(f32::NAN), 0.5);
    assert_eq!(scaler.predict(f32::NAN), 0.5);
}

#[test]
fn test_platt_scaler_fit_behavior() {
    // Empty observations -> returns identity
    let empty_scaler = PlattScaler::fit(&[]);
    assert!(empty_scaler.is_identity());

    // Single class positive
    let pos_obs = vec![(0.5, true), (0.8, true), (0.9, true)];
    let pos_scaler = PlattScaler::fit(&pos_obs);
    assert!(pos_scaler.predict(0.8) > 0.5);

    // Single class negative
    let neg_obs = vec![(-0.5, false), (-0.8, false), (-0.9, false)];
    let neg_scaler = PlattScaler::fit(&neg_obs);
    assert!(neg_scaler.predict(-0.8) < 0.5);
}

#[test]
fn test_platt_scaler_invalidation() {
    let mut scaler = PlattScaler::new(3.0, -1.0);
    assert!(!scaler.is_identity());

    let fp = ConfigFingerprint::new("model-a", "Q8_0", "tmpl-a", 0.1);
    scaler.invalidate_on_config_change(fp.clone());

    // Resets to identity (A=1.0, B=0.0)
    assert!(scaler.is_identity());
    assert_eq!(scaler.params(), (1.0, 0.0));
}

// ============================================================================
// 3. PID CONTROLLER TESTS
// ============================================================================

#[test]
fn test_pid_controller_basic_regulation() {
    let mut pid = PidController::default();

    // At target latency (150ms) -> pool size unchanged
    let pool1 = pid.update(100, 150.0);
    assert_eq!(pool1, 100);

    // Latency too high (300ms > 150ms) -> pool size decreases
    let pool2 = pid.update(100, 300.0);
    assert!(pool2 < 100);

    // Latency too low (50ms < 150ms) -> pool size increases
    let pool3 = pid.update(100, 50.0);
    assert!(pool3 > 100);
}

#[test]
fn test_pid_controller_anti_windup_and_reset() {
    let mut pid = PidController::default();

    // Saturate integral via large error
    for _ in 0..500 {
        pid.update(100, 1000.0);
    }
    assert!(pid.current_pool_size.is_some());

    pid.reset();
    assert_eq!(pid.current_pool_size, None);

    // After reset, at target latency pool size is unchanged
    let pool_after_reset = pid.update(100, 150.0);
    assert_eq!(pool_after_reset, 100);
}

#[test]
fn test_pid_controller_min_max_clamping() {
    let mut pid = PidController::default();
    pid.min_pool_size = 50;
    pid.max_pool_size = 150;

    let min_clamped = pid.update(5, 10000.0);
    assert_eq!(min_clamped, 50);

    let max_clamped = pid.update(200, 0.0);
    assert_eq!(max_clamped, 150);
}

#[test]
fn test_pid_controller_non_finite_latency_safety() {
    let mut pid = PidController::default();
    let initial_pool = 100;
    let pool_before = pid.update(initial_pool, 150.0);

    // Pass NaN latency
    let pool_nan = pid.update(pool_before, f32::NAN);
    assert_eq!(pool_nan, pool_before);

    // Pass Infinity latency
    let pool_inf = pid.update(pool_before, f32::INFINITY);
    assert_eq!(pool_inf, pool_before);

    // Subsequent normal measurement should function normally without state corruption
    let pool_normal = pid.update(pool_before, 300.0);
    assert!(pool_normal < pool_before);
}

// ============================================================================
// 4. PROPTEST PROPERTY TESTS
// ============================================================================

proptest! {
    #[test]
    fn prop_platt_scaler_bounded_output(logit in -100.0f32..100.0f32, a in -5.0f32..5.0f32, b in -5.0f32..5.0f32) {
        let scaler = PlattScaler::new(a, b);
        let prob = scaler.transform(logit);
        prop_assert!(prob >= 0.0 && prob <= 1.0, "Probability out of bounds: {}", prob);
    }

    #[test]
    fn prop_pid_output_within_bounds(
        pool_size in 50usize..1000,
        measured_lat in 0.0f32..2000.0f32,
        steps in 1usize..20
    ) {
        let mut pid = PidController::default();
        pid.min_pool_size = 50;
        pid.max_pool_size = 500;

        let mut current_pool = pool_size;
        for _ in 0..steps {
            current_pool = pid.update(current_pool, measured_lat);
            prop_assert!(current_pool >= 50, "Pool size {} below min 50", current_pool);
            prop_assert!(current_pool <= 500, "Pool size {} above max 500", current_pool);
        }
    }
}
