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
//!
//! # Concurrency and Lock Discipline
//!
//! - Lock Hierarchy in [`CsrGraph`]: Locks are managed internally via `parking_lot::RwLock`.
//!   `inner` holds graph topology (offsets, targets, weights, maps, pending edges). Lock scopes
//!   are minimal and strictly contained within single methods without holding locks across
//!   `.await` points.
//! - Lock Hierarchy in [`SessionBranchTree`]: Synchronizes `nodes`, `edges`, and `active_head`
//!   independently using `parking_lot::RwLock`. When acquiring multiple locks concurrently,
//!   `nodes` MUST be acquired before `edges` or `active_head` to prevent deadlocks. No locks are
//!   held across `.await` storage calls.

// INVARIANT: CSR-Graph für 4-Signal Fusion (WP-6.1)

#![forbid(unsafe_code)]

pub mod community;
pub mod csr;
pub mod path_rag;
pub mod ppr;
pub mod session_dag;

pub use community::{detect_communities, CommunityAssignment, CommunityDetectionConfig};
pub use csr::CsrGraph;
pub use path_rag::{EntityId, GraphPath, PathGraph, PathRAGEngine};
pub use ppr::PprContext;
pub use session_dag::{
    AgentStateNode, DagEdge, NodeIdx, NodesGuard, NodesWriteGuard, SessionBranchTree,
};
