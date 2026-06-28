use memfuse_core::DistanceMetric;
use memfuse_index::distance::{
    compute_distance, cosine_distance_scalar, dot_product_scalar, euclidean_distance_scalar,
};
use proptest::prelude::*;

proptest! {
    /// Test that SIMD and Scalar implementations yield the same result for Dot Product
    /// We use a relative epsilon because floating point errors accumulate with vector length.
    #[test]
    fn test_dot_product_determinism(
        v1 in prop::collection::vec(-10.0..10.0f32, 1..1024),
        v2 in prop::collection::vec(-10.0..10.0f32, 1..1024)
    ) {
        let len = v1.len().min(v2.len());
        let a = &v1[..len];
        let b = &v2[..len];

        let scalar = dot_product_scalar(a, b);
        let simd = -compute_distance(a, b, DistanceMetric::DotProduct).unwrap();

        // Accumulation error can be up to EPSILON * len in worst case (though usually much less)
        // Using a more robust relative check for large sums.
        let diff = (scalar - simd).abs();

        // The error bound depends on the sum of absolute values of the terms,
        // not the absolute value of the final sum, to handle catastrophic cancellation.
        let abs_sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x * y).abs()).sum();
        let tolerance = 1e-5 * abs_sum.max(1.0);

        prop_assert!(diff <= tolerance,
            "DotProduct mismatch at len {}: scalar={}, simd={} (diff={}, tolerance={}, abs_sum={})",
            len, scalar, simd, diff, tolerance, abs_sum);
    }

    /// Test that SIMD and Scalar implementations yield the same result for Euclidean Distance
    #[test]
    fn test_euclidean_determinism(
        v1 in prop::collection::vec(-10.0..10.0f32, 1..1024),
        v2 in prop::collection::vec(-10.0..10.0f32, 1..1024)
    ) {
        let len = v1.len().min(v2.len());
        let a = &v1[..len];
        let b = &v2[..len];

        let scalar = euclidean_distance_scalar(a, b);
        let simd = compute_distance(a, b, DistanceMetric::Euclidean).unwrap();

        let diff = (scalar - simd).abs();
        let tolerance = 1e-6 * (len as f32) * scalar.max(1.0);

        prop_assert!(diff < tolerance,
            "Euclidean mismatch at len {}: scalar={}, simd={} (diff={}, tolerance={})",
            len, scalar, simd, diff, tolerance);
    }

    /// Test that SIMD and Scalar implementations yield the same result for Cosine Distance
    #[test]
    fn test_cosine_determinism(
        v1 in prop::collection::vec(-10.0..10.0f32, 1..1024),
        v2 in prop::collection::vec(-10.0..10.0f32, 1..1024)
    ) {
        let len = v1.len().min(v2.len());
        let a = &v1[..len];
        let b = &v2[..len];

        let scalar = cosine_distance_scalar(a, b);
        let simd = compute_distance(a, b, DistanceMetric::Cosine).unwrap();

        let diff = (scalar - simd).abs();
        // Cosine distance is bounded [0, 2], so 1e-4 is generally safe even with accumulation.
        prop_assert!(diff < 1e-4,
            "Cosine mismatch at len {}: scalar={}, simd={} (diff={})",
            len, scalar, simd, diff);
    }
}
