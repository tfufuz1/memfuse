// FILE-CONTEXT Header (Format v3)
// ZWECK: Declarative StateGraph definition routing workflow step transitions.
// INVARIANTEN: Distinct from memfuse-graph::csr; validates node IDs, handler names, descriptions, and edge conditions.
// NICHT-OFFENSICHTLICH: try_add_node validates handler when present; add_node/add_edge marked #[deprecated].
// HOTSPOTS: try_add_node (ll. 60-110), try_add_edge (ll. 130-160).
// STAND: TS:2026-08-31T21:07:58Z (SESSION: 5f1a7b8e)

//! Declarative StateGraph definition for Agent Workflows.

use crate::context::{validate_node_id, MAX_ID_LEN};
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = String;

/// Maximum allowed length in bytes for descriptions and conditions.
pub const MAX_TEXT_LEN: usize = 65_536;

/// Type of node within the declarative agent state graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Start,
    Task,
    Decision,
    End,
}

/// Represents a single specialized node in the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub id: NodeId,
    pub description: String,
    pub node_type: NodeType,
    /// Registered name of the tool/function to execute, if applicable.
    pub handler: Option<String>,
}

/// Represents a conditional transition between two graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: Option<String>,
    pub priority: u8,
}

/// Core declarative structure routing autonomous agent steps.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StateGraph {
    pub nodes: HashMap<NodeId, AgentNode>,
    pub edges: Vec<WorkflowEdge>,
}

impl StateGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tries to insert a new node into the state graph after validating bounds.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty Node ID and description for graph nodes. (TS:2026-08-31T21:07:58Z) (SESSION: 5f1a7b8e)
    pub fn try_add_node(
        &mut self,
        id: &str,
        description: &str,
        node_type: NodeType,
        handler: Option<&str>,
    ) -> Result<()> {
        validate_node_id(id)?;

        if description.len() > MAX_TEXT_LEN {
            return Err(MemFuseError::InvalidInput(format!(
                "Node description length {} exceeds maximum allowed length of {}",
                description.len(),
                MAX_TEXT_LEN
            )));
        }
        if description.contains('\0') {
            return Err(MemFuseError::InvalidInput(
                "Node description cannot contain null bytes".to_string(),
            ));
        }
        if description.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "StateGraph node description must not be empty".to_string(),
            ));
        }

        if let Some(h) = handler {
            if h.trim().is_empty() {
                return Err(MemFuseError::InvalidInput(
                    "Node handler cannot be empty when provided".to_string(),
                ));
            }
            if h.len() > MAX_ID_LEN {
                return Err(MemFuseError::InvalidInput(format!(
                    "Node handler length {} exceeds maximum allowed length of {}",
                    h.len(),
                    MAX_ID_LEN
                )));
            }
            if h.contains('\0') {
                return Err(MemFuseError::InvalidInput(
                    "Node handler cannot contain null bytes".to_string(),
                ));
            }
        }

        self.nodes.insert(
            id.to_string(),
            AgentNode {
                id: id.to_string(),
                description: description.to_string(),
                node_type,
                handler: handler.map(|s| s.to_string()),
            },
        );
        Ok(())
    }

    /// Adds a node to the graph, panicking if validation fails.
    #[deprecated(note = "Use try_add_node instead to handle validation errors without panicking")]
    pub fn add_node(
        &mut self,
        id: &str,
        description: &str,
        node_type: NodeType,
        handler: Option<&str>,
    ) {
        self.try_add_node(id, description, node_type, handler)
            .unwrap_or_else(|e| panic!("Failed to add StateGraph node: {e}"));
    }

    /// Tries to insert a new edge between nodes in the state graph after validating bounds.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty from/to endpoints for workflow edges. (TS:2026-08-31T21:07:58Z) (SESSION: 5f1a7b8e)
    pub fn try_add_edge(
        &mut self,
        from: &str,
        to: &str,
        condition: Option<&str>,
        priority: u8,
    ) -> Result<()> {
        validate_node_id(from)?;
        validate_node_id(to)?;

        if let Some(cond) = condition {
            if cond.len() > MAX_TEXT_LEN {
                return Err(MemFuseError::InvalidInput(format!(
                    "Edge condition length {} exceeds maximum allowed length of {}",
                    cond.len(),
                    MAX_TEXT_LEN
                )));
            }
            if cond.contains('\0') {
                return Err(MemFuseError::InvalidInput(
                    "Edge condition cannot contain null bytes".to_string(),
                ));
            }
        }

        self.edges.push(WorkflowEdge {
            from: from.to_string(),
            to: to.to_string(),
            condition: condition.map(|s| s.to_string()),
            priority,
        });
        Ok(())
    }

    /// Adds an edge to the graph, panicking if validation fails.
    #[deprecated(note = "Use try_add_edge instead to handle validation errors without panicking")]
    pub fn add_edge(&mut self, from: &str, to: &str, condition: Option<&str>, priority: u8) {
        self.try_add_edge(from, to, condition, priority)
            .unwrap_or_else(|e| panic!("Failed to add WorkflowEdge: {e}"));
    }

    pub fn get_node(&self, id: &str) -> Option<&AgentNode> {
        self.nodes.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_type_serde_roundtrip_CASE_all_variants() {
        let variants = vec![
            NodeType::Start,
            NodeType::Task,
            NodeType::Decision,
            NodeType::End,
        ];

        for variant in variants {
            let serialized = serde_json::to_string(&variant).expect("Serde error");
            let deserialized: NodeType =
                serde_json::from_str(&serialized).expect("Deserialization error");
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_stategraph_serde_roundtrip_CASE_full_graph() {
        let mut graph = StateGraph::new();
        graph
            .try_add_node("start", "Start Node", NodeType::Start, None)
            .unwrap();
        graph
            .try_add_node(
                "task1",
                "Execute Step",
                NodeType::Task,
                Some("tool_calculator"),
            )
            .unwrap();
        graph
            .try_add_node("end", "End Node", NodeType::End, None)
            .unwrap();

        graph
            .try_add_edge("start", "task1", Some("always"), 10)
            .unwrap();
        graph.try_add_edge("task1", "end", None, 1).unwrap();

        let serialized = serde_json::to_string(&graph).expect("Serialization failed");
        let deserialized: StateGraph =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.nodes.len(), 3);
        assert_eq!(deserialized.edges.len(), 2);

        let task_node = deserialized.get_node("task1").expect("Node task1 missing");
        assert_eq!(task_node.description, "Execute Step");
        assert_eq!(task_node.node_type, NodeType::Task);
        assert_eq!(task_node.handler, Some("tool_calculator".to_string()));
    }

    #[test]
    fn test_graph_add_node_validation_CASE_edge_cases() {
        let mut graph = StateGraph::new();
        assert!(graph
            .try_add_node("start", "Start Node", NodeType::Start, None)
            .is_ok());

        // Unicode in description and handler
        assert!(graph
            .try_add_node(
                "unicode_node",
                "Agenten-Workflow 🧠 Startpunkt mit Umlauten",
                NodeType::Task,
                Some("werkzeug_prüfer")
            )
            .is_ok());

        // Empty ID
        assert!(matches!(
            graph.try_add_node("", "desc", NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Empty / Whitespace-only description
        assert!(matches!(
            graph.try_add_node("node1", "   ", NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Oversized description (> MAX_TEXT_LEN)
        let huge_desc = "d".repeat(MAX_TEXT_LEN + 1);
        assert!(matches!(
            graph.try_add_node("task1", &huge_desc, NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Description containing null byte
        assert!(matches!(
            graph.try_add_node("task_null", "desc\0null", NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Empty handler name when Some("")
        assert!(matches!(
            graph.try_add_node("task2", "desc", NodeType::Task, Some("")),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Oversized handler name (> MAX_ID_LEN)
        let huge_handler = "h".repeat(MAX_ID_LEN + 1);
        assert!(matches!(
            graph.try_add_node("task3", "desc", NodeType::Task, Some(&huge_handler)),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Null byte in handler name
        assert!(matches!(
            graph.try_add_node("task4", "desc", NodeType::Task, Some("handler\0null")),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_graph_add_edge_validation_CASE_edge_cases() {
        let mut graph = StateGraph::new();
        assert!(graph.try_add_edge("start", "end", None, 1).is_ok());

        // Unicode condition
        assert!(graph
            .try_add_edge("start", "end", Some("bedingung_erfüllt ✓"), 5)
            .is_ok());

        // Null byte in endpoint
        assert!(matches!(
            graph.try_add_edge("start\0", "end", None, 1),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Oversized condition
        let huge_cond = "c".repeat(MAX_TEXT_LEN + 1);
        assert!(matches!(
            graph.try_add_edge("start", "end", Some(&huge_cond), 1),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Null byte in condition
        assert!(matches!(
            graph.try_add_edge("start", "end", Some("cond\0null"), 1),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_graph_get_node_CASE_nonexistent() {
        let graph = StateGraph::new();
        assert!(graph.get_node("ghost_node").is_none());
    }
}
