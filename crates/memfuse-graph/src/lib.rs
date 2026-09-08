//! MemFuse Graph — CSR-Graph for Entity-Relation Traversal & Session DAG.
//!
//! This crate provides the graph signal (Signal 3) for the 4-Signal Fusion
//! architecture. It implements a Compressed Sparse Row (CSR) graph for
//! memory-efficient BFS traversal with score-decay, and a Session-DAG for
//! agent state branching (MemFuse Session-DAG Pattern).
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

pub mod cascade;
pub mod community;
pub mod consistency_enforcement;
pub mod csr;
#[cfg(feature = "edge-reinforcement-learning")]
pub mod edge_reinforcement;
pub mod path_rag;
#[cfg(feature = "graph-connectivity-health")]
pub mod percolation;
pub mod ppr;
pub mod provenance;
pub mod session_dag;

pub use cascade::{cascade_invalidate_edges_for_superseded_doc, CascadeInvalidationReport};
pub use community::{detect_communities, CommunityAssignment, CommunityDetectionConfig};
pub use consistency_enforcement::{
    ConflictPattern, ConsistencyEnforcer, ContradictionDetector, EdgeAssertion, EdgeId,
    ExactPredicateConflictDetector,
};
pub use csr::CsrGraph;
#[cfg(feature = "edge-reinforcement-learning")]
pub use edge_reinforcement::{
    apply_cooccurrence_reinforcement, apply_traversal_reinforcement, apply_weight_normalization,
    compute_edge_weight, EdgeReinforcementConfig,
};
pub use path_rag::{EntityId, GraphPath, PathGraph, PathRAGEngine};
#[cfg(feature = "graph-connectivity-health")]
pub use percolation::{
    compute_percolation_health, find_rebonding_candidates, should_trigger_rebonding,
    PercolationConfig,
};
pub use ppr::PprContext;
pub use provenance::{DocEdgeIndex, EdgeProvenance};
pub use session_dag::{
    AgentStateNode, DagEdge, NodeIdx, NodesGuard, NodesWriteGuard, SessionBranchTree,
};
