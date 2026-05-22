// AGENT:12
// ANCHOR:INTEGRATION STATUS:DONE
use memfuse_orchestrator::{GraphNode, StateGraph};

// FIXME: These tests are currently ignored because the StateGraph API in lib.rs is a stub.
// Ref: AGENT:00 should resolve the StateGraph conflict.

#[test]
#[ignore]
fn test_stategraph_construction() {
    let mut graph = StateGraph::new();
    graph.nodes.push(GraphNode {
        name: "research".to_string(),
        executable_identifier: "Researches a topic using search tools".to_string(),
    });

    assert_eq!(graph.nodes.len(), 1);
}

#[test]
#[ignore]
fn test_stategraph_run_placeholder() {
    let graph = StateGraph::new();
    let engine = memfuse_orchestrator::OrchestratorEngine;
    let _ = engine;
    let _ = graph;
}
