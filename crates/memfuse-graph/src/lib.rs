//! MemFuse Graph — CSR-Graph for Entity-Relation Traversal & Session DAG.
//!
//! This crate provides the graph signal (Signal 3) for the 4-Signal Fusion
//! architecture. It implements a Compressed Sparse Row (CSR) graph for
//! memory-efficient BFS traversal with score-decay, and a Session-DAG for
//! agent state branching (Grok pattern).
//!
//! # Architecture Role (Triebwerk — Layer 1)
//!
//! Peer to `memfuse-store` and `memfuse-index`. Provides the `GraphIndex`
//! trait implementation via [`CsrGraph`] and conversation branching via [`SessionBranchTree`].

// INVARIANT: CSR-Graph für 4-Signal Fusion (WP-6.1)

#![forbid(unsafe_code)]

pub mod csr;
pub mod ppr;
pub mod session_dag;

pub use csr::CsrGraph;
pub use session_dag::{AgentStateNode, DagEdge, NodeIdx, SessionBranchTree};
