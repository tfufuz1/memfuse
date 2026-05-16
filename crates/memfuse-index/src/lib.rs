//! MemFuse Index — HNSW vector index with SIMD distance computation.
// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).

#![feature(portable_simd)]
// ANCHOR:AUDIT:SEC-002 — deny(unsafe_code) statt forbid(unsafe_code)
// BEGRÜNDUNG: SIMD-Intrinsics in distance.rs benötigen unsafe für Performance.
#![deny(unsafe_code)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;
pub mod diskann;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
pub use diskann::{DiskHnsw, DiskHnswConfig};
