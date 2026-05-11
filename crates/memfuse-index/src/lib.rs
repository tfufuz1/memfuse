// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).
// ANCHOR:DEBT:SAOS-024 — forbid(unsafe_code) replaced by deny/allow for SIMD
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:13 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
//! MemFuse Index — HNSW vector index with SIMD distance computation.

#![deny(unsafe_code)]
#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
