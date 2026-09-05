//! Distance computation functions.
//!
//! This module provides distance metrics for vector comparison.
//! Implementations use AVX2 SIMD if available, falling back to scalar.

use chimera_core::DistanceMetric;
use std::simd::prelude::*;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Computes distance between two vectors using the specified metric.
#[inline]
pub fn compute_distance(a: &[f32], b: &[f32], metric: DistanceMetric) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vector dimensions must match");

    match metric {
        DistanceMetric::Cosine => cosine_distance(a, b),
        DistanceMetric::Euclidean => euclidean_distance(a, b),
        DistanceMetric::DotProduct => dot_product_distance(a, b),
    }
}

/// Computes cosine distance (1 - similarity).
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector lengths must be equal");
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let a_ptr = a.as_ptr() as usize;
        let b_ptr = b.as_ptr() as usize;

        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f")
            && a_ptr.is_multiple_of(64)
            && b_ptr.is_multiple_of(64)
        {
            // SAFETY:
            // 1. Hardware Support: AVX-512F feature detected via is_x86_feature_detected! at runtime.
            // 2. Slice Invariants: Pre-check `a.len() == b.len()` ensures both vectors have identical length.
            // 3. Alignment: Base pointers `a_ptr` and `b_ptr` are verified to be 64-byte aligned (required for AVX-512).
            // 4. Memory Soundness: `cosine_distance_avx512` correctly processes multiples of 16 floats,
            //    using unaligned loads internally if necessary, but here alignment is guaranteed.
            return unsafe { cosine_distance_avx512(a, b) };
        }
        // Then AVX2 if AVX-512 is not available or not aligned
        if is_x86_feature_detected!("avx2") && a_ptr.is_multiple_of(32) && b_ptr.is_multiple_of(32)
        {
            // SAFETY:
            // 1. Hardware Support: AVX2 feature detected via is_x86_feature_detected!.
            // 2. Slice Invariants: synchronous length check (a.len() == b.len()) ensures iteration safety.
            // 3. Alignment: Base pointers are 32-byte aligned for AVX2 vector instructions.
            // 4. Memory Soundness: Processes 8 floats (32 bytes) per iteration with a scalar fallback for rem.
            return unsafe { cosine_distance_avx2(a, b) };
        }
    }
    // Portable-simd as default high-performance fallback
    cosine_distance_std_simd(a, b)
}

/// Computes Euclidean (L2) distance.
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector lengths must be equal");
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let a_ptr = a.as_ptr() as usize;
        let b_ptr = b.as_ptr() as usize;

        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f")
            && a_ptr.is_multiple_of(64)
            && b_ptr.is_multiple_of(64)
        {
            // SAFETY:
            // 1. CPU Support: AVX-512F feature detected via is_x86_feature_detected!.
            // 2. Alignment: Both base pointers are 64-byte aligned as checked by if condition.
            // 3. Bounds: Function internal loop uses i + 16 <= n, where n = a.len() == b.len().
            //    Accesses to a.as_ptr().add(i) read 16 * sizeof(f32) = 64 bytes, staying within [ptr, ptr + n).
            return unsafe { euclidean_distance_avx512(a, b) };
        }
        // Then AVX2 if AVX-512 is not available or not aligned
        if is_x86_feature_detected!("avx2") && a_ptr.is_multiple_of(32) && b_ptr.is_multiple_of(32)
        {
            // SAFETY:
            // 1. CPU Support: AVX2 feature detected via is_x86_feature_detected!.
            // 2. Alignment: Both base pointers are 32-byte aligned as checked by if condition.
            // 3. Bounds: Function internal loop uses i + 8 <= n, where n = a.len() == b.len().
            //    Accesses to a.as_ptr().add(i) read 8 * sizeof(f32) = 32 bytes, staying within [ptr, ptr + n).
            return unsafe { euclidean_distance_avx2(a, b) };
        }
    }
    // Portable-simd as default high-performance fallback
    euclidean_distance_std_simd(a, b)
}

/// Computes negative dot product.
#[inline]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector lengths must be equal");
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let a_ptr = a.as_ptr() as usize;
        let b_ptr = b.as_ptr() as usize;

        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f")
            && a_ptr.is_multiple_of(64)
            && b_ptr.is_multiple_of(64)
        {
            // SAFETY:
            // 1. CPU Support: AVX-512F feature detected via is_x86_feature_detected!.
            // 2. Alignment: Both base pointers are 64-byte aligned as checked by if condition.
            // 3. Bounds: Function internal loop uses i + 16 <= n, where n = a.len() == b.len().
            //    Accesses to a.as_ptr().add(i) read 16 * sizeof(f32) = 64 bytes, staying within [ptr, ptr + n).
            return unsafe { -dot_product_avx512(a, b) };
        }
        // Then AVX2 if AVX-512 is not available or not aligned
        if is_x86_feature_detected!("avx2") && a_ptr.is_multiple_of(32) && b_ptr.is_multiple_of(32)
        {
            // SAFETY:
            // 1. CPU Support: AVX2 feature detected via is_x86_feature_detected!.
            // 2. Alignment: Both base pointers are 32-byte aligned as checked by if condition.
            // 3. Bounds: Function internal loop uses i + 8 <= n, where n = a.len() == b.len().
            //    Accesses to a.as_ptr().add(i) read 8 * sizeof(f32) = 32 bytes, staying within [ptr, ptr + n).
            return unsafe { -dot_product_avx2(a, b) };
        }
    }
    // Portable-simd as default high-performance fallback
    -dot_product_std_simd(a, b)
}

/// Scalar implementation of cosine distance.
pub fn cosine_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        1.0
    } else {
        1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))
    }
}

/// Scalar implementation of Euclidean distance.
pub fn euclidean_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Scalar implementation of dot product.
pub fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// SIMD implementation of dot product using portable-simd.
pub fn dot_product_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let n = a.len();
    let mut sum = f32x8::splat(0.0);

    while i + 8 <= n {
        let va = f32x8::from_slice(&a[i..i + 8]);
        let vb = f32x8::from_slice(&b[i..i + 8]);
        sum += va * vb;
        i += 8;
    }

    let mut res = sum.reduce_sum();
    while i < n {
        res += a[i] * b[i];
        i += 1;
    }
    res
}

/// SIMD implementation of Euclidean distance using portable-simd.
pub fn euclidean_distance_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let n = a.len();
    let mut sum = f32x8::splat(0.0);

    while i + 8 <= n {
        let va = f32x8::from_slice(&a[i..i + 8]);
        let vb = f32x8::from_slice(&b[i..i + 8]);
        let diff = va - vb;
        sum += diff * diff;
        i += 8;
    }

    let mut res = sum.reduce_sum();
    while i < n {
        let diff = a[i] - b[i];
        res += diff * diff;
        i += 1;
    }
    res.sqrt()
}

/// SIMD implementation of cosine distance using portable-simd.
pub fn cosine_distance_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let n = a.len();
    let mut dot = f32x8::splat(0.0);
    let mut norm_a = f32x8::splat(0.0);
    let mut norm_b = f32x8::splat(0.0);

    while i + 8 <= n {
        let va = f32x8::from_slice(&a[i..i + 8]);
        let vb = f32x8::from_slice(&b[i..i + 8]);
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
        i += 8;
    }

    let mut final_dot = dot.reduce_sum();
    let mut final_norm_a = norm_a.reduce_sum();
    let mut final_norm_b = norm_b.reduce_sum();

    while i < n {
        let x = a[i];
        let y = b[i];
        final_dot += x * y;
        final_norm_a += x * x;
        final_norm_b += y * y;
        i += 1;
    }

    if final_norm_a == 0.0 || final_norm_b == 0.0 {
        1.0
    } else {
        1.0 - (final_dot / (final_norm_a.sqrt() * final_norm_b.sqrt()))
    }
}

// -----------------------------------------------------------------------------
// AVX2 Implementations
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm256_setzero_ps();
    let n = a.len();
    let mut i = 0;

    // Process 8 floats at a time
    while i + 8 <= n {
        // SAFETY:
        // Pointers: i + 8 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) are within [ptr, ptr + n).
        // Bounds: Reading 8 * 4 = 32 bytes is safe as n elements exist.
        // Alignment: Handled by _mm256_loadu_ps (unaligned), but base pointers are asserted as 32-byte aligned in caller.
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        sum_v = _mm256_fmadd_ps(va, vb, sum_v);
        i += 8;
    }

    // Horizontal sum
    // SAFETY: CPU supports AVX2/FMA as per target_feature.
    let mut sum = hsum256_ps_avx(sum_v);

    // Remainder
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut dot_v = _mm256_setzero_ps();
    let mut norm_a_v = _mm256_setzero_ps();
    let mut norm_b_v = _mm256_setzero_ps();

    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // SAFETY:
        // Pointers: i + 8 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) are within [ptr, ptr + n).
        // Bounds: Reading 32 bytes is safe for n elements.
        // Alignment: Handled by loadu, base pointers 32-byte aligned by caller assertion.
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));

        dot_v = _mm256_fmadd_ps(va, vb, dot_v);
        norm_a_v = _mm256_fmadd_ps(va, va, norm_a_v);
        norm_b_v = _mm256_fmadd_ps(vb, vb, norm_b_v);

        i += 8;
    }

    let mut dot = hsum256_ps_avx(dot_v);
    let mut norm_a = hsum256_ps_avx(norm_a_v);
    let mut norm_b = hsum256_ps_avx(norm_b_v);

    // Remainder
    while i < n {
        let x = a[i];
        let y = b[i];
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
        i += 1;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        1.0
    } else {
        1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm256_setzero_ps();
    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // SAFETY:
        // Pointers: i + 8 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) are within [ptr, ptr + n).
        // Bounds: Reading 32 bytes safe.
        // Alignment: Handled by loadu, base pointers 32-byte aligned by caller assertion.
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let diff = _mm256_sub_ps(va, vb);
        sum_v = _mm256_fmadd_ps(diff, diff, sum_v);
        i += 8;
    }

    // SAFETY: CPU supports AVX2/FMA as per target_feature.
    let mut sum = hsum256_ps_avx(sum_v);

    while i < n {
        let diff = a[i] - b[i];
        sum += diff * diff;
        i += 1;
    }

    sum.sqrt()
}

/// Horizontal sum of __m256 vectors
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    // SAFETY:
    // 1. Hardware: Target features avx2 and fma are enabled and checked at call-site.
    // 2. Input: v is a valid __m256 vector.
    // 3. Logic: Extract halves and sum down to a single float using _mm_cvtss_f32.
    let x128 = _mm_add_ps(_mm256_extractf128_ps(v, 1), _mm256_castps256_ps128(v));
    let x64 = _mm_add_ps(x128, _mm_movehl_ps(x128, x128));
    let x32 = _mm_add_ss(x64, _mm_shuffle_ps(x64, x64, 0x55));
    _mm_cvtss_f32(x32)
}

// -----------------------------------------------------------------------------
// AVX-512 Implementations
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm512_setzero_ps();
    let n = a.len();
    let mut i = 0;

    // Process 16 floats at a time
    while i + 16 <= n {
        // SAFETY:
        // Pointers: i + 16 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) within bounds.
        // Bounds: Reading 16 * 4 = 64 bytes is safe for n elements.
        // Alignment: Handled by loadu, base pointers 64-byte aligned by caller assertion.
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        sum_v = _mm512_fmadd_ps(va, vb, sum_v);
        i += 16;
    }

    // Horizontal sum
    // SAFETY: CPU supports AVX-512F as per target_feature.
    let mut sum = hsum512_ps_avx(sum_v);

    // Remainder
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn cosine_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut dot_v = _mm512_setzero_ps();
    let mut norm_a_v = _mm512_setzero_ps();
    let mut norm_b_v = _mm512_setzero_ps();

    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // SAFETY:
        // Pointers: i + 16 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) within bounds.
        // Bounds: Reading 64 bytes safe.
        // Alignment: Handled by loadu, base pointers 64-byte aligned by caller assertion.
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));

        dot_v = _mm512_fmadd_ps(va, vb, dot_v);
        norm_a_v = _mm512_fmadd_ps(va, va, norm_a_v);
        norm_b_v = _mm512_fmadd_ps(vb, vb, norm_b_v);

        i += 16;
    }

    // SAFETY: CPU supports AVX-512F as per target_feature.
    let mut dot = hsum512_ps_avx(dot_v);
    let mut norm_a = hsum512_ps_avx(norm_a_v);
    let mut norm_b = hsum512_ps_avx(norm_b_v);

    // Remainder
    while i < n {
        let x = a[i];
        let y = b[i];
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
        i += 1;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        1.0
    } else {
        1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm512_setzero_ps();
    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // SAFETY:
        // Pointers: i + 16 <= n ensures a.as_ptr().add(i) and b.as_ptr().add(i) within bounds.
        // Bounds: Reading 64 bytes safe.
        // Alignment: Handled by loadu, base pointers 64-byte aligned by caller assertion.
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        let diff = _mm512_sub_ps(va, vb);
        sum_v = _mm512_fmadd_ps(diff, diff, sum_v);
        i += 16;
    }

    // SAFETY: CPU supports AVX-512F as per target_feature.
    let mut sum = hsum512_ps_avx(sum_v);

    while i < n {
        let diff = a[i] - b[i];
        sum += diff * diff;
        i += 1;
    }

    sum.sqrt()
}

/// Horizontal sum of __m512 vectors
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn hsum512_ps_avx(v: __m512) -> f32 {
    // SAFETY:
    // 1. Hardware: avx512f feature is enabled and checked at call-site.
    // 2. Input: v is a valid __m512 vector.
    // 3. Logic: Extract high/low 256-bit halves, add them, then use AVX2 horizontal sum.
    // Extract high and low 256-bit halves
    let low = _mm512_castps512_ps256(v);
    let high = _mm512_extractf32x8_ps(v, 1);

    // Add them together
    let sum256 = _mm256_add_ps(low, high);

    // Use existing AVX2 horizontal sum
    // SAFETY: AVX512F implies AVX2.
    hsum256_ps_avx(sum256)
}

pub fn normalize_inplace(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distances_match_scalar() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = vec![9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];

        // Dot product
        let dot_scalar = dot_product_scalar(&a, &b);
        let d = compute_distance(&a, &b, DistanceMetric::DotProduct);
        let dot_simd = -d;
        assert!((dot_scalar - dot_simd).abs() < 1e-3);

        // Euclidean
        let euc_scalar = euclidean_distance_scalar(&a, &b);
        let euc_simd = compute_distance(&a, &b, DistanceMetric::Euclidean);
        assert!((euc_scalar - euc_simd).abs() < 1e-3);

        // Cosine
        let cos_scalar = cosine_distance_scalar(&a, &b);
        let cos_simd = compute_distance(&a, &b, DistanceMetric::Cosine);
        assert!((cos_scalar - cos_simd).abs() < 1e-3);
    }

    #[test]
    fn test_std_simd_dot_product() {
        let a = vec![1.0; 64];
        let b = vec![2.0; 64];
        let expected = 128.0;
        let actual = super::dot_product_std_simd(&a, &b);
        assert!((expected - actual).abs() < 1e-3);
    }

    #[test]
    fn test_std_simd_euclidean() {
        let a = vec![1.0; 64];
        let b = vec![2.0; 64];
        let expected = 8.0; // sqrt(64 * (1-2)^2) = sqrt(64) = 8
        let actual = super::euclidean_distance_std_simd(&a, &b);
        assert!((expected - actual).abs() < 1e-3);
    }

    #[test]
    fn test_std_simd_cosine() {
        let a = vec![1.0, 0.0, 1.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 1.0];
        let expected = 1.0; // Orthogonal
        let actual = super::cosine_distance_std_simd(&a, &b);
        assert!((expected - actual).abs() < 1e-3);

        let c = vec![1.0, 1.0];
        let d = vec![1.0, 1.0];
        let expected_same = 0.0; // Identical
        let actual_same = super::cosine_distance_std_simd(&c, &d);
        assert!((expected_same - actual_same).abs() < 1e-3);
    }
}
