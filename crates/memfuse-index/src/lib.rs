// ANCHOR:AUDIT:SAOS-024 — forbid(unsafe_code) missing → added deny(unsafe_code)
// BEGRÜNDUNG: Da memfuse-index via distance.rs SIMD intrinsics nutzt, ist forbid(unsafe_code)
// technisch nicht möglich. deny(unsafe_code) wird crate-weit gesetzt, um unkontrolliertes unsafe
// zu verhindern, während distance.rs lokal via allow(unsafe_code) für SIMD freigeschaltet wird.
// AGENT:13 DATE:2026-05-10 STATUS:DONE
#![deny(unsafe_code)]
// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).
//! MemFuse Index — HNSW vector index with SIMD distance computation.

#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
