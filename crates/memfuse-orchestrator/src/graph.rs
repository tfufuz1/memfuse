//! Declarative StateGraph definition for Agent Workflows.

use std::collections::HashMap;

pub type NodeId = String;

/// Represents a single specialized node (e.g. Research, Code) in the agent workflow.
pub struct AgentNode {
    pub id: NodeId,
    pub description: String,
}

/// The main entry point for the workflow orchestrator.
/// Developers build this graph declaration in Rust or via Python bindings.
#[derive(Default)]
pub struct StateGraph {
    pub nodes: HashMap<NodeId, AgentNode>,
    /// Maps a source node to a target node with an optional transition condition name.
    pub edges: Vec<(NodeId, NodeId, Option<String>)>,
}

impl StateGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: &str, description: &str) {
        self.nodes.insert(
            id.to_string(),
            AgentNode {
                id: id.to_string(),
                description: description.to_string(),
            },
        );
    }

    pub fn add_edge(&mut self, source: &str, target: &str, condition: Option<&str>) {
        self.edges.push((
            source.to_string(),
            target.to_string(),
            condition.map(|s| s.to_string()),
        ));
    }

    pub fn run_workflow(&self, _initial_state: &str) {
        // Execute the State Graph, invoking WASM sandbox nodes
        // and retrieving context selectively via Triebwerk (Hybrid Search)
    }
}
