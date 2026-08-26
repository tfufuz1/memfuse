//! Session-DAG for Agent State Branching (Grok Pattern).
//!
//! Native DAG implementation based on standard library maps and RwLock,
//! keeping memfuse-graph pure-Rust and zero-external-graph-dependency (ADR-004).

use memfuse_core::{MemFuseError, Result, TxId};
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
        self.branch_from(
            parent,
            prompt,
            response,
            snapshot_tx_id,
            tool_outputs,
            label,
        )
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
        *self.active_head.write() = new_id;

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
            .unwrap();
        assert_eq!(step1, 1);
        assert_eq!(dag.active_head(), 1);

        let step2 = dag
            .append_step("Prompt 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap();
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
            .unwrap();
        let step2 = dag
            .append_step("Step 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap();
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
            .unwrap();
        assert_eq!(branch_step, 3);
        assert_eq!(dag.active_head(), 3);

        // Path to head should be: Root -> Step 1 -> Alternative Step 2
        let path = dag.path_to_head();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].step_id, 0);
        assert_eq!(path[1].step_id, 1);
        assert_eq!(path[2].step_id, 3);
        assert_eq!(path[2].prompt, "Alternative Step 2");

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
            .unwrap();
        let _step2 = dag
            .append_step("Step 2".into(), "Resp 2".into(), None, vec![], "main")
            .unwrap();

        assert_eq!(dag.active_head(), 2);

        // Switch head back to step1
        dag.set_active_head(step1).unwrap();
        assert_eq!(dag.active_head(), 1);

        let path = dag.path_to_head();
        assert_eq!(path.len(), 2);
        assert_eq!(path[1].step_id, 1);

        // Setting to non-existent node fails
        assert!(dag.set_active_head(999).is_err());
    }
}
