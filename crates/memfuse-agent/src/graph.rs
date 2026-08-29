//! Declarative StateGraph definition for Agent Workflows.

// INVARIANT: Deklarativer StateGraph.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = String;

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
    // AI-TAG[HARDENING][CRITICAL]: Validates non-empty Node ID and description for graph nodes. (TS:2026-08-29T17:22:08Z) (SESSION:bc60d045)
    pub fn try_add_node(
        &mut self,
        id: &str,
        description: &str,
        node_type: NodeType,
        handler: Option<&str>,
    ) -> memfuse_core::Result<()> {
        if id.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "StateGraph node id must not be empty".to_string(),
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
    // AI-TAG[HARDENING][CRITICAL]: Validates non-empty from/to endpoints for workflow edges. (TS:2026-08-29T17:22:08Z) (SESSION:bc60d045)
    pub fn try_add_edge(
        &mut self,
        from: &str,
        to: &str,
        condition: Option<&str>,
        priority: u8,
    ) -> memfuse_core::Result<()> {
        if from.trim().is_empty() || to.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "WorkflowEdge endpoints 'from' and 'to' must not be empty".to_string(),
            ));
        }

        self.edges.push(WorkflowEdge {
            from: from.to_string(),
            to: to.to_string(),
            condition: condition.map(|s| s.to_string()),
            priority,
        });
        Ok(())
    }

    pub fn add_edge(&mut self, from: &str, to: &str, condition: Option<&str>, priority: u8) {
        self.try_add_edge(from, to, condition, priority)
            .unwrap_or_else(|e| panic!("Failed to add WorkflowEdge: {e}"));
    }

    pub fn get_node(&self, id: &str) -> Option<&AgentNode> {
        self.nodes.get(id)
    }
}
