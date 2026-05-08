// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).
//! MemFuse Index — HNSW vector index with SIMD distance computation.

#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
