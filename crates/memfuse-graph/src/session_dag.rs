//! Session-DAG for Agent State Branching (Grok Pattern).
//!
//! Native DAG implementation based on standard library maps and RwLock,
//! keeping memfuse-graph pure-Rust and zero-external-graph-dependency (ADR-004).

// FILE-CONTEXT
// STAND:       2026-08-30T14:35:05Z (SESSION: ab88edae)
// ZWECK:       Session-DAG für Gesprächsverzweigung & Agent-State-Tracking (Grok-Muster)
// INVARIANTEN: Monoton steigende NodeIdx; active_head verweist stets auf existierenden Knoten.
// HOTSPOTS:    branch_from(), save(), load()
// AGENT-NOTIZ: pedantic doc & error hygiene
// SIEHE AUCH:  ADR-004

use memfuse_core::{MemFuseError, Result, StorageEngine, TxId};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Index of a node in the Session-DAG.
pub type NodeIdx = u64;

/// State of an agent step in the conversation DAG.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStateNode {
    pub step_id: NodeIdx,
    /// LLM prompt of this step.
    pub prompt: String,
    /// LLM response of this step.
    pub response: String,
    /// Optional MVCC snapshot reference (TxId at checkpoint creation).
    pub snapshot_tx_id: Option<TxId>,
    /// Compactable tool outputs for this step.
    pub tool_outputs: Vec<String>,
    /// Whether this node has already been compacted.
    pub compacted: bool,
}

/// Directed edge in the DAG: parent -> child.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DagEdge {
    pub parent: NodeIdx,
    pub child: NodeIdx,
    /// Label (e.g. "explore", "reject", "main")
    pub label: String,
}

/// Session-DAG for a single agent conversation tree.
///
/// # Invariants
/// - Directed Acyclic Graph: parent must exist before branch creation
/// - `active_head` always points to an existing node
/// - Node IDs are monotonically increasing (`AtomicU64`)
pub struct SessionBranchTree {
    nodes: RwLock<HashMap<NodeIdx, AgentStateNode>>,
    edges: RwLock<Vec<DagEdge>>,
    active_head: RwLock<NodeIdx>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl SessionBranchTree {
    /// Creates a new DAG with a root node (step_id 0).
    pub fn new(root_prompt: String, root_response: String) -> Self {
        let root = AgentStateNode {
            step_id: 0,
            prompt: root_prompt,
            response: root_response,
            snapshot_tx_id: None,
            tool_outputs: Vec::new(),
            compacted: false,
        };
        let mut nodes = HashMap::new();
        nodes.insert(0, root);

        Self {
            nodes: RwLock::new(nodes),
            edges: RwLock::new(Vec::new()),
            active_head: RwLock::new(0),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Appends a step to the current active head node.
    pub fn append_step(
        &self,
        prompt: String,
        response: String,
        snapshot_tx_id: Option<TxId>,
        tool_outputs: Vec<String>,
        label: &str,
    ) -> Result<NodeIdx> {
        let parent = *self.active_head.read();
        let new_id = self.branch_from(
            parent,
            prompt,
            response,
            snapshot_tx_id,
            tool_outputs,
            label,
        )?;
        // Update head for linear append (intended sequential behavior)
        *self.active_head.write() = new_id;
        Ok(new_id)
    }

    /// Branches from an arbitrary prior node (Grok Branching).
    ///
    /// # Errors
    /// Returns `Err(MemFuseError::InvalidInput)` if `parent_node` does not exist in the DAG.
    pub fn branch_from(
        &self,
        parent_node: NodeIdx,
        prompt: String,
        response: String,
        snapshot_tx_id: Option<TxId>,
        tool_outputs: Vec<String>,
        label: &str,
    ) -> Result<NodeIdx> {
        if !self.nodes.read().contains_key(&parent_node) {
            return Err(MemFuseError::InvalidInput(format!(
                "SessionDAG: node {} nicht gefunden",
                parent_node
            )));
        }

        let new_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let node = AgentStateNode {
            step_id: new_id,
            prompt,
            response,
            snapshot_tx_id,
            tool_outputs,
            compacted: false,
        };

        self.nodes.write().insert(new_id, node);
        self.edges.write().push(DagEdge {
            parent: parent_node,
            child: new_id,
            label: label.to_string(),
        });
        // active_head is updated ONLY via set_active_head() or append_step().
        // branch_from() creates the new node but leaves head management to the caller.
        // This allows parallel branch exploration without fighting over active_head.

        Ok(new_id)
    }

    /// Sets the active head to an existing node.
    pub fn set_active_head(&self, node_idx: NodeIdx) -> Result<()> {
        if self.nodes.read().contains_key(&node_idx) {
            *self.active_head.write() = node_idx;
            Ok(())
        } else {
            Err(MemFuseError::InvalidInput(format!(
                "SessionDAG: node {} nicht gefunden",
                node_idx
            )))
        }
    }

    /// Reconstructs the linear conversation path from root to current active head.
    pub fn path_to_head(&self) -> Vec<AgentStateNode> {
        let nodes = self.nodes.read();
        let edges = self.edges.read();
        let head = *self.active_head.read();

        let mut path = Vec::new();
        let mut current = head;

        loop {
            if let Some(node) = nodes.get(&current) {
                path.push(node.clone());
            }
            if let Some(edge) = edges.iter().rev().find(|e| e.child == current) {
                current = edge.parent;
            } else {
                break;
            }
        }

        path.reverse();
        path
    }

    /// Returns all direct children of a given node.
    pub fn children_of(&self, node_idx: NodeIdx) -> Vec<NodeIdx> {
        self.edges
            .read()
            .iter()
            .filter(|e| e.parent == node_idx)
            .map(|e| e.child)
            .collect()
    }

    /// Returns the active head node index.
    pub fn active_head(&self) -> NodeIdx {
        *self.active_head.read()
    }

    /// Returns the total number of nodes in the DAG.
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Retrieves a reference to a specific node by index if it exists.
    pub fn get_node(&self, node_idx: NodeIdx) -> Option<AgentStateNode> {
        self.nodes.read().get(&node_idx).cloned()
    }

    /// Persists all nodes, edges, active head, and `next_id` of the Session-DAG to the storage engine under the given namespace prefix.
    ///
    /// # Errors
    /// Returns `Err(MemFuseError::Serialization)` if serialization fails, or `Err(MemFuseError::Storage)` on storage I/O errors.
    pub async fn save<S: StorageEngine + ?Sized>(
        &self,
        storage: &S,
        namespace: &str,
        tx: TxId,
    ) -> Result<()> {
        let prefix = format!("__session_dag:{namespace}:");

        let serialized_nodes: Vec<(Vec<u8>, Vec<u8>)> = {
            let nodes = self.nodes.read();
            let mut items = Vec::with_capacity(nodes.len());
            for (node_id, node) in nodes.iter() {
                let key = format!("{prefix}node:{node_id}").into_bytes();
                let val = bincode::serialize(node).map_err(|e| {
                    MemFuseError::Serialization(format!("session dag node serialize: {e}"))
                })?;
                items.push((key, val));
            }
            items
        };

        for (key, val) in serialized_nodes {
            storage.put(tx, &key, &val).await?;
        }

        let edges_val = {
            let edges = self.edges.read();
            bincode::serialize(&*edges).map_err(|e| {
                MemFuseError::Serialization(format!("session dag edges serialize: {e}"))
            })?
        };
        let edges_key = format!("{prefix}edges").into_bytes();
        storage.put(tx, &edges_key, &edges_val).await?;

        let head = *self.active_head.read();
        let next_id = self.next_id.load(std::sync::atomic::Ordering::SeqCst);
        let meta = (head, next_id);
        let meta_key = format!("{prefix}meta").into_bytes();
        let meta_val = bincode::serialize(&meta)
            .map_err(|e| MemFuseError::Serialization(format!("session dag meta serialize: {e}")))?;
        storage.put(tx, &meta_key, &meta_val).await?;

        Ok(())
    }

    /// Loads and reconstructs a `SessionBranchTree` from storage for the specified namespace.
    ///
    /// # Errors
    /// Returns `Err(MemFuseError::NotFound)` if the namespace is missing, or `Err(MemFuseError::Serialization)` on corrupt payload deserialization.
    pub async fn load<S: StorageEngine + ?Sized>(storage: &S, namespace: &str) -> Result<Self> {
        let prefix_str = format!("__session_dag:{namespace}:");
        let prefix_bytes = prefix_str.as_bytes();

        let entries = storage.scan_prefix(prefix_bytes).await?;
        if entries.is_empty() {
            return Err(MemFuseError::NotFound(format!(
                "SessionDAG: namespace '{namespace}' nicht im Storage gefunden"
            )));
        }

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut active_head = 0;
        let mut next_id = 1;

        for (raw_key, raw_val) in entries {
            let key_str = std::str::from_utf8(&raw_key)
                .map_err(|e| MemFuseError::Internal(format!("session dag key UTF-8: {e}")))?;

            if key_str.ends_with("edges") {
                edges = bincode::deserialize(&raw_val).map_err(|e| {
                    MemFuseError::Serialization(format!("session dag edges deserialize: {e}"))
                })?;
            } else if key_str.ends_with("meta") {
                let (h, n): (NodeIdx, u64) = bincode::deserialize(&raw_val).map_err(|e| {
                    MemFuseError::Serialization(format!("session dag meta deserialize: {e}"))
                })?;
                active_head = h;
                next_id = n;
            } else if key_str.contains(":node:") {
                let node: AgentStateNode = bincode::deserialize(&raw_val).map_err(|e| {
                    MemFuseError::Serialization(format!("session dag node deserialize: {e}"))
                })?;
                nodes.insert(node.step_id, node);
            }
        }

        Ok(Self {
            nodes: RwLock::new(nodes),
            edges: RwLock::new(edges),
            active_head: RwLock::new(active_head),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(next_id)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_root_creation() {
        let dag = SessionBranchTree::new("Hello".into(), "Hi there!".into());
        assert_eq!(dag.node_count(), 1);
        assert_eq!(dag.active_head(), 0);

        let path = dag.path_to_head();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].step_id, 0);
        assert_eq!(path[0].prompt, "Hello");
        assert_eq!(path[0].response, "Hi there!");
        assert_eq!(path[0].snapshot_tx_id, None);
        assert!(!path[0].compacted);
    }

    #[test]
    fn test_linear_append() {
        let dag = SessionBranchTree::new("Root Prompt".into(), "Root Resp".into());

        let step1 = dag
            .append_step(
                "Prompt 1".into(),
                "Resp 1".into(),
                Some(TxId::new(10)),
                vec!["tool_out_1".into()],
                "main",
            )
            .unwrap(); // unwrap
        assert_eq!(step1, 1);
        assert_eq!(dag.active_head(), 1);

        let step2 = dag
            .append_step("Prompt 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap(); // unwrap
        assert_eq!(step2, 2);
        assert_eq!(dag.active_head(), 2);
        assert_eq!(dag.node_count(), 3);

        let path = dag.path_to_head();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].step_id, 0);
        assert_eq!(path[1].step_id, 1);
        assert_eq!(path[1].snapshot_tx_id, Some(TxId::new(10)));
        assert_eq!(path[2].step_id, 2);
    }

    #[test]
    fn test_grok_branching() {
        let dag = SessionBranchTree::new("Root".into(), "Root Resp".into());
        let step1 = dag
            .append_step("Step 1".into(), "Resp 1".into(), None, vec![], "main")
            .unwrap(); // unwrap
        let step2 = dag
            .append_step("Step 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap(); // unwrap
        assert_eq!(step2, 2);

        // Branch off from step1 (Grok branching)
        let branch_step = dag
            .branch_from(
                step1,
                "Alternative Step 2".into(),
                "Alt Resp 2".into(),
                Some(TxId::new(99)),
                vec![],
                "explore",
            )
            .unwrap(); // unwrap
        assert_eq!(branch_step, 3);
        // branch_from() no longer updates active_head
        assert_eq!(dag.active_head(), 2);

        // Path to head (when head is step2): Root -> Step 1 -> Step 2
        let path = dag.path_to_head();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].step_id, 0);
        assert_eq!(path[1].step_id, 1);
        assert_eq!(path[2].step_id, 2);
        assert_eq!(path[2].prompt, "Step 2");

        // Set active head explicitly to branch_step
        dag.set_active_head(branch_step).unwrap();
        assert_eq!(dag.active_head(), 3);

        // Path to head after set_active_head: Root -> Step 1 -> Alternative Step 2
        let path_branch = dag.path_to_head();
        assert_eq!(path_branch.len(), 3);
        assert_eq!(path_branch[0].step_id, 0);
        assert_eq!(path_branch[1].step_id, 1);
        assert_eq!(path_branch[2].step_id, 3);
        assert_eq!(path_branch[2].prompt, "Alternative Step 2");

        // Step 1 should have two children: Step 2 and Step 3
        let children = dag.children_of(step1);
        assert_eq!(children, vec![2, 3]);
    }

    #[test]
    fn test_invalid_parent_branching_fails() {
        let dag = SessionBranchTree::new("Root".into(), "Root Resp".into());
        let res = dag.branch_from(999, "Prompt".into(), "Resp".into(), None, vec![], "test");
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), MemFuseError::InvalidInput(_)));
    }

    #[test]
    fn test_set_active_head() {
        let dag = SessionBranchTree::new("Root".into(), "Root Resp".into());
        let step1 = dag
            .append_step("Step 1".into(), "Resp 1".into(), None, vec![], "main")
            .unwrap(); // unwrap
        let _step2 = dag
            .append_step("Step 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap(); // unwrap

        assert_eq!(dag.active_head(), 2);

        // Switch head back to step1
        dag.set_active_head(step1).unwrap(); // unwrap
        assert_eq!(dag.active_head(), 1);

        let path = dag.path_to_head();
        assert_eq!(path.len(), 2);
        assert_eq!(path[1].step_id, 1);

        // Setting to non-existent node fails
        assert!(dag.set_active_head(999).is_err());
    }

    #[tokio::test]
    async fn session_dag_survives_restart() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap allowed
        );

        let dag = SessionBranchTree::new("Root Prompt".into(), "Root Resp".into());
        let step1 = dag
            .append_step(
                "Step 1 Prompt".into(),
                "Step 1 Resp".into(),
                Some(TxId::new(5)),
                vec!["tool_out".into()],
                "main",
            )
            .unwrap(); // unwrap allowed

        let branch1 = dag
            .branch_from(
                step1,
                "Branch Prompt".into(),
                "Branch Resp".into(),
                None,
                vec![],
                "explore",
            )
            .unwrap(); // unwrap allowed

        // branch_from leaves active_head unchanged (step1)
        assert_eq!(dag.active_head(), step1);

        // Set active_head explicitly to branch1 before saving
        dag.set_active_head(branch1).unwrap(); // unwrap allowed

        let tx = TxId::new(1);
        dag.save(storage.as_ref(), "agent_session_1", tx)
            .await
            .unwrap(); // unwrap allowed
        storage.commit(tx).await.unwrap(); // unwrap allowed
        storage.flush().await.unwrap(); // unwrap allowed

        let loaded_dag = SessionBranchTree::load(storage.as_ref(), "agent_session_1")
            .await
            .unwrap(); // unwrap allowed

        assert_eq!(loaded_dag.node_count(), 3);
        assert_eq!(loaded_dag.active_head(), branch1);

        let path = loaded_dag.path_to_head();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].step_id, 0);
        assert_eq!(path[1].step_id, 1);
        assert_eq!(path[1].snapshot_tx_id, Some(TxId::new(5)));
        assert_eq!(path[2].step_id, branch1);
        assert_eq!(path[2].prompt, "Branch Prompt");

        let children = loaded_dag.children_of(step1);
        assert_eq!(children, vec![branch1]);
    }
}
