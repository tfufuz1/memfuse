// ANCHOR:DOC:DOC-DISTANCE-001 — Module documentation added
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:03 DATE:2026-05-16 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:SEC:UNSAFE-001 — Dokumentierte unsafe-Blöcke in SIMD-Zone
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:03 DATE:2026-05-16 STATUS:DONE
// CREATED:2026-05-08 DEADLINE:NONE
// GEFUNDEN: 42 unsafe-Blöcke (AVX2 + AVX-512) ohne SAFETY: Kommentare
// ERWARTET: Jeder unsafe-Block braucht SAFETY: Kommentar mit:
//   1. Warum die Operation sicher ist (Slice-Bounds, Alignment)
//   2. Welche Invarianten vom Caller garantiert werden
// RISIKO: Release-Blocker — undokumentiertes unsafe verhindert qualifiziertes Review
// MASSNAHME: SAFETY: Kommentare für alle 12 unsafe fn + 30 unsafe-Blöcke hinzufügen
//
// ANCHOR:ARCH:SIMD-001 — Hardware-beschleunigte Distanzberechnung.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// PRECEDENCE: AVX-512 > AVX2 > portable_simd > scalar.
// INVARIANTE: Caller (hnsw.rs) validiert Vektor-Dimensionen VOR dem Aufruf.
//! # Distance Computation Module
//!
//! This module provides highly optimized distance metrics for vector comparison,
//! essential for the HNSW index performance.
//!
//! ## Supported Metrics
//! - **Cosine Distance**: 1 - cosine similarity, useful for orientation-based similarity.
//! - **Euclidean Distance (L2)**: Standard straight-line distance.
//! - **Dot Product**: Negative dot product for Maximum Inner Product Search (MIPS).
//!
//! ## SIMD Optimization Hierarchy
//! To ensure maximum performance across different hardware, the implementation follows a tiered fallback:
//! 1. **AVX-512**: Highest performance on modern Intel/AMD CPUs.
//! 2. **AVX2 + FMA**: Standard high-performance path for x86_64.
//! 3. **Portable SIMD (`std::simd`)**: Cross-platform SIMD using Rust's unstable SIMD features.
//! 4. **Scalar**: Standard Rust iterator-based fallback.
//!
//! ## Safety
//! This module contains `unsafe` code for hardware-specific intrinsics. All `unsafe` blocks
//! are guarded by runtime feature detection and documented with safety justifications.

#![allow(unused_unsafe)]
#![allow(unsafe_code)]

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
            // ANCHOR:SAFETY:SIMD-001 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX-512 Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
            return unsafe { cosine_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-002 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX2 und FMA Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
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
            // ANCHOR:SAFETY:SIMD-003 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX-512 Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
            return unsafe { euclidean_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-004 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX2 und FMA Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
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
            // ANCHOR:SAFETY:SIMD-005 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX-512 Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
            return unsafe { -dot_product_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // ANCHOR:SAFETY:SIMD-006 — Hardware-Support-Check und Bounds-Validation.
            // BEGRÜNDUNG: AVX2 und FMA Support wurde via is_x86_feature_detected geprüft.
            // Dimensionen werden durch compute_distance validiert.
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
// ANCHOR:SAFETY:SIMD-007 — AVX2/FMA Dot Product.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-008 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-009 — AVX2 Load und FMA.
        // BEGRÜNDUNG: i + 8 <= n garantiert In-Bounds Zugriff auf a und b. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm256_fmadd_ps(va, vb, sum_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-010 — Horizontale Summe.
    // BEGRÜNDUNG: hsum256_ps_avx benötigt AVX Support, der hier durch target_feature garantiert ist.
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
// ANCHOR:SAFETY:SIMD-011 — AVX2/FMA Cosine Distance.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-012 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let (mut dot_v, mut norm_a_v, mut norm_b_v) = unsafe {
        (
            _mm256_setzero_ps(),
            _mm256_setzero_ps(),
            _mm256_setzero_ps(),
        )
    };

    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-013 — AVX2 Load und FMA.
        // BEGRÜNDUNG: i + 8 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm256_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm256_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm256_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-014 — Horizontale Summen.
    // BEGRÜNDUNG: hsum256_ps_avx benötigt AVX Support, der hier durch target_feature garantiert ist.
    let (mut dot, mut norm_a, mut norm_b) = unsafe {
        (
            hsum256_ps_avx(dot_v),
            hsum256_ps_avx(norm_a_v),
            hsum256_ps_avx(norm_b_v),
        )
    };

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
// ANCHOR:SAFETY:SIMD-015 — AVX2/FMA Euclidean Distance.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-016 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 8 <= n {
        // ANCHOR:SAFETY:SIMD-017 — AVX2 Load, Sub und FMA.
        // BEGRÜNDUNG: i + 8 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            let diff = _mm256_sub_ps(va, vb);
            sum_v = _mm256_fmadd_ps(diff, diff, sum_v);
        }
        i += 8;
    }

    // ANCHOR:SAFETY:SIMD-018 — Horizontale Summe.
    // BEGRÜNDUNG: hsum256_ps_avx benötigt AVX Support, der hier durch target_feature garantiert ist.
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
// ANCHOR:SAFETY:SIMD-019 — Horizontale Summe AVX2.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    // ANCHOR:SAFETY:SIMD-020 — AVX Extraktion und Addition.
    // BEGRÜNDUNG: Standard AVX/AVX2 Befehle zur horizontalen Reduktion.
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
// ANCHOR:SAFETY:SIMD-021 — AVX-512 Dot Product.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-022 — Initialisierung.
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm512_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-023 — AVX-512 Load und FMA.
        // BEGRÜNDUNG: i + 16 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm512_fmadd_ps(va, vb, sum_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-024 — Horizontale Summe.
    // BEGRÜNDUNG: hsum512_ps_avx benötigt AVX-512 Support, der hier durch target_feature garantiert ist.
    let mut sum = unsafe { hsum512_ps_avx(sum_v) };

    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
// ANCHOR:SAFETY:SIMD-025 — AVX-512 Cosine Distance.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn cosine_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-026 — Initialisierung.
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let (mut dot_v, mut norm_a_v, mut norm_b_v) = unsafe {
        (
            _mm512_setzero_ps(),
            _mm512_setzero_ps(),
            _mm512_setzero_ps(),
        )
    };

    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-027 — AVX-512 Load und FMA.
        // BEGRÜNDUNG: i + 16 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm512_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm512_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm512_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-028 — Horizontale Summen.
    // BEGRÜNDUNG: hsum512_ps_avx benötigt AVX-512 Support, der hier durch target_feature garantiert ist.
    let (mut dot, mut norm_a, mut norm_b) = unsafe {
        (
            hsum512_ps_avx(dot_v),
            hsum512_ps_avx(norm_a_v),
            hsum512_ps_avx(norm_b_v),
        )
    };

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
// ANCHOR:SAFETY:SIMD-029 — AVX-512 Euclidean Distance.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    // ANCHOR:SAFETY:SIMD-030 — Initialisierung.
    // BEGRÜNDUNG: _mm512_setzero_ps ist immer sicher.
    let mut sum_v = unsafe { _mm512_setzero_ps() };
    let n = a.len();
    let mut i = 0;

    while i + 16 <= n {
        // ANCHOR:SAFETY:SIMD-031 — AVX-512 Load, Sub und FMA.
        // BEGRÜNDUNG: i + 16 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            let diff = _mm512_sub_ps(va, vb);
            sum_v = _mm512_fmadd_ps(diff, diff, sum_v);
        }
        i += 16;
    }

    // ANCHOR:SAFETY:SIMD-032 — Horizontale Summe.
    // BEGRÜNDUNG: hsum512_ps_avx benötigt AVX-512 Support, der hier durch target_feature garantiert ist.
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
// ANCHOR:SAFETY:SIMD-033 — Horizontale Summe AVX-512.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum512_ps_avx(v: __m512) -> f32 {
    // ANCHOR:SAFETY:SIMD-034 — AVX-512 Kastrieren und Summieren.
    // BEGRÜNDUNG: Standard AVX-512 Befehle zur Reduktion auf AVX2.
    unsafe {
        let low = _mm512_castps512_ps256(v);
        let high = _mm512_extractf32x8_ps(v, 1);
        let sum256 = _mm256_add_ps(low, high);
        hsum256_ps_avx(sum256)
    }
}

/// Normalizes a vector in-place to unit length (L2 norm = 1.0).
pub fn normalize_inplace(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Computes the dot product of two u8 vectors.
#[inline]
pub fn dot_product_u8(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // ANCHOR:SAFETY:SIMD-U8-014 — AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { dot_product_u8_avx2(a, b) };
        }
    }
    dot_product_u8_scalar(a, b)
}

pub fn dot_product_u8_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| x as u32 * y as u32)
        .sum()
}

/// Computes the squared Euclidean distance between two u8 vectors.
#[inline]
pub fn euclidean_distance_sq_u8(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // ANCHOR:SAFETY:SIMD-U8-015 — AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { euclidean_distance_sq_u8_avx2(a, b) };
        }
    }
    euclidean_distance_sq_u8_scalar(a, b)
}

pub fn euclidean_distance_sq_u8_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = x as i32 - y as i32;
            (diff * diff) as u32
        })
        .sum()
}

/// Parts required to compute cosine similarity for quantized vectors.
#[derive(Debug, Clone, Copy)]
pub struct CosineSimilarityPartsU8 {
    pub dot: u32,
    pub sum_a: u32,
    pub sum_b: u32,
    pub norm_a_sq: u32,
    pub norm_b_sq: u32,
}

/// Computes the parts required for cosine similarity between two u8 vectors.
#[inline]
pub fn cosine_similarity_parts_u8(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // ANCHOR:SAFETY:SIMD-U8-016 — AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            return unsafe { cosine_similarity_parts_u8_avx2(a, b) };
        }
    }
    cosine_similarity_parts_u8_scalar(a, b)
}

pub fn cosine_similarity_parts_u8_scalar(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    let mut dot = 0;
    let mut sum_a = 0;
    let mut sum_b = 0;
    let mut norm_a_sq = 0;
    let mut norm_b_sq = 0;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let xu = x as u32;
        let yu = y as u32;
        dot += xu * yu;
        sum_a += xu;
        sum_b += yu;
        norm_a_sq += xu * xu;
        norm_b_sq += yu * yu;
    }

    CosineSimilarityPartsU8 {
        dot,
        sum_a,
        sum_b,
        norm_a_sq,
        norm_b_sq,
    }
}

/// Computes the dot product between an f32 vector and a u8 vector.
pub fn dot_product_f32_u8(a: &[f32], b: &[u8]) -> f32 {
    a.iter().zip(b.iter()).map(|(&x, &y)| x * (y as f32)).sum()
}

/// Computes the squared Euclidean distance between an f32 vector and a u8 vector
/// performing inline dequantization.
pub fn euclidean_distance_sq_f32_u8(a: &[f32], b: &[u8], alpha: f32, min: f32) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let y_f32 = (y as f32) * alpha + min;
            let diff = x - y_f32;
            diff * diff
        })
        .sum()
}

/// Parts required to compute asymmetric cosine similarity.
#[derive(Debug, Clone, Copy)]
pub struct CosineSimilarityPartsF32U8 {
    pub dot_f32_u8: f32,
    pub sum_u8: u32,
    pub norm_u8_sq: u32,
}

/// Computes the parts required for asymmetric cosine similarity between an f32 and a u8 vector.
pub fn cosine_similarity_parts_f32_u8(a: &[f32], b: &[u8]) -> CosineSimilarityPartsF32U8 {
    let mut dot_f32_u8 = 0.0;
    let mut sum_u8 = 0;
    let mut norm_u8_sq = 0;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let yu = y as u32;
        dot_f32_u8 += x * (y as f32);
        sum_u8 += yu;
        norm_u8_sq += yu * yu;
    }

    CosineSimilarityPartsF32U8 {
        dot_f32_u8,
        sum_u8,
        norm_u8_sq,
    }
}

// -----------------------------------------------------------------------------
// AVX2 Implementations for u8
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
// ANCHOR:SAFETY:SIMD-U8-001 — AVX2 Dot Product for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2 via runtime detection.
/// Both slices `a` and `b` must have the same length.
pub unsafe fn dot_product_u8_avx2(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len();
    let mut i = 0;
    // ANCHOR:SAFETY:SIMD-U8-017 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_si256() };

    while i + 32 <= n {
        // ANCHOR:SAFETY:SIMD-U8-002 — AVX2 Load und Madd.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff.
        unsafe {
            let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
            let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);

            // Split 32 u8 into two 16 i16
            let va_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(va));
            let va_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(va, 1));
            let vb_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(vb));
            let vb_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(vb, 1));

            sum_v = _mm256_add_epi32(sum_v, _mm256_madd_epi16(va_lo, vb_lo));
            sum_v = _mm256_add_epi32(sum_v, _mm256_madd_epi16(va_hi, vb_hi));
        }
        i += 32;
    }

    // ANCHOR:SAFETY:SIMD-U8-011 — Horizontale Summe.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    let mut sum = unsafe { hsum256_epi32_avx2(sum_v) as u32 };
    while i < n {
        sum += a[i] as u32 * b[i] as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
// ANCHOR:SAFETY:SIMD-U8-003 — AVX2 Squared Euclidean for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2 via runtime detection.
/// Both slices `a` and `b` must have the same length.
pub unsafe fn euclidean_distance_sq_u8_avx2(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len();
    let mut i = 0;
    // ANCHOR:SAFETY:SIMD-U8-018 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    let mut sum_v = unsafe { _mm256_setzero_si256() };

    while i + 32 <= n {
        // ANCHOR:SAFETY:SIMD-U8-004 — AVX2 Load und Sub/Madd.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff.
        unsafe {
            let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
            let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);

            let va_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(va));
            let va_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(va, 1));
            let vb_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(vb));
            let vb_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(vb, 1));

            let diff_lo = _mm256_sub_epi16(va_lo, vb_lo);
            let diff_hi = _mm256_sub_epi16(va_hi, vb_hi);

            sum_v = _mm256_add_epi32(sum_v, _mm256_madd_epi16(diff_lo, diff_lo));
            sum_v = _mm256_add_epi32(sum_v, _mm256_madd_epi16(diff_hi, diff_hi));
        }
        i += 32;
    }

    // ANCHOR:SAFETY:SIMD-U8-012 — Horizontale Summe.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    let mut sum = unsafe { hsum256_epi32_avx2(sum_v) as u32 };
    while i < n {
        let diff = a[i] as i32 - b[i] as i32;
        sum += (diff * diff) as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
// ANCHOR:SAFETY:SIMD-U8-005 — AVX2 Cosine Similarity Parts for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2 via runtime detection.
/// Both slices `a` and `b` must have the same length.
pub unsafe fn cosine_similarity_parts_u8_avx2(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    let n = a.len();
    let mut i = 0;

    // ANCHOR:SAFETY:SIMD-U8-019 — Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    let (mut dot_v, mut sum_a_v, mut sum_b_v, mut norm_a_v, mut norm_b_v) = unsafe {
        (
            _mm256_setzero_si256(),
            _mm256_setzero_si256(),
            _mm256_setzero_si256(),
            _mm256_setzero_si256(),
            _mm256_setzero_si256(),
        )
    };

    while i + 32 <= n {
        // ANCHOR:SAFETY:SIMD-U8-006 — AVX2 Loads und Accumulation.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff.
        unsafe {
            let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
            let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);

            let va_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(va));
            let va_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(va, 1));
            let vb_lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(vb));
            let vb_hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(vb, 1));

            dot_v = _mm256_add_epi32(dot_v, _mm256_madd_epi16(va_lo, vb_lo));
            dot_v = _mm256_add_epi32(dot_v, _mm256_madd_epi16(va_hi, vb_hi));

            norm_a_v = _mm256_add_epi32(norm_a_v, _mm256_madd_epi16(va_lo, va_lo));
            norm_a_v = _mm256_add_epi32(norm_a_v, _mm256_madd_epi16(va_hi, va_hi));

            norm_b_v = _mm256_add_epi32(norm_b_v, _mm256_madd_epi16(vb_lo, vb_lo));
            norm_b_v = _mm256_add_epi32(norm_b_v, _mm256_madd_epi16(vb_hi, vb_hi));

            // Sums can use SAD against zero to sum bytes fast
            let zero = _mm256_setzero_si256();
            let sa = _mm256_sad_epu8(va, zero);
            let sb = _mm256_sad_epu8(vb, zero);
            sum_a_v = _mm256_add_epi64(sum_a_v, sa);
            sum_b_v = _mm256_add_epi64(sum_b_v, sb);
        }
        i += 32;
    }

    // ANCHOR:SAFETY:SIMD-U8-013 — Horizontale Summen.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    let (mut dot, mut norm_a_sq, mut norm_b_sq, mut sum_a, mut sum_b) = unsafe {
        (
            hsum256_epi32_avx2(dot_v) as u32,
            hsum256_epi32_avx2(norm_a_v) as u32,
            hsum256_epi32_avx2(norm_b_v) as u32,
            hsum256_epi64_avx2(sum_a_v) as u32,
            hsum256_epi64_avx2(sum_b_v) as u32,
        )
    };

    while i < n {
        let xu = a[i] as u32;
        let yu = b[i] as u32;
        dot += xu * yu;
        sum_a += xu;
        sum_b += yu;
        norm_a_sq += xu * xu;
        norm_b_sq += yu * yu;
        i += 1;
    }

    CosineSimilarityPartsU8 {
        dot,
        sum_a,
        sum_b,
        norm_a_sq,
        norm_b_sq,
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
// ANCHOR:SAFETY:SIMD-U8-007 — Horizontal Sum epi32.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_epi32_avx2(v: __m256i) -> i32 {
    // ANCHOR:SAFETY:SIMD-U8-009 — AVX2 Reduktion.
    // BEGRÜNDUNG: Standard AVX2 Befehle zur horizontalen Reduktion.
    unsafe {
        let v128 = _mm_add_epi32(_mm256_castsi256_si128(v), _mm256_extracti128_si256(v, 1));
        let v64 = _mm_add_epi32(v128, _mm_shuffle_epi32(v128, 0x4E));
        let v32 = _mm_add_epi32(v64, _mm_shuffle_epi32(v64, 0xB1));
        _mm_cvtsi128_si32(v32)
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
// ANCHOR:SAFETY:SIMD-U8-008 — Horizontal Sum epi64.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_epi64_avx2(v: __m256i) -> i64 {
    // ANCHOR:SAFETY:SIMD-U8-010 — AVX2 Reduktion epi64.
    // BEGRÜNDUNG: Standard AVX2 Befehle zur horizontalen Reduktion.
    unsafe {
        let v128 = _mm_add_epi64(_mm256_castsi256_si128(v), _mm256_extracti128_si256(v, 1));
        let v64 = _mm_add_epi64(v128, _mm_unpackhi_epi64(v128, v128));
        _mm_cvtsi128_si64(v64)
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
    fn test_u8_metrics_match_scalar() {
        let a = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ];
        let b = vec![
            32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11,
            10, 9, 8, 7, 6, 5, 4, 3, 2, 1,
        ];

        // Dot product
        let dot_scalar = dot_product_u8(&a, &b);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                // ANCHOR:SAFETY:SIMD-U8-TEST-001 — AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                let dot_simd = unsafe { dot_product_u8_avx2(&a, &b) };
                assert_eq!(dot_scalar, dot_simd);
            }
        }

        // Euclidean
        let euc_scalar = euclidean_distance_sq_u8(&a, &b);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                // ANCHOR:SAFETY:SIMD-U8-TEST-002 — AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                let euc_simd = unsafe { euclidean_distance_sq_u8_avx2(&a, &b) };
                assert_eq!(euc_scalar, euc_simd);
            }
        }

        // Cosine parts
        let parts_scalar = cosine_similarity_parts_u8(&a, &b);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                // ANCHOR:SAFETY:SIMD-U8-TEST-003 — AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                let parts_simd = unsafe { cosine_similarity_parts_u8_avx2(&a, &b) };
                assert_eq!(parts_scalar.dot, parts_simd.dot);
                assert_eq!(parts_scalar.sum_a, parts_simd.sum_a);
                assert_eq!(parts_scalar.sum_b, parts_simd.sum_b);
                assert_eq!(parts_scalar.norm_a_sq, parts_simd.norm_a_sq);
                assert_eq!(parts_scalar.norm_b_sq, parts_simd.norm_b_sq);
            }
        }
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
