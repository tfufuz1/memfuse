//! MemFuse Index — HNSW vector index with SIMD distance computation.
// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).

#![feature(portable_simd)]
// ANCHOR:SEC:UNSAFE-POLICY — Warum deny statt forbid?
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:10 DATE:2026-05-16 STATUS:DONE
// BEGRÜNDUNG: memfuse-index benötigt hardware-spezifische SIMD-Intrinsics (AVX2, AVX-512)
// für maximale Such-Performance in `distance.rs`. Diese erfordern `unsafe`.
// `deny(unsafe_code)` stellt sicher, dass kein ANDERES Modul im Crate `unsafe` verwenden darf.
#![deny(unsafe_code)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
