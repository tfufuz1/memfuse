// FILE-CONTEXT
// ZWECK: Audit-Testsuite für numerische Korrektheit (SIMD vs Skalar vs f64-Referenz)
// STAND: TS:2026-08-31T00:00:00Z

use memfuse_core::DistanceMetric;
use memfuse_index::distance::*;

/// Independent f64 reference implementation for Cosine Distance
fn cosine_distance_f64_ref(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let x64 = x as f64;
        let y64 = y as f64;
        dot += x64 * y64;
        norm_a += x64 * x64;
        norm_b += y64 * y64;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        1.0
    } else {
        1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))
    }
}

/// Independent f64 reference implementation for Euclidean Distance
fn euclidean_distance_f64_ref(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = (x as f64) - (y as f64);
            diff * diff
        })
        .sum::<f64>()
        .sqrt()
}

/// Independent f64 reference implementation for Dot Product Distance
fn dot_product_f64_ref(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as f64) * (y as f64))
        .sum::<f64>()
}

#[test]
fn test_simd_vs_scalar_vs_f64_all_metrics() {
    let dimensions = [1, 2, 7, 8, 13, 16, 31, 32, 64, 128, 129, 384, 768, 1536, 4096];
    let mut max_cos_diff = 0.0f64;
    let mut max_euc_diff = 0.0f64;
    let mut max_dot_diff = 0.0f64;

    for &dim in &dimensions {
        // Generate pseudo-random deterministic test vectors
        let a: Vec<f32> = (0..dim).map(|i| ((i as f32) * 0.17).sin()).collect();
        let b: Vec<f32> = (0..dim).map(|i| (((i + 5) as f32) * 0.23).cos()).collect();

        // 1. Cosine Distance
        let cos_f64 = cosine_distance_f64_ref(&a, &b);
        let cos_scalar = cosine_distance_scalar(&a, &b) as f64;
        let cos_simd = compute_distance(&a, &b, DistanceMetric::Cosine).unwrap() as f64;

        let cos_diff_scalar = (cos_scalar - cos_f64).abs();
        let cos_diff_simd = (cos_simd - cos_f64).abs();
        max_cos_diff = max_cos_diff.max(cos_diff_simd).max(cos_diff_scalar);

        assert!(
            (cos_simd - cos_scalar).abs() <= 1e-4,
            "Cosine SIMD vs Scalar divergence > 1e-4 at dim {dim}: simd={cos_simd}, scalar={cos_scalar}"
        );
        assert!(
            cos_diff_simd <= 1e-4,
            "Cosine SIMD vs f64 divergence > 1e-4 at dim {dim}: simd={cos_simd}, f64={cos_f64}"
        );

        // 2. Euclidean Distance
        let euc_f64 = euclidean_distance_f64_ref(&a, &b);
        let euc_scalar = euclidean_distance_scalar(&a, &b) as f64;
        let euc_simd = compute_distance(&a, &b, DistanceMetric::Euclidean).unwrap() as f64;

        let euc_diff_scalar = (euc_scalar - euc_f64).abs();
        let euc_diff_simd = (euc_simd - euc_f64).abs();
        max_euc_diff = max_euc_diff.max(euc_diff_simd).max(euc_diff_scalar);

        assert!(
            (euc_simd - euc_scalar).abs() <= 1e-4,
            "Euclidean SIMD vs Scalar divergence > 1e-4 at dim {dim}: simd={euc_simd}, scalar={euc_scalar}"
        );
        assert!(
            euc_diff_simd <= 1e-4,
            "Euclidean SIMD vs f64 divergence > 1e-4 at dim {dim}: simd={euc_simd}, f64={euc_f64}"
        );

        // 3. Dot Product Distance (compute_distance returns -dot)
        let dot_f64 = dot_product_f64_ref(&a, &b);
        let dot_scalar = dot_product_scalar(&a, &b) as f64;
        let dot_simd = -compute_distance(&a, &b, DistanceMetric::DotProduct).unwrap() as f64;

        let dot_diff_scalar = (dot_scalar - dot_f64).abs();
        let dot_diff_simd = (dot_simd - dot_f64).abs();
        max_dot_diff = max_dot_diff.max(dot_diff_simd).max(dot_diff_scalar);

        assert!(
            (dot_simd - dot_scalar).abs() <= 1e-4,
            "DotProduct SIMD vs Scalar divergence > 1e-4 at dim {dim}: simd={dot_simd}, scalar={dot_scalar}"
        );
        assert!(
            dot_diff_simd <= 1e-4,
            "DotProduct SIMD vs f64 divergence > 1e-4 at dim {dim}: simd={dot_simd}, f64={dot_f64}"
        );
    }

    println!("Max Cosine Deviation vs f64: {max_cos_diff:.8e}");
    println!("Max Euclidean Deviation vs f64: {max_euc_diff:.8e}");
    println!("Max DotProduct Deviation vs f64: {max_dot_diff:.8e}");
}

#[test]
fn test_u8_metrics_exact_match() {
    let dimensions = [1, 7, 16, 32, 33, 64, 128, 256];

    for &dim in &dimensions {
        let a: Vec<u8> = (0..dim).map(|i| ((i * 17 + 3) % 256) as u8).collect();
        let b: Vec<u8> = (0..dim).map(|i| ((i * 31 + 11) % 256) as u8).collect();

        // 1. Dot Product u8
        let dot_scalar = dot_product_u8_scalar(&a, &b);
        let dot_dispatch = dot_product_u8(&a, &b);
        assert_eq!(
            dot_scalar, dot_dispatch,
            "u8 DotProduct mismatch at dim {dim}: scalar={dot_scalar}, dispatch={dot_dispatch}"
        );

        // 2. Squared Euclidean u8
        let euc_sq_scalar = euclidean_distance_sq_u8_scalar(&a, &b);
        let euc_sq_dispatch = euclidean_distance_sq_u8(&a, &b);
        assert_eq!(
            euc_sq_scalar, euc_sq_dispatch,
            "u8 Squared Euclidean mismatch at dim {dim}: scalar={euc_sq_scalar}, dispatch={euc_sq_dispatch}"
        );

        // 3. Cosine Parts u8
        let parts_scalar = cosine_similarity_parts_u8_scalar(&a, &b);
        let parts_dispatch = cosine_similarity_parts_u8(&a, &b);
        assert_eq!(
            parts_scalar.dot, parts_dispatch.dot,
            "u8 Cosine dot mismatch at dim {dim}"
        );
        assert_eq!(
            parts_scalar.sum_a, parts_dispatch.sum_a,
            "u8 Cosine sum_a mismatch at dim {dim}"
        );
        assert_eq!(
            parts_scalar.sum_b, parts_dispatch.sum_b,
            "u8 Cosine sum_b mismatch at dim {dim}"
        );
        assert_eq!(
            parts_scalar.norm_a_sq, parts_dispatch.norm_a_sq,
            "u8 Cosine norm_a_sq mismatch at dim {dim}"
        );
        assert_eq!(
            parts_scalar.norm_b_sq, parts_dispatch.norm_b_sq,
            "u8 Cosine norm_b_sq mismatch at dim {dim}"
        );
    }
}

#[test]
fn test_extreme_and_special_values() {
    // 1. Zero vectors
    let zero_a = vec![0.0f32; 128];
    let zero_b = vec![0.0f32; 128];

    let cos_zero = compute_distance(&zero_a, &zero_b, DistanceMetric::Cosine).unwrap();
    assert_eq!(
        cos_zero, 1.0,
        "Zero vector cosine distance must be 1.0 (uncorrelated)"
    );

    let euc_zero = compute_distance(&zero_a, &zero_b, DistanceMetric::Euclidean).unwrap();
    assert_eq!(euc_zero, 0.0, "Zero vector euclidean distance must be 0.0");

    let dot_zero = compute_distance(&zero_a, &zero_b, DistanceMetric::DotProduct).unwrap();
    assert_eq!(
        dot_zero, 0.0,
        "Zero vector dot product distance must be 0.0"
    );

    // 2. Subnormals
    let subnormal_a = vec![f32::MIN_POSITIVE * 0.5f32; 128];
    let subnormal_b = vec![f32::MIN_POSITIVE * 0.5f32; 128];
    let cos_sub = compute_distance(&subnormal_a, &subnormal_b, DistanceMetric::Cosine).unwrap();
    assert!(
        !cos_sub.is_nan(),
        "Subnormal vector distance must not return NaN"
    );

    // 3. Large values
    let large_a = vec![1e10f32; 64];
    let large_b = vec![1e10f32; 64];
    let cos_large = compute_distance(&large_a, &large_b, DistanceMetric::Cosine).unwrap();
    assert!(
        (cos_large - 0.0).abs() < 1e-4,
        "Identical large vectors must have ~0.0 cosine distance, got {cos_large}"
    );

    // 4. NaN / Infinity input rejection
    let nan_vec = vec![1.0f32, f32::NAN, 3.0];
    let normal_vec = vec![1.0f32, 2.0, 3.0];

    assert!(compute_distance(&nan_vec, &normal_vec, DistanceMetric::Cosine).is_err());
    assert!(compute_distance(&normal_vec, &nan_vec, DistanceMetric::Euclidean).is_err());

    let inf_vec = vec![1.0f32, f32::INFINITY, 3.0];
    assert!(compute_distance(&inf_vec, &normal_vec, DistanceMetric::Cosine).is_ok());
    // Note: compute_distance validates NaN explicitly
}

proptest::proptest! {
    #[test]
    fn prop_simd_vs_f64_parity_random_vectors(
        v1 in proptest::collection::vec(-100.0..100.0f32, 1..512),
        v2 in proptest::collection::vec(-100.0..100.0f32, 1..512)
    ) {
        let len = v1.len().min(v2.len());
        let a = &v1[..len];
        let b = &v2[..len];

        // 1. Cosine
        let cos_f64 = cosine_distance_f64_ref(a, b) as f32;
        let cos_simd = compute_distance(a, b, DistanceMetric::Cosine).unwrap();
        proptest::prop_assert!(
            (cos_simd - cos_f64).abs() < 1e-4,
            "Cosine proptest mismatch: simd={}, f64={}", cos_simd, cos_f64
        );

        // 2. Euclidean
        let euc_f64 = euclidean_distance_f64_ref(a, b) as f32;
        let euc_simd = compute_distance(a, b, DistanceMetric::Euclidean).unwrap();
        let rel_err = if euc_f64 > 1e-4 {
            (euc_simd - euc_f64).abs() / euc_f64
        } else {
            (euc_simd - euc_f64).abs()
        };
        proptest::prop_assert!(
            rel_err < 1e-4,
            "Euclidean proptest mismatch: simd={}, f64={}, rel_err={}", euc_simd, euc_f64, rel_err
        );
    }
}
