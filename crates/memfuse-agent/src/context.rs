//! Operational context for an agent workflow execution.
//!
//! Carries task identity, graph position, token budget, DB references, and
//! accumulated step memory across the entire lifecycle of a single workflow run.

use memfuse_core::TokenBudget;
use memfuse_db::{Collection, MemFuse};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

/// Operational status of an agent workflow.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Failed,
}

/// Operational context spanning the entire workflow execution.
pub struct AgentContext {
    pub task_id: String,
    pub current_node: String,
    pub step_count: u64,
    pub db: Arc<MemFuse>,
    pub state_collection: Arc<Collection<LsmStorage>>,
    pub budget: TokenBudget,
    pub status: AgentStatus,
    /// Accumulates results and state transfers between steps.
    pub memory: HashMap<String, serde_json::Value>,
    /// History of attached background telemetry events.
    pub events: Vec<crate::event_source::BackgroundEvent>,
}

impl AgentContext {
    /// Attempts to construct an `AgentContext` with boundary validation on `task_id` and `start_node`.
    pub fn try_new(
        task_id: impl Into<String>,
        start_node: impl Into<String>,
        db: Arc<MemFuse>,
        state_collection: Arc<Collection<LsmStorage>>,
        budget: TokenBudget,
    ) -> Result<Self> {
        let task_id_str = task_id.into();
        let start_node_str = start_node.into();

        validate_task_id(&task_id_str)?;
        validate_node_id(&start_node_str)?;

        Ok(Self {
            task_id: task_id_str,
            current_node: start_node_str,
            step_count: 0,
            db,
            state_collection,
            budget,
            status: AgentStatus::Idle,
            memory: HashMap::new(),
            events: Vec::new(),
        })
    }

    /// Constructs an `AgentContext`, panicking if `task_id` or `start_node` is invalid.
    pub fn new(
        task_id: impl Into<String>,
        start_node: impl Into<String>,
        db: Arc<MemFuse>,
        state_collection: Arc<Collection<LsmStorage>>,
        budget: TokenBudget,
    ) -> Self {
        Self::try_new(task_id, start_node, db, state_collection, budget)
            .expect("Invalid task_id or start_node in AgentContext::new")
    }

    /// Integrates a background telemetry event into the agent context memory and history.
    pub fn attach_event(&mut self, event: crate::event_source::BackgroundEvent) {
        if let Ok(val) = serde_json::to_value(&event) {
            self.memory.insert("latest_event".to_string(), val);
        }
        self.events.push(event);
    }
}
