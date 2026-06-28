//! MemFuse Graph — CSR-Graph for Entity-Relation Traversal.
//!
//! This crate provides the graph signal (Signal 3) for the 4-Signal Fusion
//! architecture. It implements a Compressed Sparse Row (CSR) graph for
//! memory-efficient BFS traversal with score-decay.
//!
//! # Architecture Role (Triebwerk — Layer 1)
//!
//! Peer to `memfuse-store` and `memfuse-index`. Provides the `GraphIndex`
//! trait implementation via [`CsrGraph`].

// INVARIANT: CSR-Graph für 4-Signal Fusion (WP-6.1)

#![forbid(unsafe_code)]

pub mod csr;

pub use csr::CsrGraph;
