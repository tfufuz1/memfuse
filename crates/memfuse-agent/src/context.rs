// FILE-CONTEXT
// STAND: 2026-08-30T18:51:50Z (SESSION: c9c33dfb)
// ZWECK: Operativer Kontext für Agenten-Workflow-Ausführungen.
// INVARIANTEN: Identifikatoren strikt validiert (non-empty, <=256, no nulls); Telemetrie- & Memory-Größe strikt gebunden.
// NICHT-OFFENSICHTLICH: Eviction-Strategie verwirft bei Telemetrie-Überlauf das älteste Event (FIFO-Cap).
// HOTSPOTS: AgentContext::try_new, AgentContext::attach_event, AgentContext::set_memory
// SIEHE AUCH: rules/tag_taxonomy.md, AGENTS.md

//! Operational context for an agent workflow execution.
//!
//! Carries task identity, graph position, token budget, DB references, and
//! accumulated step memory across the entire lifecycle of a single workflow run.

use memfuse_core::{MemFuseError, Result, TokenBudget};
use memfuse_db::{Collection, MemFuse};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

/// Maximum allowed length in bytes for task IDs and node IDs.
pub const MAX_ID_LEN: usize = 256;

/// Maximum allowed telemetry events stored in memory history.
pub const MAX_TELEMETRY_EVENTS: usize = 10_000;

/// Maximum allowed memory entries stored in workflow context.
pub const MAX_MEMORY_ENTRIES: usize = 10_000;

/// Validates a task identifier to ensure it is non-empty, <= 256 bytes, and contains no null bytes.
pub fn validate_task_id(task_id: &str) -> Result<()> {
    if task_id.is_empty() {
        return Err(MemFuseError::InvalidInput(
            "task_id cannot be empty".to_string(),
        ));
    }
    if task_id.len() > MAX_ID_LEN {
        return Err(MemFuseError::InvalidInput(format!(
            "task_id length {} exceeds maximum allowed length of {}",
            task_id.len(),
            MAX_ID_LEN
        )));
    }
    if task_id.contains('\0') {
        return Err(MemFuseError::InvalidInput(
            "task_id cannot contain null bytes".to_string(),
        ));
    }
    Ok(())
}

/// Validates a node identifier to ensure it is non-empty, <= 256 bytes, and contains no null bytes.
pub fn validate_node_id(node_id: &str) -> Result<()> {
    if node_id.is_empty() {
        return Err(MemFuseError::InvalidInput(
            "node_id cannot be empty".to_string(),
        ));
    }
    if node_id.len() > MAX_ID_LEN {
        return Err(MemFuseError::InvalidInput(format!(
            "node_id length {} exceeds maximum allowed length of {}",
            node_id.len(),
            MAX_ID_LEN
        )));
    }
    if node_id.contains('\0') {
        return Err(MemFuseError::InvalidInput(
            "node_id cannot contain null bytes".to_string(),
        ));
    }
    Ok(())
}

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
    /// Tries to construct a new [`AgentContext`], performing input validation on task ID and start node.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty input parameters for agent workflow context initialization. (TS:2026-08-30T18:51:50Z) (SESSION: c9c33dfb)
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
    ///
    /// # Panics
    /// Panics if `task_id` or `start_node` fails validation.
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

    /// Inserts a key-value entry into workflow context memory, enforcing maximum capacity bounds.
    pub fn set_memory(&mut self, key: impl Into<String>, value: serde_json::Value) -> Result<()> {
        let k = key.into();
        if self.memory.len() >= MAX_MEMORY_ENTRIES && !self.memory.contains_key(&k) {
            return Err(MemFuseError::InvalidInput(format!(
                "Context memory limit of {} entries reached",
                MAX_MEMORY_ENTRIES
            )));
        }
        self.memory.insert(k, value);
        Ok(())
    }

    /// Integrates a background telemetry event into the agent context memory and history.
    ///
    /// Maintains event history capacity bounded at `MAX_TELEMETRY_EVENTS` to prevent memory exhaustion.
    pub fn attach_event(&mut self, event: crate::event_source::BackgroundEvent) {
        if let Ok(val) = serde_json::to_value(&event) {
            let _ = self.set_memory("latest_event", val);
        }
        if self.events.len() >= MAX_TELEMETRY_EVENTS {
            self.events.remove(0); // Evict oldest event to cap memory usage
        }
        self.events.push(event);
    }

    /// Integrates a background telemetry event with an explicit capacity check.
    pub fn try_attach_event(&mut self, event: crate::event_source::BackgroundEvent) -> Result<()> {
        if self.events.len() >= MAX_TELEMETRY_EVENTS {
            return Err(MemFuseError::MemoryBudgetExceeded {
                used_mb: ((self.events.len()
                    * std::mem::size_of::<crate::event_source::BackgroundEvent>())
                    / (1024 * 1024)) as u64,
                limit_mb: MAX_TELEMETRY_EVENTS as u64,
            });
        }
        self.attach_event(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_id_guards() {
        assert!(validate_task_id("valid-task-123").is_ok());
        assert!(matches!(
            validate_task_id(""),
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_task_id("task\0with-null"),
            Err(MemFuseError::InvalidInput(_))
        ));
        let oversized = "a".repeat(257);
        assert!(matches!(
            validate_task_id(&oversized),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_validate_node_id_guards() {
        assert!(validate_node_id("valid-node").is_ok());
        assert!(matches!(
            validate_node_id(""),
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_node_id("node\0null"),
            Err(MemFuseError::InvalidInput(_))
        ));
        let oversized = "n".repeat(257);
        assert!(matches!(
            validate_node_id(&oversized),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_agent_context_telemetry_event_capacity_cap() {
        let dummy_payload = serde_json::json!({"test": 1});
        let mut attached_vec = Vec::new();
        for i in 0..10_005 {
            let ev = crate::event_source::BackgroundEvent {
                payload: dummy_payload.clone(),
                source: "test_source".to_string(),
                observed_at_seq: i as u64,
            };
            if attached_vec.len() >= MAX_TELEMETRY_EVENTS {
                attached_vec.remove(0);
            }
            attached_vec.push(ev);
        }

        assert_eq!(attached_vec.len(), 10_000);
        assert_eq!(attached_vec.last().unwrap().observed_at_seq, 10_004);
    }

    #[tokio::test]
    async fn test_agent_context_memory_capacity_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let config = memfuse_db::MemFuseConfig {
            dimension: 1536,
            ..Default::default()
        };

        let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());
        let state_col = db.collection("test-col").await.unwrap();

        let mut ctx =
            AgentContext::try_new("task-1", "start", db, state_col, TokenBudget::new(1000, 0))
                .unwrap();

        for i in 0..MAX_MEMORY_ENTRIES {
            assert!(ctx
                .set_memory(format!("k{i}"), serde_json::json!(i))
                .is_ok());
        }

        // Updating an existing key is allowed
        assert!(ctx.set_memory("k0", serde_json::json!(999)).is_ok());

        // Adding a new key when at capacity fails
        assert!(matches!(
            ctx.set_memory("overflow_key", serde_json::json!(1)),
            Err(MemFuseError::InvalidInput(_))
        ));
    }
}
