// Comprehensive tests verifying boundary input validations and resource limits.

use memfuse_core::BoxFuture;

use memfuse_agent::context::{validate_node_id, validate_task_id};
use memfuse_agent::{
    AgentContext, AgentTool, BackgroundEvent, NodeType, OrchestratorEngine, StateGraph, StepResult,
    VecEventSource,
};
use memfuse_core::MemFuseError;
use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

struct DummyTool {
    name: String,
}

impl AgentTool for DummyTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn execute<'a>(
        &'a self,
        _ctx: &'a AgentContext,
        _input: serde_json::Value,
    ) -> BoxFuture<'a, memfuse_core::Result<StepResult>> {
        Box::pin(async move {
            Ok(StepResult {
                node_id: "test".to_string(),
                output: serde_json::Value::Null,
                tokens_consumed: 0,
                next_edge: None,
            })
        })
    }
}

#[test]
fn test_task_and_node_id_validation_boundaries() {
    // 1. Empty string
    assert!(matches!(
        validate_task_id(""),
        Err(MemFuseError::InvalidInput(_))
    ));
    assert!(matches!(
        validate_node_id(""),
        Err(MemFuseError::InvalidInput(_))
    ));

    // 2. Null byte
    assert!(matches!(
        validate_task_id("task\0id"),
        Err(MemFuseError::InvalidInput(_))
    ));
    assert!(matches!(
        validate_node_id("node\0id"),
        Err(MemFuseError::InvalidInput(_))
    ));

    // 3. Oversized string (> 256 bytes)
    let oversized = "x".repeat(257);
    assert!(matches!(
        validate_task_id(&oversized),
        Err(MemFuseError::InvalidInput(_))
    ));
    assert!(matches!(
        validate_node_id(&oversized),
        Err(MemFuseError::InvalidInput(_))
    ));

    // 4. Valid string (256 bytes)
    let valid_max = "x".repeat(256);
    assert!(validate_task_id(&valid_max).is_ok());
    assert!(validate_node_id(&valid_max).is_ok());
}

#[test]
fn test_state_graph_boundary_validations() {
    let mut graph = StateGraph::new();

    // Invalid node ID
    assert!(graph
        .try_add_node("node\0null", "desc", NodeType::Task, None)
        .is_err());

    // Invalid handler name (empty)
    assert!(graph
        .try_add_node("task1", "desc", NodeType::Task, Some(""))
        .is_err());

    // Invalid handler name (null byte)
    assert!(graph
        .try_add_node("task1", "desc", NodeType::Task, Some("handler\0null"))
        .is_err());

    // Invalid edge endpoints
    assert!(graph.try_add_edge("", "end", None, 1).is_err());
    assert!(graph.try_add_edge("start", "end\0", None, 1).is_err());
}

#[tokio::test]
async fn test_orchestrator_tool_registration_boundaries() {
    let temp_dir = TempDir::new().unwrap();
    let config = MemFuseConfig::default();
    let db = MemFuse::open_with_config(temp_dir.path(), config)
        .await
        .unwrap();
    let mut engine = OrchestratorEngine::new(db.inner_storage());

    // Empty tool name
    let empty_tool = Box::new(DummyTool {
        name: "".to_string(),
    });
    assert!(engine.try_register_tool(empty_tool).is_err());

    // Null byte tool name
    let null_tool = Box::new(DummyTool {
        name: "tool\0null".to_string(),
    });
    assert!(engine.try_register_tool(null_tool).is_err());

    // Valid tool name
    let valid_tool = Box::new(DummyTool {
        name: "valid_tool".to_string(),
    });
    assert!(engine.try_register_tool(valid_tool).is_ok());
}

#[test]
fn test_background_event_and_vec_source_capacity_boundaries() {
    // Invalid source
    assert!(BackgroundEvent::try_new(serde_json::json!({}), "", 1).is_err());
    assert!(BackgroundEvent::try_new(serde_json::json!({}), "src\0", 1).is_err());

    // Exceed capacity in VecEventSource
    let mut events = Vec::new();
    for i in 0..10_001 {
        events.push(BackgroundEvent::try_new(serde_json::json!({}), "valid_source", i).unwrap());
    }
    assert!(VecEventSource::try_new(events).is_err());
}
