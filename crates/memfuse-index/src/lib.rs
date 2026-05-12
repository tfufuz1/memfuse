// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).
//! MemFuse Index — HNSW vector index with SIMD distance computation.
//!
//! This crate uses `unsafe` code primarily in the `distance` module for hardware-specific
//! SIMD optimizations (AVX2, AVX-512). All other modules are subject to strict `forbid(unsafe_code)`.

#![allow(unsafe_code)]
#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
