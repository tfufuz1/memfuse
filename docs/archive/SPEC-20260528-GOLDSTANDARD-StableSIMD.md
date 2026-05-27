# SPEC: Goldstandard Stable SIMD
**Date**: 2026-05-28
**Author**: Lead Architect
**Target Agent**: 03 (Index Master)

## context
The project currently relies on `#![feature(portable_simd)]` in `memfuse-index/src/lib.rs` and `distance.rs`. The Sovereign Core Doctrine mandates that MemFuse must compile on `stable` Rust.

## objective
Remove the nightly `#![feature(portable_simd)]` dependency entirely. Rewrite `distance.rs` to use standard auto-vectorized loops or stable scalar math for distance metrics until stable `std::simd` or explicit AVX2 intrinsics (via `cfg(target_feature)`) are implemented.

## instructions
1. Open `crates/memfuse-index/src/lib.rs` and remove `#![feature(portable_simd)]`.
2. Open `crates/memfuse-index/src/distance.rs`.
3. Rip out `std::simd` imports.
4. Implement standard iterator-based scalar loops (e.g. `a.iter().zip(b.iter()).map(|(x, y)| ...).sum()`) or chunked loops for Cosine, Euclidean, and DotProduct. Let LLVM auto-vectorize.
5. If explicit intrinsics are used, ensure they are gated safely via `#[cfg(target_feature = "avx2")]` and `#[cfg(target_feature = "neon")]`.

## verification
1. The project must compile successfully with the stable Rust toolchain (`cargo +stable check`).
2. Run `just triple-test` in `memfuse-index`.
