//! MemFuse Index — HNSW vector index with SIMD distance computation.

#![feature(portable_simd)]

pub mod csr;
pub mod distance;
pub mod hnsw;

pub use csr::CsrGraph;
pub use hnsw::{HnswConfig, HnswIndex};
