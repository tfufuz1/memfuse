// FILE-CONTEXT Header (Format v3)
// ZWECK: Step result and tool trait definitions for agent workflows.
// INVARIANTEN: StepResult carries node execution outcome, consumed tokens, and optional edge direction.
// NICHT-OFFENSICHTLICH: Tools implement AgentTool async trait and receive AgentContext alongside input Value.
// HOTSPOTS: StepResult serialization/deserialization.
// STAND: TS:2026-08-31T21:07:58Z (SESSION: 5f1a7b8e)

//! Step result and tool trait definitions for agent workflows.
//!
//! Each agent step produces a [`StepResult`] and tools implement the
//! [`AgentTool`] trait to participate in the orchestration loop.

use crate::context::AgentContext;
use memfuse_core::Result;
use serde::{Deserialize, Serialize};

/// The explicit result of an agent step execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub node_id: String,
    pub output: serde_json::Value,
    pub tokens_consumed: usize,
    /// Identifier condition of the next edge transition if dictated dynamically.
    pub next_edge: Option<String>,
}

#[async_trait::async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_result_serialization_roundtrip_CASE_with_next_edge() {
        let original = StepResult {
            node_id: "task_node_1".to_string(),
            output: serde_json::json!({"status": "ok", "value": 42}),
            tokens_consumed: 150,
            next_edge: Some("edge_success".to_string()),
        };

        let serialized = serde_json::to_string(&original).expect("Serialization failed");
        let deserialized: StepResult =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.node_id, "task_node_1");
        assert_eq!(deserialized.tokens_consumed, 150);
        assert_eq!(deserialized.next_edge, Some("edge_success".to_string()));
        assert_eq!(deserialized.output["status"], "ok");
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_step_result_serialization_roundtrip_CASE_without_next_edge() {
        let original = StepResult {
            node_id: "end_node".to_string(),
            output: serde_json::Value::Null,
            tokens_consumed: 0,
            next_edge: None,
        };

        let serialized = serde_json::to_string(&original).expect("Serialization failed");
        let deserialized: StepResult =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.node_id, "end_node");
        assert_eq!(deserialized.tokens_consumed, 0);
        assert_eq!(deserialized.next_edge, None);
        assert_eq!(deserialized.output, serde_json::Value::Null);
        assert_eq!(deserialized, original);
    }
}
