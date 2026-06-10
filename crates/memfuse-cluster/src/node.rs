//! # Node definition and management
use crate::{network::Network, storage::Store, Node, NodeId, TypeConfig};
use memfuse_store::lsm::LsmStorage;
use openraft::{Config, Raft};
use std::sync::Arc;

#[allow(missing_docs)]
pub type MemFuseRaft = Raft<TypeConfig>;

/// Sets up a Raft node with the given ID, node info, and storage engine.
///
/// Creates separate `Store` instances for log storage and state machine,
/// both backed by the same LSM engine. This follows the openraft v0.9
/// storage-v2 separation of concerns.
pub async fn setup_raft(
    node_id: NodeId,
    _node: Node,
    lsm: Arc<LsmStorage>,
) -> memfuse_core::Result<MemFuseRaft> {
    let config = Arc::new(Config::default().validate().map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!("Invalid Raft config: {}", e))
    })?);

    let network = Network;

    // Separate instances for log storage and state machine, sharing the LSM engine.
    let log_store = Store::new(Arc::clone(&lsm));
    let state_machine = Store::new(lsm);

    Raft::new(node_id, config, network, log_store, state_machine)
        .await
        .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Raft init error: {}", e)))
}
