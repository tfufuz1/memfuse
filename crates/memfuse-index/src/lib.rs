// ANCHOR:ARCH:INDEX-001 — Vector Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// KERNKOMPONENTEN: HNSW (Graph-basierte ANN) + CSR Graph (Relationen) + SIMD-Distanz.
// INVARIANTE: HNSW-Graphen liegen exklusiv im RAM. Disk-Storage erfolgt über memfuse-store (via LsmStorage).
//! # MemFuse Index — Vector Similarity Search Engine
//!
//! This crate provides the indexing and search core for MemFuse, focusing on
//! high-performance vector retrieval.
//!
//! ## Key Components
//! - **HNSW**: Hierarchical Navigable Small World graph for ANN search.
//! - **SIMD Distance**: Hardware-accelerated distance metrics (Cosine, L2, Dot).
//! - **Quantization**: Memory-efficient scalar quantization (SQ8).
//! - **CSR Graph**: Compressed Sparse Row graph representation for optimized traversal.

#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;
pub mod quantize;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
