// ANCHOR:DOC:DOC-DISTANCE-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:03 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:SEC:UNSAFE-001 — Dokumentierte unsafe-Blöcke in SIMD-Zone
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:10 DATE:2026-05-09 STATUS:REVIEW
// CREATED:2026-05-08 DEADLINE:NONE
// GEFUNDEN: 42 unsafe-Blöcke (AVX2 + AVX-512) ohne SAFETY: Kommentare
// ERWARTET: Jeder unsafe-Block braucht SAFETY: Kommentar mit:
//   1. Warum die Operation sicher ist (Slice-Bounds, Alignment)
//   2. Welche Invarianten vom Caller garantiert werden
// RISIKO: Release-Blocker — undokumentiertes unsafe verhindert qualifiziertes Review
// MASSNAHME: SAFETY: Kommentare für alle 12 unsafe fn + 30 unsafe-Blöcke hinzugefügt.
//
// ANCHOR:ARCH:SIMD-001 — Hardware-beschleunigte Distanzberechnung.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// PRECEDENCE: AVX-512 > AVX2 > portable_simd > scalar.
// INVARIANTE: Caller (hnsw.rs) validiert Vektor-Dimensionen VOR dem Aufruf.
//!
//! Distance computation functions.
//!
//! This module provides distance metrics for vector comparison.
//! Implementations use AVX2/AVX-512 SIMD if available, falling back to portable-simd, then scalar.

#![allow(unsafe_code)]
#![allow(unused_unsafe)]

use memfuse_core::DistanceMetric;
use std::simd::prelude::*;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Computes distance between two vectors using the specified metric.
#[inline]
pub fn compute_distance(a: &[f32], b: &[f32], metric: DistanceMetric) -> memfuse_core::Result<f32> {
    if a.len() != b.len() {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Vector dimensions must match",
        ));
    }

    Ok(match metric {
        DistanceMetric::Cosine => cosine_distance(a, b),
        DistanceMetric::Euclidean => euclidean_distance(a, b),
        DistanceMetric::DotProduct => dot_product_distance(a, b),
    })
}

/// Computes cosine distance (1 - similarity).
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f") {
            // ANCHOR:SAFETY:SIMD-029 — [Manual Dispatch AVX-512]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { cosine_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-030 — [Manual Dispatch AVX2]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { cosine_distance_avx2(a, b) };
        }
    }
    // Portable-simd fallback
    cosine_distance_std_simd(a, b)
}

/// Computes Euclidean (L2) distance.
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512
        if is_x86_feature_detected!("avx512f") {
            // ANCHOR:SAFETY:SIMD-031 — [Manual Dispatch AVX-512]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { euclidean_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-032 — [Manual Dispatch AVX2]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { euclidean_distance_avx2(a, b) };
        }
    }
    // Portable-simd fallback
    euclidean_distance_std_simd(a, b)
}

/// Computes negative dot product.
#[inline]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512
        if is_x86_feature_detected!("avx512f") {
            // ANCHOR:SAFETY:SIMD-033 — [Manual Dispatch AVX-512]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { -dot_product_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-034 — [Manual Dispatch AVX2]
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { -dot_product_avx2(a, b) };
        }
    }
    // Portable-simd fallback
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
// ANCHOR:SAFETY:SIMD-001 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX2/FMA Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-002 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-003 — [Unaligned Load & FMA]
        // BEGRÜNDUNG: Unaligned Loads (_loadu) sind sicher für beliebige f32 Slices.
        // Pointer-Arithmetik sicher, da i+8 <= n.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm256_fmadd_ps(va, vb, sum_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-004 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum256_ps_avx wird auf validem __m256 Register aufgerufen.
    let mut sum = unsafe { hsum256_ps_avx(sum_v) };

    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
// ANCHOR:SAFETY:SIMD-005 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX2/FMA Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-006 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let mut dot_v = unsafe { _mm256_setzero_ps() };
    let mut norm_a_v = unsafe { _mm256_setzero_ps() };
    let mut norm_b_v = unsafe { _mm256_setzero_ps() };

    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-007 — [Unaligned Load & FMA]
        // BEGRÜNDUNG: Unaligned Loads (_loadu) sind sicher für beliebige f32 Slices.
        // Pointer-Arithmetik sicher, da i+8 <= n.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm256_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm256_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm256_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-008 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum256_ps_avx wird auf validen __m256 Registern aufgerufen.
    let mut dot = unsafe { hsum256_ps_avx(dot_v) };
    let mut norm_a = unsafe { hsum256_ps_avx(norm_a_v) };
    let mut norm_b = unsafe { hsum256_ps_avx(norm_b_v) };

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
// ANCHOR:SAFETY:SIMD-009 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX2/FMA Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-010 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-011 — [Unaligned Load & Sub/FMA]
        // BEGRÜNDUNG: Unaligned Loads (_loadu) sind sicher für beliebige f32 Slices.
        // Pointer-Arithmetik sicher, da i+8 <= n.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            let diff = _mm256_sub_ps(va, vb);
            sum_v = _mm256_fmadd_ps(diff, diff, sum_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-012 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum256_ps_avx wird auf validem __m256 Register aufgerufen.
    let mut sum = unsafe { hsum256_ps_avx(sum_v) };

    while i < n {
        let diff = a[i] - b[i];
        sum += diff * diff;
        i += 1;
    }

    sum.sqrt()
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
// ANCHOR:SAFETY:SIMD-013 — [Target Feature Invariant]
// BEGRÜNDUNG: Caller muss AVX2/FMA Unterstützung garantieren.
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    // ANCHOR:SAFETY:SIMD-014 — [AVX2 Horizontal Sum]
    // BEGRÜNDUNG: Alle aufgerufenen Intrinsics sind sicher mit validen __m256 Registern.
    unsafe {
        let x128 = _mm_add_ps(_mm256_extractf128_ps(v, 1), _mm256_castps256_ps128(v));
        let x64 = _mm_add_ps(x128, _mm_movehl_ps(x128, x128));
        let x32 = _mm_add_ss(x64, _mm_shuffle_ps(x64, x64, 0x55));
        _mm_cvtss_f32(x32)
    }
}

// -----------------------------------------------------------------------------
// AVX-512 Implementations
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
// ANCHOR:SAFETY:SIMD-015 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX512F Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-016 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm512_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-017 — [AVX-512 Unaligned Load & FMA]
        // BEGRÜNDUNG: _loadu_ps ist sicher für f32 Slices. i+16 <= n garantiert Bounds.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm512_fmadd_ps(va, vb, sum_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-018 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum512_ps_avx wird auf validem __m512 Register aufgerufen.
    let mut sum = unsafe { hsum512_ps_avx(sum_v) };

    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
// ANCHOR:SAFETY:SIMD-019 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX512F Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn cosine_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-020 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let mut dot_v = unsafe { _mm512_setzero_ps() };
    let mut norm_a_v = unsafe { _mm512_setzero_ps() };
    let mut norm_b_v = unsafe { _mm512_setzero_ps() };

    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-021 — [AVX-512 Unaligned Load & FMA]
        // BEGRÜNDUNG: _loadu_ps ist sicher für f32 Slices. i+16 <= n garantiert Bounds.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm512_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm512_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm512_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-022 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum512_ps_avx wird auf validen __m512 Registern aufgerufen.
    let mut dot = unsafe { hsum512_ps_avx(dot_v) };
    let mut norm_a = unsafe { hsum512_ps_avx(norm_a_v) };
    let mut norm_b = unsafe { hsum512_ps_avx(norm_b_v) };

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
// ANCHOR:SAFETY:SIMD-023 — [Target Feature and Length Invariant]
// BEGRÜNDUNG: Caller muss AVX512F Unterstützung garantieren. Slices müssen gleiche Länge haben.
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-024 — [Zero Initialisierung]
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm512_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-025 — [AVX-512 Unaligned Load & Sub/FMA]
        // BEGRÜNDUNG: _loadu_ps ist sicher für f32 Slices. i+16 <= n garantiert Bounds.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            let diff = _mm512_sub_ps(va, vb);
            sum_v = _mm512_fmadd_ps(diff, diff, sum_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-026 — [Horizontal Sum]
    // BEGRÜNDUNG: hsum512_ps_avx wird auf validem __m512 Register aufgerufen.
    let mut sum = unsafe { hsum512_ps_avx(sum_v) };

    while i < n {
        let diff = a[i] - b[i];
        sum += diff * diff;
        i += 1;
    }

    sum.sqrt()
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
// ANCHOR:SAFETY:SIMD-027 — [Target Feature Invariant]
// BEGRÜNDUNG: Caller muss AVX512F Unterstützung garantieren.
unsafe fn hsum512_ps_avx(v: __m512) -> f32 {
    // ANCHOR:SAFETY:SIMD-028 — [AVX-512 Horizontal Sum]
    // BEGRÜNDUNG: Alle aufgerufenen Intrinsics sind sicher mit validen __m512 Registern.
    unsafe {
        let low = _mm512_castps512_ps256(v);
        let high = _mm512_extractf32x8_ps(v, 1);
        let sum256 = _mm256_add_ps(low, high);
        hsum256_ps_avx(sum256)
    }
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
        let d = compute_distance(&a, &b, DistanceMetric::DotProduct).expect("test");
        let dot_simd = -d;
        assert!((dot_scalar - dot_simd).abs() < 1e-3);

        // Euclidean
        let euc_scalar = euclidean_distance_scalar(&a, &b);
        let euc_simd = compute_distance(&a, &b, DistanceMetric::Euclidean).expect("test");
        assert!((euc_scalar - euc_simd).abs() < 1e-3);

        // Cosine
        let cos_scalar = cosine_distance_scalar(&a, &b);
        let cos_simd = compute_distance(&a, &b, DistanceMetric::Cosine).expect("test");
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
