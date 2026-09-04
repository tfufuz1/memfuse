// FILE-CONTEXT
// ZWECK: Layer-1 Vektor-Such- & Index-Engine mit HNSW, DiskANN, Quantisierung und SIMD.
// INVARIANTEN: Einhaltung der DAG Layer-1 Grenzen (keine Aufwärts-Imports); Zero-Panic Invariante.
// NICHT-OFFENSICHTLICH: Hardware-Dispatch wählt zur Laufzeit die optimalen SIMD intrinsics (AVX-512 > AVX2 > Skalar).
// HOTSPOTS: lib.rs (Modul-Deklarationen)
// STAND: TS:2026-08-30T18:53:53Z (SESSION: 37b1d991)

//! MemFuse Index — HNSW vector index with SIMD distance computation.
// INVARIANT: Vector Engine (Triebwerk — Layer 1).
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).

// ANCHOR[REFACTOR:WP-0.0-STABLESIMD] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Remove nightly portable_simd
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

#[cfg(feature = "experimental-diskann")]
pub use diskann::{DiskAnnConfig, DiskAnnIndex};
pub use hnsw::{HnswConfig, HnswIndex, RebuildStatus};
#[cfg(feature = "graph")]
pub use memfuse_graph::CsrGraph;
pub use persistence::{HnswHeader, MmapIndex};
pub use quantize::ScalarQuantizer;

// REVIEW-PASS[1/2] STATUS:PASS (ID: TEST:AGT-INDEX-AUDIT-001) (TS: 2026-09-04T11:40:35Z) (SESSION: 1a901c59) PRÜFER-KONTEXT: FRESH
// REVIEW-PASS[2/2] STATUS:PASS (ID: TEST:AGT-INDEX-AUDIT-001) (TS: 2026-09-04T15:28:21Z) (SESSION: 5f69ac44) PRÜFER-KONTEXT: FRESH
