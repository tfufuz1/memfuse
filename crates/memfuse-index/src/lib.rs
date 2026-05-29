//! MemFuse Index — HNSW vector index with SIMD distance computation.
// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).

// ANCHOR:REFACTOR:WP-0.0-STABLESIMD — Remove nightly portable_simd
// ANCHOR:REFACTOR:WP-0.0-STABLESIMD — Remove nightly portable_simd
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:@JULES-03 DATE:2026-05-27 STATUS:READY
// TEST: cargo +stable check -p memfuse-index
// DONE: #![feature(portable_simd)] ist entfernt und distance.rs nutzt stabiles Rust.
// SUCCESSOR: @JULES-13 — "SIMD ist stabil. Tech-Debt Audit fortsetzen."
// ANCHOR:AUDIT:SEC-002 — deny(unsafe_code) statt forbid(unsafe_code)
// BEGRÜNDUNG: SIMD-Intrinsics in distance.rs benötigen unsafe für Performance.
#![deny(unsafe_code)]
// TODO(FIND-IDX-001): SIMD Safety - Add #![forbid(unsafe_code)] exception safely and audit missing Safety comments in distance.rs.

pub mod diskann;
pub mod distance;
pub mod hnsw;
pub mod persistence;
pub mod quantize;

pub use hnsw::{HnswConfig, HnswIndex};
pub use memfuse_graph::CsrGraph;
pub use persistence::{HnswHeader, MmapIndex};
