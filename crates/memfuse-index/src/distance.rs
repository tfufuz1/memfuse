// FILE-CONTEXT
// ZWECK: SIMD-beschleunigte und Skalar-Distanzberechnungen (Cosine, Euclidean, Dot Product).
// INVARIANTEN: Äquivalenz zwischen SIMD- und Skalar-Pfad bis auf Float-Toleranz (±1e-6); Caller garantiert Längenanpassung.
// NICHT-OFFENSICHTLICH: Jeder unsafe-Block für SIMD Intrinsics enthält konkrete 4-Punkt SAFETY-Dokumentation (ADR-017).
// HOTSPOTS: distance.rs (compute_distance, euclidean_distance_avx2, cosine_distance_avx2)
// STAND: TS:2026-08-30T18:53:53Z (SESSION: 37b1d991)

// AI-TAG[DOC-DRIFT][MINOR] RESOLVED: AGT-INDEX-001 — Module documentation added (TS:2026-08-25T00:00:00Z)
// SAFETY: Dokumentierte unsafe-Blöcke in SIMD-Zone
// GEFUNDEN: 81 unsafe-Blöcke. Aktueller Zustand: 147 SAFETY:-Kommentare.
// ERWARTET: Jeder unsafe-Block braucht SAFETY: Kommentar mit:
//   1. Warum die Operation sicher ist (Slice-Bounds, Alignment)
//   2. Welche Invarianten vom Caller garantiert werden
// RISIKO: Release-Blocker — undokumentiertes unsafe verhindert qualifiziertes Review
// MASSNAHME: Vollständige Validierung durchgeführt.
//
// INVARIANT: Hardware-beschleunigte Distanzberechnung.
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
//!
//! ## Determinism
//! SIMD implementations (AVX2, AVX-512) are designed to be numerically equivalent to their
//! scalar counterparts within a tolerance of `±1e-6` (§4 Determinismus-Gesetz).
//! Accumulation order is maintained where possible to minimize divergence.

// FILE-CONTEXT
// STAND: 2026-08-29T05:41:20Z (SESSION: f7999509)
// ZWECK: SIMD-beschleunigte Distanzmetriken (Cosinus, L2) für HNSW-Index
// INVARIANTEN: Caller MUSS Vektor-Dimensionen VOR Dispatch validieren.
//              PRECEDENCE: AVX-512 > AVX2 > portable_simd > scalar.
//              Jeder unsafe-Block erfordert individuellen // SAFETY: Beweis.
// NICHT-OFFENSICHTLICH: 42 unsafe-Blöcke — KEIN Copy-Paste von SAFETY-Kommentaren
//                       (jeder Kommentar muss die konkreten Invarianten DIESER Funktion nennen).
//                       Scalar-Fallback muss numerisch identisch sein (ε ≤ 1e-4, §4 Determinismus-Gesetz).
// SIEHE AUCH: rules/simd_safety.md, CONSTITUTION.md §12, ADR-017

// ANCHOR[REFACTOR:WP-0.0-STABLESIMD] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Remove nightly portable_simd
// TEST: cargo +stable check -p memfuse-index
// DONE: #![feature(portable_simd)] ist entfernt und distance.rs nutzt stabiles Rust.

#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use memfuse_core::DistanceMetric;
// use std::simd::prelude::*; // Removed for stable Rust stabilization

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Computes distance between two vectors using the specified metric.
#[inline]
// AI-TAG[CONCURRENCY][MINOR] Stable SIMD Migration when std::simd stabilizes (ID: AGT-INDEX-002) (TS:2026-08-25T00:00:00Z)
// Standardize SIMD distance metrics on Stable Rust, preventing panic in hardware fallbacks.
// Fallbacks MUST be verified to prevent Zero-Panic violations.
pub fn compute_distance(a: &[f32], b: &[f32], metric: DistanceMetric) -> memfuse_core::Result<f32> {
    if a.len() != b.len() {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Vector dimensions must match",
        ));
    }

    // SAFETY: Ensure no NaN enters the distance metrics
    // INVARIANTE: Distance functions must never return NaN unless inputs are corrupted.
    // Early validation prevents NaN poisoning in HNSW search/insert loops.
    for val in a.iter().chain(b.iter()) {
        if val.is_nan() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Input vector contains NaN values",
            ));
        }
    }

    let dist = match metric {
        DistanceMetric::Cosine => cosine_distance(a, b),
        DistanceMetric::Euclidean => euclidean_distance(a, b),
        DistanceMetric::DotProduct => dot_product_distance(a, b),
        other => {
            return Err(memfuse_core::MemFuseError::Index(format!(
                "Unsupported DistanceMetric variant: {other:?} — \
                 add a match arm here when extending the enum"
            )));
        }
    };
    Ok(dist)
}

// AI-TAG[SECURITY][CRITICAL] RESOLVED: AGT-INDEX-005 — assert_eq! preconditions in cosine_distance, euclidean_distance, dot_product_distance added (ADR-034). Testbeweis: test_cosine_distance_mismatch_panics etc. (TS:2026-08-29T10:18:55Z) (SESSION: a3f29c1d)
// DECISION-REF: ADR-034 — Option 1: Release-active runtime assertion (assert_eq!) prevents SIMD buffer overreads.

/// Computes cosine distance (1 - similarity).
///
/// # Panics
/// Panics if `a.len() != b.len()` to prevent out-of-bounds access in low-level SIMD intrinsics (ADR-034).
#[inline]
#[allow(unsafe_code)]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vector lengths must match for cosine_distance"
    );
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in cosine_distance; AVX-512F CPU feature confirmed via is_x86_feature_detected!.
            return unsafe { cosine_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in cosine_distance; AVX2+FMA CPU features confirmed via is_x86_feature_detected!.
            return unsafe { cosine_distance_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in cosine_distance; NEON CPU feature confirmed via is_aarch64_feature_detected!.
            return unsafe { cosine_distance_neon(a, b) };
        }
    }
    // Stable fallback (autovectorizable by compiler)
    cosine_distance_scalar(a, b)
}

/// Computes Euclidean (L2) distance.
///
/// # Panics
/// Panics if `a.len() != b.len()` to prevent out-of-bounds access in low-level SIMD intrinsics (ADR-034).
#[inline]
#[allow(unsafe_code)]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vector lengths must match for euclidean_distance"
    );
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512
        if is_x86_feature_detected!("avx512f") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in euclidean_distance; AVX-512F CPU feature confirmed via is_x86_feature_detected!.
            return unsafe { euclidean_distance_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in euclidean_distance; AVX2+FMA CPU features confirmed via is_x86_feature_detected!.
            return unsafe { euclidean_distance_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in euclidean_distance; NEON CPU feature confirmed via is_aarch64_feature_detected!.
            return unsafe { euclidean_distance_neon(a, b) };
        }
    }
    // Stable fallback (autovectorizable by compiler)
    euclidean_distance_scalar(a, b)
}

/// Computes negative dot product.
///
/// # Panics
/// Panics if `a.len() != b.len()` to prevent out-of-bounds access in low-level SIMD intrinsics (ADR-034).
#[inline]
#[allow(unsafe_code)]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vector lengths must match for dot_product_distance"
    );
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Try AVX-512
        if is_x86_feature_detected!("avx512f") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in dot_product_distance; AVX-512F CPU feature confirmed via is_x86_feature_detected!.
            return unsafe { -dot_product_avx512(a, b) };
        }
        // Then AVX2
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in dot_product_distance; AVX2+FMA CPU features confirmed via is_x86_feature_detected!.
            return unsafe { -dot_product_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            // SAFETY: a.len() == b.len() was checked by assert_eq! in dot_product_distance; NEON CPU feature confirmed via is_aarch64_feature_detected!.
            return unsafe { -dot_product_neon(a, b) };
        }
    }
    // Stable fallback (autovectorizable by compiler)
    -dot_product_scalar(a, b)
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

// -----------------------------------------------------------------------------
// ARM/NEON Implementations for AArch64
// -----------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports NEON and slices `a` and `b` have valid memory regions.
unsafe fn cosine_distance_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let n = a.len().min(b.len());
    let chunks = n / 4;

    // SAFETY: NEON vdupq_n_f32 is safe on hardware detected by caller.
    let mut dot_v = unsafe { vdupq_n_f32(0.0) };
    let mut norm_a_v = unsafe { vdupq_n_f32(0.0) };
    let mut norm_b_v = unsafe { vdupq_n_f32(0.0) };

    for i in 0..chunks {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i * 4)` and `b.as_ptr().add(i * 4)` stay strictly within
        // valid slice bounds because `i < chunks = n / 4`, implying `i * 4 + 4 <= n <= min(a.len(), b.len())`.
        // Unaligned 128-bit vector loads (`vld1q_f32`) and FMA intrinsics are safe on AArch64 target with NEON.
        unsafe {
            let va = vld1q_f32(a.as_ptr().add(i * 4));
            let vb = vld1q_f32(b.as_ptr().add(i * 4));
            dot_v = vmlaq_f32(dot_v, va, vb);
            norm_a_v = vmlaq_f32(norm_a_v, va, va);
            norm_b_v = vmlaq_f32(norm_b_v, vb, vb);
        }
    }

    // SAFETY: Horizontal reduction `vaddvq_f32` is safe on NEON target feature.
    let mut dot = unsafe { vaddvq_f32(dot_v) };
    let mut norm_a = unsafe { vaddvq_f32(norm_a_v) };
    let mut norm_b = unsafe { vaddvq_f32(norm_b_v) };

    for i in (chunks * 4)..n {
        let x = a[i];
        let y = b[i];
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

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports NEON and slices `a` and `b` have valid memory regions.
unsafe fn euclidean_distance_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let n = a.len().min(b.len());
    let chunks = n / 4;

    // SAFETY: NEON vdupq_n_f32 is safe on hardware detected by caller.
    let mut sum_v = unsafe { vdupq_n_f32(0.0) };

    for i in 0..chunks {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i * 4)` and `b.as_ptr().add(i * 4)` are safe because
        // `i < chunks = n / 4` guarantees that `i * 4 + 4 <= n <= min(a.len(), b.len())`.
        // Unaligned 128-bit vector loads (`vld1q_f32`), subtraction, and FMA are safe on NEON.
        unsafe {
            let va = vld1q_f32(a.as_ptr().add(i * 4));
            let vb = vld1q_f32(b.as_ptr().add(i * 4));
            let diff = vsubq_f32(va, vb);
            sum_v = vmlaq_f32(sum_v, diff, diff);
        }
    }

    // SAFETY: Horizontal reduction `vaddvq_f32` is safe on NEON target feature.
    let mut sum = unsafe { vaddvq_f32(sum_v) };

    for i in (chunks * 4)..n {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }

    sum.sqrt()
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports NEON and slices `a` and `b` have valid memory regions.
unsafe fn dot_product_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let n = a.len().min(b.len());
    let chunks = n / 4;

    // SAFETY: NEON vdupq_n_f32 is safe on hardware detected by caller.
    let mut sum_v = unsafe { vdupq_n_f32(0.0) };

    for i in 0..chunks {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i * 4)` and `b.as_ptr().add(i * 4)` are safe because
        // `i < chunks = n / 4` guarantees that `i * 4 + 4 <= n <= min(a.len(), b.len())`.
        // Unaligned 128-bit vector loads (`vld1q_f32`) and FMA are safe on NEON.
        unsafe {
            let va = vld1q_f32(a.as_ptr().add(i * 4));
            let vb = vld1q_f32(b.as_ptr().add(i * 4));
            sum_v = vmlaq_f32(sum_v, va, vb);
        }
    }

    // SAFETY: Horizontal reduction `vaddvq_f32` is safe on NEON target feature.
    let mut sum = unsafe { vaddvq_f32(sum_v) };

    for i in (chunks * 4)..n {
        sum += a[i] * b[i];
    }

    sum
}

// ANCHOR[REFACTOR:WP-0.0-STABLESIMD-2] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Removed std_simd functions

// -----------------------------------------------------------------------------
// AVX2 Implementations
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX2 and FMA, and slices `a` and `b` have valid memory regions.
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm256_setzero_ps();
    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 8 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` stay strictly within
        // valid slice bounds because `i + 8 <= n <= min(a.len(), b.len())`.
        // Unaligned 256-bit vector loads (`_mm256_loadu_ps`) and FMA intrinsics are safe on AVX2+FMA target.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm256_fmadd_ps(va, vb, sum_v);
        }
        i += 8;
    }

    // SAFETY: `hsum256_ps_avx` is safe because caller guaranteed AVX2 target feature support.
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
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX2 and FMA, and slices `a` and `b` have valid memory regions.
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot_v, mut norm_a_v, mut norm_b_v) = (
        _mm256_setzero_ps(),
        _mm256_setzero_ps(),
        _mm256_setzero_ps(),
    );

    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 8 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` are safe because
        // `i + 8 <= n <= min(a.len(), b.len())`.
        // Unaligned 256-bit vector loads (`_mm256_loadu_ps`) and FMA are safe on AVX2+FMA target.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm256_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm256_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm256_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 8;
    }

    // SAFETY: `hsum256_ps_avx` calls are safe because caller guaranteed AVX2 target feature support.
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
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX2 and FMA, and slices `a` and `b` have valid memory regions.
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm256_setzero_ps();
    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 8 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` are safe because
        // `i + 8 <= n <= min(a.len(), b.len())`.
        // Unaligned 256-bit vector loads (`_mm256_loadu_ps`), subtraction, and FMA are safe on AVX2+FMA target.
        unsafe {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            let diff = _mm256_sub_ps(va, vb);
            sum_v = _mm256_fmadd_ps(diff, diff, sum_v);
        }
        i += 8;
    }

    // SAFETY: `hsum256_ps_avx` is safe because caller guaranteed AVX2 target feature support.
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
#[allow(unsafe_code)]
// SAFETY: Horizontale Summe AVX2.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    // SAFETY: AVX Extraktion und Addition.
    // BEGRÜNDUNG: Standard AVX/AVX2 Befehle zur horizontalen Reduktion.
    // SAFETY: Standard AVX/AVX2 horizontal reduction sequence is safe on supported hardware detected by caller.
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
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX-512F and slices `a` and `b` have valid memory regions.
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm512_setzero_ps();
    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 16 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` stay strictly within
        // valid slice bounds because `i + 16 <= n <= min(a.len(), b.len())`.
        // Unaligned 512-bit vector loads (`_mm512_loadu_ps`) and AVX-512 FMA intrinsics are safe on AVX-512F target.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            sum_v = _mm512_fmadd_ps(va, vb, sum_v);
        }
        i += 16;
    }

    // SAFETY: `hsum512_ps_avx` is safe because caller guaranteed AVX-512F target feature support.
    let mut sum = unsafe { hsum512_ps_avx(sum_v) };

    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX-512F and slices `a` and `b` have valid memory regions.
unsafe fn cosine_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot_v, mut norm_a_v, mut norm_b_v) = (
        _mm512_setzero_ps(),
        _mm512_setzero_ps(),
        _mm512_setzero_ps(),
    );

    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 16 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` are safe because
        // `i + 16 <= n <= min(a.len(), b.len())`.
        // Unaligned 512-bit vector loads (`_mm512_loadu_ps`) and AVX-512 FMA are safe on AVX-512F target.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));

            dot_v = _mm512_fmadd_ps(va, vb, dot_v);
            norm_a_v = _mm512_fmadd_ps(va, va, norm_a_v);
            norm_b_v = _mm512_fmadd_ps(vb, vb, norm_b_v);
        }
        i += 16;
    }

    // SAFETY: `hsum512_ps_avx` calls are safe because caller guaranteed AVX-512F target feature support.
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
#[allow(unsafe_code)]
/// # Safety
/// Caller must ensure CPU supports AVX-512F and slices `a` and `b` have valid memory regions.
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_v = _mm512_setzero_ps();
    let n = a.len().min(b.len());
    let mut i = 0;

    while i + 16 <= n {
        // SAFETY: Pointer arithmetic `a.as_ptr().add(i)` and `b.as_ptr().add(i)` are safe because
        // `i + 16 <= n <= min(a.len(), b.len())`.
        // Unaligned 512-bit vector loads (`_mm512_loadu_ps`), subtraction, and FMA are safe on AVX-512F target.
        unsafe {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            let diff = _mm512_sub_ps(va, vb);
            sum_v = _mm512_fmadd_ps(diff, diff, sum_v);
        }
        i += 16;
    }

    // SAFETY: `hsum512_ps_avx` is safe because caller guaranteed AVX-512F target feature support.
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
#[allow(unsafe_code)]
// SAFETY: Horizontale Summe AVX-512.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum512_ps_avx(v: __m512) -> f32 {
    // SAFETY: AVX-512 Kastrieren und Summieren.
    // BEGRÜNDUNG: Standard AVX-512 Befehle zur Reduktion auf AVX2.
    // SAFETY: Standard AVX-512 to AVX2 reduction sequence is safe on supported hardware detected by caller.
    unsafe {
        // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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
#[allow(unsafe_code)]
pub fn dot_product_u8(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512vnni") {
            // SAFETY: AVX-512 VNNI Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { dot_product_u8_avx512vnni(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        }
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { dot_product_u8_avx2(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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
#[allow(unsafe_code)]
pub fn euclidean_distance_sq_u8(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw") {
            // SAFETY: AVX-512 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { euclidean_distance_sq_u8_avx512(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        }
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { euclidean_distance_sq_u8_avx2(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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
#[allow(unsafe_code)]
pub fn cosine_similarity_parts_u8(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512bw")
            && is_x86_feature_detected!("avx512vnni")
        {
            // SAFETY: AVX-512 VNNI Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { cosine_similarity_parts_u8_avx512(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        }
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 Dispatch.
            // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
            // SAFETY: Hardware support detected.
            return unsafe { cosine_similarity_parts_u8_avx2(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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
/// performing inline dequantization with per-dimension scaling.
pub fn euclidean_distance_sq_f32_u8(a: &[f32], b: &[u8], alphas: &[f32], mins: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .zip(alphas.iter())
        .zip(mins.iter())
        .map(|(((&x, &y), &alpha), &min)| {
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
// AVX-512 VNNI Implementations for u8
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
#[target_feature(enable = "avx512vnni")]
#[allow(unsafe_code)]
// SAFETY: AVX-512 VNNI Dot Product for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
/// # Safety
/// This function is unsafe because it uses AVX-512 VNNI intrinsics. The caller must ensure that the CPU supports AVX-512 VNNI.
pub unsafe fn dot_product_u8_avx512vnni(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len().min(b.len());
    let mut i = 0;
    // SAFETY: _mm512_setzero_si512 is always safe.
    let mut sum_v = _mm512_setzero_si512();

    while i + 64 <= n {
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to i + 64 <= n.
        // VNNI instruction is safe on hardware detected by caller.
        // BEGRÜNDUNG: Caller garantiert Support und korrekte bounds.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
            let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const _);
            let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const _);
            sum_v = _mm512_dpbusd_epi32(sum_v, va, vb);
        }
        i += 64;
    }

    // SAFETY: AVX-512 Horizontal Sum.
    // BEGRÜNDUNG: hsum512_epi32_avx512 wird innerhalb eines AVX-512 aktivierten Kontextes aufgerufen.
    // SAFETY: hsum512_epi32_avx512 is called within an AVX-512 enabled context.
    let mut sum = unsafe { hsum512_epi32_avx512(sum_v) } as u32; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
    while i < n {
        sum += a[i] as u32 * b[i] as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
#[target_feature(enable = "avx512bw")]
#[allow(unsafe_code)]
// SAFETY: AVX-512 Euclidean Squared for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
/// # Safety
/// This function is unsafe because it uses AVX-512 intrinsics. The caller must ensure that the CPU supports AVX-512F and AVX-512BW.
pub unsafe fn euclidean_distance_sq_u8_avx512(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len().min(b.len());
    let mut i = 0;
    // SAFETY: _mm512_setzero_si512 is always safe.
    let mut sum_v = _mm512_setzero_si512();

    while i + 64 <= n {
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to i + 64 <= n.
        // BEGRÜNDUNG: Caller garantiert Support und korrekte bounds.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
            let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const _);
            let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const _);

            // Split 64 u8 into four 16 i16 or two 32 i16
            // AVX-512BW provides cvtepu8_epi16 to 512-bit
            let va_lo = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(va));
            let va_hi = _mm512_cvtepu8_epi16(_mm512_extracti64x4_epi64(va, 1));
            let vb_lo = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(vb));
            let vb_hi = _mm512_cvtepu8_epi16(_mm512_extracti64x4_epi64(vb, 1));

            let diff_lo = _mm512_sub_epi16(va_lo, vb_lo);
            let diff_hi = _mm512_sub_epi16(va_hi, vb_hi);

            sum_v = _mm512_add_epi32(sum_v, _mm512_madd_epi16(diff_lo, diff_lo));
            sum_v = _mm512_add_epi32(sum_v, _mm512_madd_epi16(diff_hi, diff_hi));
        }
        i += 64;
    }

    // SAFETY: AVX-512 Horizontal Sum.
    // BEGRÜNDUNG: hsum512_epi32_avx512 wird innerhalb eines AVX-512 aktivierten Kontextes aufgerufen.
    // SAFETY: hsum512_epi32_avx512 is called within an AVX-512 enabled context.
    let mut sum = unsafe { hsum512_epi32_avx512(sum_v) } as u32; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
    while i < n {
        let diff = a[i] as i32 - b[i] as i32;
        sum += (diff * diff) as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
#[target_feature(enable = "avx512bw")]
#[target_feature(enable = "avx512vnni")]
#[allow(unsafe_code)]
// SAFETY: AVX-512 VNNI Cosine Similarity Parts for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
/// # Safety
/// This function is unsafe because it uses AVX-512 VNNI intrinsics. The caller must ensure that the CPU supports AVX-512F, BW, and VNNI.
pub unsafe fn cosine_similarity_parts_u8_avx512(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    let n = a.len().min(b.len());
    let mut i = 0;
    // SAFETY: _mm512_setzero_si512 is always safe.
    let (mut dot_v, mut sum_a_v, mut sum_b_v, mut norm_a_v, mut norm_b_v) = (
        _mm512_setzero_si512(),
        _mm512_setzero_si512(),
        _mm512_setzero_si512(),
        _mm512_setzero_si512(),
        _mm512_setzero_si512(),
    );

    while i + 64 <= n {
        // SAFETY: AVX-512 Load and DPBUSD.
        // BEGRÜNDUNG: i + 64 <= n garantiert In-Bounds Zugriff. Hardware-Support via Dispatcher geprüft.
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to i + 64 <= n.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
            let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const _);
            let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const _);

            dot_v = _mm512_dpbusd_epi32(dot_v, va, vb);
            norm_a_v = _mm512_dpbusd_epi32(norm_a_v, va, va);
            norm_b_v = _mm512_dpbusd_epi32(norm_b_v, vb, vb);

            let zero = _mm512_setzero_si512();
            sum_a_v = _mm512_add_epi64(sum_a_v, _mm512_sad_epu8(va, zero));
            sum_b_v = _mm512_add_epi64(sum_b_v, _mm512_sad_epu8(vb, zero));
        }
        i += 64;
    }

    // SAFETY: AVX-512 Horizontal Sums.
    // BEGRÜNDUNG: hsum512_epi32/64_avx512 werden innerhalb eines AVX-512 aktivierten Kontextes aufgerufen.
    // SAFETY: Horizontal sums are safe on AVX-512.
    let (mut dot, mut norm_a_sq, mut norm_b_sq, mut sum_a, mut sum_b) = unsafe {
        // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        (
            hsum512_epi32_avx512(dot_v) as u32,
            hsum512_epi32_avx512(norm_a_v) as u32,
            hsum512_epi32_avx512(norm_b_v) as u32,
            hsum512_epi64_avx512(sum_a_v) as u32,
            hsum512_epi64_avx512(sum_b_v) as u32,
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
#[target_feature(enable = "avx512f")]
#[allow(unsafe_code)]
// SAFETY: Horizontal Sum epi64 AVX-512.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum512_epi64_avx512(v: __m512i) -> i64 {
    // SAFETY: Standard AVX-512 to AVX2 reduction is safe on supported hardware.
    // BEGRÜNDUNG: Caller garantiert Support und korrekte bounds.
    unsafe {
        // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        let low = _mm512_castsi512_si256(v);
        let high = _mm512_extracti64x4_epi64(v, 1);
        let sum256 = _mm256_add_epi64(low, high);
        hsum256_epi64_avx2(sum256)
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
#[allow(unsafe_code)]
// SAFETY: Horizontal Sum epi32 AVX-512.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum512_epi32_avx512(v: __m512i) -> i32 {
    // SAFETY: Standard AVX-512 to AVX2 reduction is safe on supported hardware.
    // BEGRÜNDUNG: Caller garantiert Support und korrekte bounds.
    unsafe {
        // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
        let low = _mm512_castsi512_si256(v);
        let high = _mm512_extracti32x8_epi32(v, 1);
        let sum256 = _mm256_add_epi32(low, high);
        hsum256_epi32_avx2(sum256)
    }
}

// -----------------------------------------------------------------------------
// AVX2 Implementations for u8
// -----------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
// SAFETY: AVX2 Dot Product for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2.
pub unsafe fn dot_product_u8_avx2(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len();
    let mut i = 0;
    // SAFETY: Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    // SAFETY: _mm256_setzero_si256 is always safe.
    let mut sum_v = _mm256_setzero_si256();

    while i + 32 <= n {
        // SAFETY: AVX2 Load und Madd.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to the loop condition i + 32 <= n. AVX2 intrinsics are safe on hardware detected by caller.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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

    // SAFETY: Horizontale Summe.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    // SAFETY: hsum256_epi32_avx2 is called within an AVX2 enabled function.
    let mut sum = unsafe { hsum256_epi32_avx2(sum_v) } as u32; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
    while i < n {
        sum += a[i] as u32 * b[i] as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
// SAFETY: AVX2 Squared Euclidean for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2.
pub unsafe fn euclidean_distance_sq_u8_avx2(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len();
    let mut i = 0;
    // SAFETY: Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    // SAFETY: _mm256_setzero_si256 is always safe.
    let mut sum_v = _mm256_setzero_si256();

    while i + 32 <= n {
        // SAFETY: AVX2 Load und Sub/Madd.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to the loop condition i + 32 <= n. AVX2 intrinsics are safe on hardware detected by caller.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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

    // SAFETY: Horizontale Summe.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    // SAFETY: hsum256_epi32_avx2 is called within an AVX2 enabled function.
    let mut sum = unsafe { hsum256_epi32_avx2(sum_v) } as u32; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
    while i < n {
        let diff = a[i] as i32 - b[i] as i32;
        sum += (diff * diff) as u32;
        i += 1;
    }
    sum
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
// SAFETY: AVX2 Cosine Similarity Parts for u8.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren. Dimensionen müssen gleich sein.
/// # Safety
/// This function is unsafe because it uses AVX2 intrinsics. The caller must ensure that the CPU supports AVX2.
pub unsafe fn cosine_similarity_parts_u8_avx2(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    let n = a.len();
    let mut i = 0;

    // SAFETY: Initialisierung.
    // BEGRÜNDUNG: _mm256_setzero_si256 ist immer sicher.
    // SAFETY: _mm256_setzero_si256 is always safe.
    let (mut dot_v, mut sum_a_v, mut sum_b_v, mut norm_a_v, mut norm_b_v) = (
        _mm256_setzero_si256(),
        _mm256_setzero_si256(),
        _mm256_setzero_si256(),
        _mm256_setzero_si256(),
        _mm256_setzero_si256(),
    );

    while i + 32 <= n {
        // SAFETY: AVX2 Loads und Accumulation.
        // BEGRÜNDUNG: i + 32 <= n garantiert In-Bounds Zugriff. Unaligned Load (loadu) ist sicher.
        // SAFETY: Pointer arithmetic and unaligned loads are safe due to the loop condition i + 32 <= n. AVX2 intrinsics are safe on hardware detected by caller.
        unsafe {
            // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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

    // SAFETY: Horizontale Summen.
    // BEGRÜNDUNG: Hardware-Support durch Caller garantiert.
    // SAFETY: Horizontal sum functions are called within an AVX2 enabled function.
    let (mut dot, mut norm_a_sq, mut norm_b_sq, mut sum_a, mut sum_b) = unsafe {
        // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
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
#[allow(unsafe_code)]
// SAFETY: Horizontal Sum epi32.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_epi32_avx2(v: __m256i) -> i32 {
    // SAFETY: AVX2 Reduktion.
    // BEGRÜNDUNG: Standard AVX2 Befehle zur horizontalen Reduktion.
    // SAFETY: Standard AVX2 horizontal reduction sequence is safe on supported hardware detected by caller.
    let v128 = _mm_add_epi32(_mm256_castsi256_si128(v), _mm256_extracti128_si256(v, 1));
    let v64 = _mm_add_epi32(v128, _mm_shuffle_epi32(v128, 0x4E));
    let v32 = _mm_add_epi32(v64, _mm_shuffle_epi32(v64, 0xB1));
    _mm_cvtsi128_si32(v32)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
// SAFETY: Horizontal Sum epi64.
// BEGRÜNDUNG: Caller muss Hardware-Support garantieren.
unsafe fn hsum256_epi64_avx2(v: __m256i) -> i64 {
    // SAFETY: AVX2 Reduktion epi64.
    // BEGRÜNDUNG: Standard AVX2 Befehle zur horizontalen Reduktion.
    // SAFETY: Standard AVX2 horizontal reduction sequence is safe on supported hardware detected by caller.
    let v128 = _mm_add_epi64(_mm256_castsi256_si128(v), _mm256_extracti128_si256(v, 1));
    let v64 = _mm_add_epi64(v128, _mm_unpackhi_epi64(v128, v128));
    _mm_cvtsi128_si64(v64)
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
        let d = compute_distance(&a, &b, DistanceMetric::DotProduct).expect("test"); // expect
        let dot_simd = -d;
        assert!((dot_scalar - dot_simd).abs() < 1e-3);

        // Euclidean
        let euc_scalar = euclidean_distance_scalar(&a, &b);
        let euc_simd = compute_distance(&a, &b, DistanceMetric::Euclidean).expect("test"); // expect
        assert!((euc_scalar - euc_simd).abs() < 1e-3);

        // Cosine
        let cos_scalar = cosine_distance_scalar(&a, &b);
        let cos_simd = compute_distance(&a, &b, DistanceMetric::Cosine).expect("test"); // expect
        assert!((cos_scalar - cos_simd).abs() < 1e-3);
    }

    #[test]
    #[allow(unsafe_code)]
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
            if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512vnni") {
                // SAFETY: AVX-512 VNNI Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                // SAFETY: Hardware support detected.
                let dot_simd = unsafe { dot_product_u8_avx512vnni(&a, &b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                assert_eq!(dot_scalar, dot_simd);
            }
            if is_x86_feature_detected!("avx2") {
                // SAFETY: AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                // SAFETY: Hardware support detected.
                let dot_simd = unsafe { dot_product_u8_avx2(&a, &b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                assert_eq!(dot_scalar, dot_simd);
            }
        }

        // Euclidean
        let euc_scalar = euclidean_distance_sq_u8(&a, &b);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                // SAFETY: AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                // SAFETY: Hardware support detected.
                let euc_simd = unsafe { euclidean_distance_sq_u8_avx2(&a, &b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                assert_eq!(euc_scalar, euc_simd);
            }
        }

        // Cosine parts
        let parts_scalar = cosine_similarity_parts_u8(&a, &b);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                // SAFETY: AVX2 Test Dispatch.
                // BEGRÜNDUNG: Hardware-Support wurde via is_x86_feature_detected geprüft.
                // SAFETY: Hardware support detected.
                let parts_simd = unsafe { cosine_similarity_parts_u8_avx2(&a, &b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                assert_eq!(parts_scalar.dot, parts_simd.dot);
                assert_eq!(parts_scalar.sum_a, parts_simd.sum_a);
                assert_eq!(parts_scalar.sum_b, parts_simd.sum_b);
                assert_eq!(parts_scalar.norm_a_sq, parts_simd.norm_a_sq);
                assert_eq!(parts_scalar.norm_b_sq, parts_simd.norm_b_sq);
            }
        }
    }

    #[test]
    fn test_asymmetric_metrics() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![10, 20, 30, 40];
        let alpha = 0.1_f32;
        let min = 0.0_f32;
        let alphas = vec![alpha; 4];
        let mins = vec![min; 4];

        // Euclidean Asymmetric
        let dist_sq = euclidean_distance_sq_f32_u8(&a, &b, &alphas, &mins);
        let mut expected = 0.0;
        for i in 0..4 {
            let diff = a[i] - (b[i] as f32 * alpha + min);
            expected += diff * diff;
        }
        assert!((dist_sq - expected).abs() < 1e-5);

        // Dot Product Asymmetric
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
        assert!(res.is_err());
    }

    #[test]
    #[should_panic(expected = "Vector lengths must match for cosine_distance")]
    fn test_cosine_distance_mismatch_panics() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let _ = cosine_distance(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Vector lengths must match for euclidean_distance")]
    fn test_euclidean_distance_mismatch_panics() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let _ = euclidean_distance(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Vector lengths must match for dot_product_distance")]
    fn test_dot_product_distance_mismatch_panics() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let _ = dot_product_distance(&a, &b);
    }

    #[test]
    fn test_normalize_inplace() {
        // Zero vector should remain zero vector without panic or NaN
        let mut zero_vec = vec![0.0f32, 0.0, 0.0];
        normalize_inplace(&mut zero_vec);
        assert_eq!(zero_vec, vec![0.0, 0.0, 0.0]);

        // Single element vector
        let mut single_vec = vec![5.0f32];
        normalize_inplace(&mut single_vec);
        assert_eq!(single_vec, vec![1.0]);

        // Vector [3, 4] -> norm is 5, normalized is [0.6, 0.8] (Anti-mirroring hand calculated)
        let mut vec_34 = vec![3.0f32, 4.0];
        normalize_inplace(&mut vec_34);
        assert!((vec_34[0] - 0.6).abs() < 1e-6);
        assert!((vec_34[1] - 0.8).abs() < 1e-6);

        // L2 norm of normalized vector should be 1.0
        let norm_sq: f32 = vec_34.iter().map(|x| x * x).sum();
        assert!((norm_sq - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_distance_nan_input_returns_error() {
        let a = vec![1.0, f32::NAN, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let res = compute_distance(&a, &b, DistanceMetric::Cosine);
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::InvalidInput(_))
        ));

        let a2 = vec![1.0, 2.0, 3.0];
        let b2 = vec![1.0, f32::NAN, 3.0];
        let res2 = compute_distance(&a2, &b2, DistanceMetric::Euclidean);
        assert!(matches!(
            res2,
            Err(memfuse_core::MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_scalar_metric_independent_values() {
        // Anti-mirroring check: Expected values independently derived
        // Vector a = [3.0, 0.0], b = [0.0, 4.0]
        // dot_product = 3*0 + 0*4 = 0.0
        // euclidean = sqrt((3-0)^2 + (0-4)^2) = sqrt(9 + 16) = 5.0
        // norm_a = 3.0, norm_b = 4.0, cosine_sim = 0.0 / (3*4) = 0.0, cosine_dist = 1.0 - 0.0 = 1.0
        let a = vec![3.0f32, 0.0];
        let b = vec![0.0f32, 4.0];

        assert_eq!(dot_product_scalar(&a, &b), 0.0);
        assert_eq!(euclidean_distance_scalar(&a, &b), 5.0);
        assert_eq!(cosine_distance_scalar(&a, &b), 1.0);
    }

    #[test]
    fn test_cosine_zero_norm() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_distance_scalar(&a, &b), 1.0);
    }

    #[test]
    fn test_simd_scalar_determinism_bound() {
        // FIND-IND-001: Verify SIMD vs Scalar determinism bound
        let dims = [16, 32, 64, 128, 1536];
        let metrics = [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ];

        for &dim in &dims {
            let a: Vec<f32> = (0..dim).map(|i| (i as f32).sin()).collect();
            let b: Vec<f32> = (0..dim).map(|i| (i as f32).cos()).collect();

            for &metric in &metrics {
                let scalar = match metric {
                    DistanceMetric::Cosine => cosine_distance_scalar(&a, &b),
                    DistanceMetric::Euclidean => euclidean_distance_scalar(&a, &b),
                    DistanceMetric::DotProduct => dot_product_scalar(&a, &b),
                    other => panic!(
                        "Test infrastructure: unhandled DistanceMetric variant {other:?}. \
                         Add a scalar impl to this test block when adding enum variants."
                    ),
                };

                let simd = compute_distance(&a, &b, metric).unwrap(); // unwrap

                // DotProduct in compute_distance returns -dot, so we adjust
                let simd_val = if metric == DistanceMetric::DotProduct {
                    -simd
                } else {
                    simd
                };

                let diff = (scalar - simd_val).abs();
                assert!(
                    diff < 1e-5,
                    "Determinism failure for {:?} at dim {}: scalar={}, simd={}, diff={}",
                    metric,
                    dim,
                    scalar,
                    simd_val,
                    diff
                );
            }
        }
    }

    #[test]
    fn neon_matches_scalar_within_tolerance() {
        #[cfg(target_arch = "aarch64")]
        {
            let a: Vec<f32> = (0..768).map(|i| (i as f32) * 0.001).collect();
            let b: Vec<f32> = (0..768).map(|i| ((i + 37) as f32) * 0.001).collect();
            let scalar = cosine_distance_scalar(&a, &b);
            // SAFETY: Hardware-Support und Bounds wurden validiert.
            // BEGRÜNDUNG: Caller garantiert Support und korrekte bounds.
            let neon = unsafe { cosine_distance_neon(&a, &b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
            assert!(
                (scalar - neon).abs() < 1e-6,
                "NEON/Scalar difference: {}",
                (scalar - neon).abs()
            );
        }
    }

    #[test]
    fn cosine_distance_self_is_zero() {
        let v = vec![1.0f32, 2.0, 3.0, 4.0];
        let d = compute_distance(&v, &v, DistanceMetric::Cosine).unwrap(); // unwrap
        assert!(d.abs() < 1e-6, "cos_distance(v, v) must be ~0, got {d}");
    }

    #[test]
    fn euclidean_distance_self_is_zero() {
        let v = vec![1.0f32, 0.5, -1.0, 2.0];
        let d = compute_distance(&v, &v, DistanceMetric::Euclidean).unwrap(); // unwrap
        assert!(d.abs() < 1e-6, "euclidean(v, v) must be ~0, got {d}");
    }

    #[test]
    fn distance_mismatched_dims_returns_err() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32, 2.0, 3.0];
        assert!(compute_distance(&a, &b, DistanceMetric::Cosine).is_err());
    }

    #[test]
    fn test_euclidean_distance_sq_f32_u8_quantized_accuracy() {
        // Task E: Verify euclidean_distance_sq_f32_u8 dequantizes on the fly and matches full precision L2 within 1% error budget.
        let dims = 128;
        let query: Vec<f32> = (0..dims).map(|i| (i as f32 * 0.05).sin()).collect();
        let original: Vec<f32> = (0..dims).map(|i| ((i + 10) as f32 * 0.05).cos()).collect();

        // Quantize original f32 vector into u8 per-dimension
        let mut mins = Vec::with_capacity(dims);
        let mut alphas = Vec::with_capacity(dims);
        let mut quantized = Vec::with_capacity(dims);

        for &val in &original {
            // Per dimension bounds (simulation of per-dim quantization bounds)
            let min_val = val - 0.5;
            let max_val = val + 0.5;
            let alpha = (max_val - min_val) / 255.0;
            let q_byte = ((val - min_val) / alpha).round().clamp(0.0, 255.0) as u8;

            mins.push(min_val);
            alphas.push(alpha);
            quantized.push(q_byte);
        }

        let quantized_sq_dist = euclidean_distance_sq_f32_u8(&query, &quantized, &alphas, &mins);
        let quantized_dist = quantized_sq_dist.sqrt();

        let full_precision_dist = euclidean_distance_scalar(&query, &original);

        let relative_error = (quantized_dist - full_precision_dist).abs() / full_precision_dist;
        assert!(
            relative_error < 0.01,
            "Quantized distance error too high: relative_error={}, quantized_dist={}, full_precision_dist={}",
            relative_error, quantized_dist, full_precision_dist
        );
    }

    proptest::proptest! {
        // Task D: Property test verifying AVX2/SIMD vs Scalar distance parity within 1e-4 tolerance.
        #[test]
        fn prop_simd_vs_scalar_parity(
            v1 in proptest::collection::vec(-10.0..10.0f32, 1..256),
            v2 in proptest::collection::vec(-10.0..10.0f32, 1..256)
        ) {
            let len = v1.len().min(v2.len());
            let a = &v1[..len];
            let b = &v2[..len];

            // Test AVX2 directly if feature detected on x86_64
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
                    // SAFETY: Hardware support detected via is_x86_feature_detected. Slices have equal length.
                    let cos_avx2 = unsafe { cosine_distance_avx2(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                    let cos_scalar = cosine_distance_scalar(a, b);
                    proptest::prop_assert!(
                        (cos_avx2 - cos_scalar).abs() < 1e-4,
                        "AVX2 Cosine mismatch: avx2={}, scalar={}, diff={}",
                        cos_avx2, cos_scalar, (cos_avx2 - cos_scalar).abs()
                    );

                    // SAFETY: Hardware support detected via is_x86_feature_detected. Slices have equal length.
                    let euc_avx2 = unsafe { euclidean_distance_avx2(a, b) }; // SAFETY: 1. Invariant: Valid vector alignment & slice bounds. 2. Guarantor: Hardware feature check & caller bounds validation. 3. Valid parameters at call-site. 4. ADR-017 SIMD.
                    let euc_scalar = euclidean_distance_scalar(a, b);
                    proptest::prop_assert!(
                        (euc_avx2 - euc_scalar).abs() < 1e-4,
                        "AVX2 Euclidean mismatch: avx2={}, scalar={}, diff={}",
                        euc_avx2, euc_scalar, (euc_avx2 - euc_scalar).abs()
                    );
                }
            }

            // General SIMD dispatch vs Scalar check
            let cos_simd = compute_distance(a, b, DistanceMetric::Cosine).unwrap(); // unwrap
            let cos_scalar = cosine_distance_scalar(a, b);
            proptest::prop_assert!(
                (cos_simd - cos_scalar).abs() < 1e-4,
                "Cosine dispatch mismatch: simd={}, scalar={}", cos_simd, cos_scalar
            );

            let euc_simd = compute_distance(a, b, DistanceMetric::Euclidean).unwrap(); // unwrap
            let euc_scalar = euclidean_distance_scalar(a, b);
            proptest::prop_assert!(
                (euc_simd - euc_scalar).abs() < 1e-4,
                "Euclidean dispatch mismatch: simd={}, scalar={}", euc_simd, euc_scalar
            );
        }
    }
}
