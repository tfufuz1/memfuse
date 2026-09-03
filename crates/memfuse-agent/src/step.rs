//! Step result and tool trait definitions for agent workflows.
//!
//! Each agent step produces a [`StepResult`] and tools implement the
//! [`AgentTool`] trait to participate in the orchestration loop.

use crate::context::AgentContext;
use memfuse_core::Result;
use serde::{Deserialize, Serialize};

/// The explicit result of an agent step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Returns the estimated token cost of executing this tool with the given input.
    ///
    /// Used for strict pre-execution budget validation to prevent side-effects when budget is exhausted.
    fn estimated_cost(&self, _input: &serde_json::Value) -> usize {
        0
    }

    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
}
