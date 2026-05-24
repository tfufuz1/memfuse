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

// ANCHOR:INTEGRATION:GRAPH-001 STATUS:DONE AGENT:12 DATE:2026-05-24
// ANCHOR:ARCH:GRAPH-001 — CSR-Graph für 4-Signal Fusion (WP-6.1)
// WP:WP-6.1 PRIO:2 NEEDS:WP-2.1
// STATUS:SCAFFOLD DATE:2026-05-17

#![forbid(unsafe_code)]

pub mod csr;

pub use csr::CsrGraph;
