// FILE-CONTEXT
// ZWECK: Distanzberechnungen und Vektor-Dequantisierung via memfuse-simd.
// INVARIANTEN: Zero unsafe code in distance.rs; Delegierung an memfuse-simd.
// HOTSPOTS: compute_distance, compute_distance_trusted, compute_distance_f32_bytes_trusted

//! # Distance Computation Module
//!
//! This module re-exports distance metric computations and vector validation from `memfuse_simd`.

pub use memfuse_simd::{
    compute_distance, compute_distance_f32_bytes_trusted, compute_distance_trusted,
    cosine_distance, cosine_distance_f32_bytes, cosine_distance_scalar,
    cosine_similarity_parts_f32_u8, cosine_similarity_parts_u8, dot_product_distance,
    dot_product_distance_f32_bytes, dot_product_f32_u8, dot_product_scalar, dot_product_u8,
    euclidean_distance, euclidean_distance_f32_bytes, euclidean_distance_scalar,
    euclidean_distance_sq_f32_u8, euclidean_distance_sq_u8, normalize_inplace, validate_vector,
    CosineSimilarityPartsF32U8, CosineSimilarityPartsU8,
};

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::DistanceMetric;

    #[test]
    fn test_compute_distance_f32_bytes_equivalence() {
        let dims = [1, 7, 16, 32, 64, 128, 768];
        let metrics = [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ];

        for &dim in &dims {
            let query: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.17).sin()).collect();
            let target: Vec<f32> = (0..dim).map(|i| ((i + 3) as f32 * 0.23).cos()).collect();

            let mut target_bytes = Vec::with_capacity(dim * 4);
            for &val in &target {
                target_bytes.extend_from_slice(&val.to_le_bytes());
            }

            for &metric in &metrics {
                let dist_trusted = compute_distance_trusted(&query, &target, metric).unwrap();
                let dist_bytes =
                    compute_distance_f32_bytes_trusted(&query, &target_bytes, metric).unwrap();

                let diff = (dist_trusted - dist_bytes).abs();
                assert!(
                    diff < 1e-5,
                    "Unaligned byte kernel mismatch for {:?} at dim {dim}: trusted={dist_trusted}, bytes={dist_bytes}, diff={diff}",
                    metric
                );
            }
        }
    }

    #[test]
    fn test_public_compute_distance_api_sanity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0];

        let cos = compute_distance(&v1, &v2, DistanceMetric::Cosine).expect("cosine");
        assert!((cos - 1.0).abs() < 1e-5);

        let euc = compute_distance(&v1, &v2, DistanceMetric::Euclidean).expect("euclidean");
        assert!((euc - 2.0_f32.sqrt()).abs() < 1e-5);

        let dot = compute_distance(&v1, &v2, DistanceMetric::DotProduct).expect("dot");
        assert_eq!(dot, 0.0);
    }

    #[test]
    fn test_distances_match_scalar() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = vec![9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];

        let dot_scalar = memfuse_simd::kernels::scalar::dot_product_scalar(&a, &b);
        let d = compute_distance(&a, &b, DistanceMetric::DotProduct).expect("test");
        let dot_simd = d; // Note: compute_distance returns -dot for DotProduct
        assert!((dot_scalar - dot_simd).abs() < 1e-3);

        let euc_scalar = memfuse_simd::kernels::scalar::euclidean_distance_scalar(&a, &b);
        let euc_simd = compute_distance(&a, &b, DistanceMetric::Euclidean).expect("test");
        assert!((euc_scalar - euc_simd).abs() < 1e-3);

        let cos_scalar = memfuse_simd::kernels::scalar::cosine_distance_scalar(&a, &b);
        let cos_simd = compute_distance(&a, &b, DistanceMetric::Cosine).expect("test");
        assert!((cos_scalar - cos_simd).abs() < 1e-3);
    }

    #[test]
    fn test_u8_metrics_exact_match() {
        let dimensions = [1, 7, 16, 32, 33, 64, 128, 256];

        for &dim in &dimensions {
            let a: Vec<u8> = (0..dim).map(|i| ((i * 17 + 3) % 256) as u8).collect();
            let b: Vec<u8> = (0..dim).map(|i| ((i * 31 + 11) % 256) as u8).collect();

            let dot_scalar = memfuse_simd::kernels::scalar::dot_product_u8_scalar(&a, &b);
            let dot_dispatch = dot_product_u8(&a, &b).unwrap();
            assert_eq!(
                dot_scalar, dot_dispatch,
                "u8 DotProduct mismatch at dim {dim}: scalar={dot_scalar}, dispatch={dot_dispatch}"
            );

            let euc_sq_scalar =
                memfuse_simd::kernels::scalar::euclidean_distance_sq_u8_scalar(&a, &b);
            let euc_sq_dispatch = euclidean_distance_sq_u8(&a, &b).unwrap();
            assert_eq!(
                euc_sq_scalar, euc_sq_dispatch,
                "u8 Squared Euclidean mismatch at dim {dim}: scalar={euc_sq_scalar}, dispatch={euc_sq_dispatch}"
            );

            let parts_scalar =
                memfuse_simd::kernels::scalar::cosine_similarity_parts_u8_scalar(&a, &b);
            let parts_dispatch = cosine_similarity_parts_u8(&a, &b).unwrap();
            assert_eq!(
                parts_scalar.dot, parts_dispatch.dot,
                "u8 Cosine dot mismatch at dim {dim}"
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
    fn test_u8_distance_unequal_lengths_return_error() {
        let a = vec![1u8; 64];
        let b = vec![1u8; 32];
        assert!(dot_product_u8(&a, &b).is_err());
        assert!(euclidean_distance_sq_u8(&a, &b).is_err());
        assert!(cosine_similarity_parts_u8(&a, &b).is_err());
    }

    #[test]
    fn test_asymmetric_metrics() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![10, 20, 30, 40];
        let alpha = 0.1_f32;
        let min = 0.0_f32;
        let alphas = vec![alpha; 4];
        let mins = vec![min; 4];

        let dist_sq = euclidean_distance_sq_f32_u8(&a, &b, &alphas, &mins);
        let mut expected = 0.0;
        for i in 0..4 {
            let diff = a[i] - (b[i] as f32 * alpha + min);
            expected += diff * diff;
        }
        assert!((dist_sq - expected).abs() < 1e-5);

        let dot = dot_product_f32_u8(&a, &b);
        let mut expected_dot = 0.0;
        for i in 0..4 {
            expected_dot += a[i] * (b[i] as f32);
        }
        assert!((dot - expected_dot).abs() < 1e-5);
    }

    #[test]
    fn test_distance_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let res = compute_distance(&a, &b, DistanceMetric::Cosine);
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::EmbeddingDimensionMismatch {
                expected: 2,
                got: 3
            })
        ));
    }

    #[test]
    fn test_cosine_distance_mismatch_returns_error() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let res = cosine_distance(&a, &b);
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::EmbeddingDimensionMismatch {
                expected: 2,
                got: 3
            })
        ));
    }

    #[test]
    fn test_euclidean_distance_mismatch_returns_error() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let res = euclidean_distance(&a, &b);
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::EmbeddingDimensionMismatch {
                expected: 2,
                got: 3
            })
        ));
    }

    #[test]
    fn test_dot_product_distance_mismatch_returns_error() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let res = dot_product_distance(&a, &b);
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::EmbeddingDimensionMismatch {
                expected: 2,
                got: 3
            })
        ));
    }

    #[test]
    fn test_normalize_inplace() {
        let mut zero_vec = vec![0.0f32, 0.0, 0.0];
        normalize_inplace(&mut zero_vec);
        assert_eq!(zero_vec, vec![0.0, 0.0, 0.0]);

        let mut single_vec = vec![5.0f32];
        normalize_inplace(&mut single_vec);
        assert_eq!(single_vec, vec![1.0]);

        let mut vec_34 = vec![3.0f32, 4.0];
        normalize_inplace(&mut vec_34);
        assert!((vec_34[0] - 0.6).abs() < 1e-6);
        assert!((vec_34[1] - 0.8).abs() < 1e-6);

        let norm_sq: f32 = vec_34.iter().map(|x| x * x).sum();
        assert!((norm_sq - 1.0).abs() < 1e-6);
    }
}
