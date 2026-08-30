//! Declarative StateGraph definition for Agent Workflows.

// INVARIANT: Deklarativer StateGraph.

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
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty Node ID and description for graph nodes. (TS:2026-08-30T15:00:19Z) (SESSION: 283abf0f)
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
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty from/to endpoints for workflow edges. (TS:2026-08-30T15:00:19Z) (SESSION: 283abf0f)
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
}
