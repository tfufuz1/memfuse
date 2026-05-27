//! Operational context for an agent workflow execution.
//!
//! Carries task identity, graph position, token budget, DB references, and
//! accumulated step memory across the entire lifecycle of a single workflow run.

use memfuse_core::TokenBudget;
use memfuse_db::{Collection, MemFuse};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

/// Operational context spanning the entire workflow execution.
pub struct AgentContext {
    pub task_id: String,
    pub current_node: String,
    pub step_count: u64,
    pub db: Arc<MemFuse>,
    pub state_collection: Arc<Collection<LsmStorage>>,
    pub budget: TokenBudget,
    /// Accumulates results and state transfers between steps.
    pub memory: HashMap<String, serde_json::Value>,
}

impl AgentContext {
    pub fn new(
        task_id: impl Into<String>,
        start_node: impl Into<String>,
        db: Arc<MemFuse>,
        state_collection: Arc<Collection<LsmStorage>>,
        budget: TokenBudget,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            current_node: start_node.into(),
            step_count: 0,
            db,
            state_collection,
            budget,
            memory: HashMap::new(),
        }
    }
}
