// FILE-CONTEXT
// STAND: 2026-08-30T18:51:50Z (SESSION: c9c33dfb)
// ZWECK: Deklarative StateGraph-Definition für Agenten-Workflows.
// INVARIANTEN: Node- und Edge-Anzahl hart begrenzt (MAX_GRAPH_NODES / MAX_GRAPH_EDGES).
// NICHT-OFFENSICHTLICH: Keine externen Graph-Bibliotheken (kein petgraph); pure Rust sovereign graph representation.
// HOTSPOTS: StateGraph::try_add_node, StateGraph::try_add_edge
// SIEHE AUCH: rules/tag_taxonomy.md, AGENTS.md

//! Declarative StateGraph definition for Agent Workflows.

use crate::context::{validate_node_id, MAX_ID_LEN};
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = String;

/// Maximum allowed length in bytes for descriptions and conditions.
pub const MAX_TEXT_LEN: usize = 65_536;

/// Maximum allowed nodes in a single StateGraph.
pub const MAX_GRAPH_NODES: usize = 10_000;

/// Maximum allowed edges in a single StateGraph.
pub const MAX_GRAPH_EDGES: usize = 50_000;

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

    /// Tries to insert a new node into the state graph after validating bounds and graph capacity.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty Node ID and description for graph nodes. (TS:2026-08-30T18:51:50Z) (SESSION: c9c33dfb)
    pub fn try_add_node(
        &mut self,
        id: &str,
        description: &str,
        node_type: NodeType,
        handler: Option<&str>,
    ) -> Result<()> {
        validate_node_id(id)?;

        if self.nodes.len() >= MAX_GRAPH_NODES && !self.nodes.contains_key(id) {
            return Err(MemFuseError::InvalidInput(format!(
                "StateGraph node limit of {} reached",
                MAX_GRAPH_NODES
            )));
        }

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

        if let Some(h) = handler {
            if h.is_empty() {
                return Err(MemFuseError::InvalidInput(
                    "Handler name cannot be empty".to_string(),
                ));
            }
            if h.len() > MAX_ID_LEN {
                return Err(MemFuseError::InvalidInput(format!(
                    "Handler name length {} exceeds maximum allowed length of {}",
                    h.len(),
                    MAX_ID_LEN
                )));
            }
            if h.contains('\0') {
                return Err(MemFuseError::InvalidInput(
                    "Handler name cannot contain null bytes".to_string(),
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
    ///
    /// # Panics
    /// Panics if input parameters are invalid or if node limit (`MAX_GRAPH_NODES`) is exceeded.
    pub fn add_node(
        &mut self,
        id: &str,
        description: &str,
        node_type: NodeType,
        handler: Option<&str>,
    ) {
        self.try_add_node(id, description, node_type, handler)
            .expect("Invalid parameters for add_node");
    }

    /// Tries to insert a new edge between nodes in the state graph after validating bounds and edge capacity.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty from/to endpoints for workflow edges. (TS:2026-08-30T18:51:50Z) (SESSION: c9c33dfb)
    pub fn try_add_edge(
        &mut self,
        from: &str,
        to: &str,
        condition: Option<&str>,
        priority: u8,
    ) -> Result<()> {
        validate_node_id(from)?;
        validate_node_id(to)?;

        if self.edges.len() >= MAX_GRAPH_EDGES {
            return Err(MemFuseError::InvalidInput(format!(
                "StateGraph edge limit of {} reached",
                MAX_GRAPH_EDGES
            )));
        }

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
    ///
    /// # Panics
    /// Panics if input endpoints/conditions are invalid or if edge limit (`MAX_GRAPH_EDGES`) is exceeded.
    pub fn add_edge(&mut self, from: &str, to: &str, condition: Option<&str>, priority: u8) {
        self.try_add_edge(from, to, condition, priority)
            .expect("Invalid parameters for add_edge");
    }

    pub fn get_node(&self, id: &str) -> Option<&AgentNode> {
        self.nodes.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_add_node_validation() {
        let mut graph = StateGraph::new();
        assert!(graph
            .try_add_node("start", "Start Node", NodeType::Start, None)
            .is_ok());

        // Empty ID
        assert!(matches!(
            graph.try_add_node("", "desc", NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Oversized description
        let huge_desc = "d".repeat(MAX_TEXT_LEN + 1);
        assert!(matches!(
            graph.try_add_node("task1", &huge_desc, NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Empty handler name
        assert!(matches!(
            graph.try_add_node("task2", "desc", NodeType::Task, Some("")),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_graph_add_edge_validation() {
        let mut graph = StateGraph::new();
        assert!(graph.try_add_edge("start", "end", None, 1).is_ok());

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
    }

    #[test]
    fn test_graph_node_and_edge_capacity_limits() {
        let mut graph = StateGraph::new();

        // Fill up nodes to limit
        for i in 0..MAX_GRAPH_NODES {
            assert!(graph
                .try_add_node(&format!("n{i}"), "node", NodeType::Task, None)
                .is_ok());
        }

        // Updating existing node should succeed
        assert!(graph
            .try_add_node("n0", "updated", NodeType::Task, None)
            .is_ok());

        // Adding one more new node should fail with InvalidInput
        assert!(matches!(
            graph.try_add_node("overflow_node", "overflow", NodeType::Task, None),
            Err(MemFuseError::InvalidInput(_))
        ));

        // Fill up edges to limit
        for i in 0..MAX_GRAPH_EDGES {
            let from = format!("n{}", i % MAX_GRAPH_NODES);
            let to = format!("n{}", (i + 1) % MAX_GRAPH_NODES);
            assert!(graph.try_add_edge(&from, &to, None, 1).is_ok());
        }

        // Adding one more edge should fail with InvalidInput
        assert!(matches!(
            graph.try_add_edge("n0", "n1", None, 1),
            Err(MemFuseError::InvalidInput(_))
        ));
    }
}
