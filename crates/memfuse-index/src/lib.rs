//! MemFuse Index — HNSW vector index with SIMD distance computation.
// INVARIANT: Vector Engine (Triebwerk — Layer 1).
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).

// ANCHOR:REFACTOR:WP-0.0-STABLESIMD — Remove nightly portable_simd
// ANCHOR:REFACTOR:WP-0.0-STABLESIMD — Remove nightly portable_simd
// TEST: cargo +stable check -p memfuse-index
// DONE: #![feature(portable_simd)] ist entfernt und distance.rs nutzt stabiles Rust.
// INTENT: deny(unsafe_code) statt forbid(unsafe_code)
// BEGRÜNDUNG: SIMD-Intrinsics in distance.rs benötigen unsafe für Performance.
#![deny(unsafe_code)]

#[cfg(feature = "experimental-diskann")]
pub mod diskann;
pub mod distance;
pub mod hnsw;
pub mod persistence;
pub mod quantize;

pub use hnsw::{HnswConfig, HnswIndex, RebuildStatus};
#[cfg(feature = "graph")]
pub use memfuse_graph::CsrGraph;
pub use persistence::{HnswHeader, MmapIndex};
