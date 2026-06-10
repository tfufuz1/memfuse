# memfuse-index

This crate provides the core HNSW vector index implementation along with hardware-accelerated, highly optimized SIMD distance mathematical routines.

## Zero-Unsafe Doctrine Exceptions (SIMD Safety)

In compliance with the MemFuse Sovereign Core Doctrine, all crates mandate `#![forbid(unsafe_code)]` with exactly **one exception**: `memfuse-index`. The performance demands of vector clustering and graph traversal require hardware-specific instructions (AVX2, AVX-512, FMA) which are inherently `unsafe` in Rust.

To guarantee cryptographic isolation and memory crash-safety inside this module:
1. `compute_distance` acts as an absolute safety barrier. It rigorously enforces equal dimensionality limits (`a.len() == b.len()`) to guarantee `in-bounds` mapping prior to calling unchecked vectorized memory offsets.
2. Dynamic feature detection (`is_x86_feature_detected!`) routes the control flow only when intrinsic instructions are undeniably available.
3. Every internal `unsafe` memory manipulation is mapped with robust `ANCHOR:SAFETY:` blocks that prove safe slice-bounds and pointer-arithmetic (verified by `cargo check`).
4. Native architecture scalar fallbacks (`compute_distance_scalar`) are fully implemented in Safe Rust to ensure standard evaluation without any panics.
