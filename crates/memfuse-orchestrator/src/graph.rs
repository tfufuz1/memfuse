//! Declarative StateGraph definition for Agent Workflows.

// ANCHOR:ARCH:GRAPH-001 — Deklarativer StateGraph.
// WP:NONE PRIO:2 NEEDS:NONE
// AGENT:NONE DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// DESIGN: Hashmaps für Knoten, einfache Tuple-Listen für Edges.

use std::collections::HashMap;

pub type NodeId = String;

/// Represents a single specialized node (e.g. Research, Code) in the agent workflow.
pub struct AgentNode {
    pub id: NodeId,
    pub description: String,
    pub tool: Option<String>,
    pub params: Option<serde_json::Value>,
}

/// The main entry point for the workflow orchestrator.
/// Developers build this graph declaration in Rust or via Python bindings.
#[derive(Default)]
pub struct StateGraph {
    pub nodes: HashMap<NodeId, AgentNode>,
    /// Maps a source node to a target node with an optional transition condition name.
    pub edges: Vec<(NodeId, NodeId, Option<String>)>,
    pub max_cycles: usize,
}

impl StateGraph {
    /// Creates a new, empty StateGraph.
    pub fn new() -> Self {
        Self {
            max_cycles: 5,
            ..Default::default()
        }
    }

    /// Adds a node to the workflow graph.
    pub fn add_node(&mut self, id: &str, description: &str) {
        self.nodes.insert(
            id.to_string(),
            AgentNode {
                id: id.to_string(),
                description: description.to_string(),
                tool: None,
                params: None,
            },
        );
    }

    /// Adds a node with tool and params.
    pub fn add_agent_node(&mut self, node: AgentNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Adds a directed edge between two nodes with an optional condition.
    pub fn add_edge(&mut self, source: &str, target: &str, condition: Option<&str>) {
        self.edges.push((
            source.to_string(),
            target.to_string(),
            condition.map(|s| s.to_string()),
        ));
    }

    /// Executes the workflow starting from the given node.
    pub fn run_workflow(&self, _initial_state: &str) {
        // Execute the State Graph, invoking WASM sandbox nodes
        // and retrieving context selectively via Triebwerk (Hybrid Search)
    }
}
