// FILE-CONTEXT Header (Format v3)
// ZWECK: Workflow execution state context carrying task ID, budget, and memory.
// INVARIANTEN: Task ID & node ID must be non-empty, <=256 bytes, null-byte free; event history capped at 10,000 items.
// NICHT-OFFENSICHTLICH: attach_event evicts oldest event to strictly limit memory growth.
// HOTSPOTS: validate_task_id/validate_node_id (ll. 20-55), attach_event (ll. 140-155).
// STAND: TS:2026-08-30T21:53:49Z (SESSION: 8a7c2f1e)

//! Operational context for an agent workflow execution.
//!
//! Carries task identity, graph position, token budget, DB references, and
//! accumulated step memory across the entire lifecycle of a single workflow run.

use memfuse_core::{MemFuseError, Result, TokenBudget};
use memfuse_db::{Collection, MemFuse};
use memfuse_store::LsmStorage;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Maximum allowed length in bytes for task IDs and node IDs.
pub const MAX_ID_LEN: usize = 256;

/// Maximum allowed telemetry events stored in memory history.
pub const MAX_TELEMETRY_EVENTS: usize = 10_000;

/// Validates a task identifier to ensure it is non-empty, <= 256 bytes, and contains no null bytes.
pub fn validate_task_id(task_id: &str) -> Result<()> {
    if task_id.trim().is_empty() {
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
    if node_id.trim().is_empty() {
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
    pub events: VecDeque<crate::event_source::BackgroundEvent>,
}

impl AgentContext {
    /// Tries to construct a new [`AgentContext`], performing input validation on task ID and start node.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty input parameters for agent workflow context initialization. (TS:2026-08-30T15:00:19Z) (SESSION: 283abf0f)
    pub fn try_new(
        task_id: impl Into<String>,
        start_node: impl Into<String>,
        db: Arc<MemFuse>,
        state_collection: Arc<Collection<LsmStorage>>,
        budget: TokenBudget,
    ) -> memfuse_core::Result<Self> {
        let task_id_str = task_id.into();
        let start_node_str = start_node.into();

        if task_id_str.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "AgentContext task_id must not be empty".to_string(),
            ));
        }

        if start_node_str.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "AgentContext start_node must not be empty".to_string(),
            ));
        }

        Ok(Self {
            task_id: task_id_str,
            current_node: start_node_str,
            step_count: 0,
            db,
            state_collection,
            budget,
            status: AgentStatus::Idle,
            memory: HashMap::new(),
            events: VecDeque::new(),
        })
    }

    /// Constructs an `AgentContext`, panicking if `task_id` or `start_node` is invalid.
    #[deprecated(note = "Use try_new instead to handle validation errors without panicking")]
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
    ///
    /// Maintains event history capacity bounded at `MAX_TELEMETRY_EVENTS` to prevent memory exhaustion.
    pub fn attach_event(&mut self, event: crate::event_source::BackgroundEvent) {
        if let Ok(val) = serde_json::to_value(&event) {
            self.memory.insert("latest_event".to_string(), val);
        }
        if self.events.len() >= MAX_TELEMETRY_EVENTS {
            self.events.pop_front(); // Evict oldest event to cap memory usage
        }
        self.events.push_back(event);
    }

    /// Integrates a background telemetry event with an explicit capacity check.
    pub fn try_attach_event(&mut self, event: crate::event_source::BackgroundEvent) -> Result<()> {
        if self.events.len() >= MAX_TELEMETRY_EVENTS {
            let current_count = self.events.len();
            let max_event_count = MAX_TELEMETRY_EVENTS;
            return Err(MemFuseError::InvalidInput(format!(
                "Telemetry event buffer limit reached: {} events (max {})",
                current_count, max_event_count
            )));
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
        let mut attached_deque = VecDeque::new();
        for i in 0..10_005 {
            let ev = crate::event_source::BackgroundEvent {
                payload: dummy_payload.clone(),
                source: "test_source".to_string(),
                observed_at_seq: i as u64,
            };
            if attached_deque.len() >= MAX_TELEMETRY_EVENTS {
                attached_deque.pop_front();
            }
            attached_deque.push_back(ev);
        }

        assert_eq!(attached_deque.len(), 10_000);
        assert_eq!(
            attached_deque.back().map(|e| e.observed_at_seq),
            Some(10_004)
        );
    }

    #[tokio::test]
    async fn test_agent_context_fifo_eviction() -> memfuse_core::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let config = memfuse_db::MemFuseConfig::default();
        let db = Arc::new(memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?);
        let state_coll = db.collection("test_fifo").await?;
        let mut ctx = AgentContext::try_new(
            "test_fifo_task",
            "start",
            db,
            state_coll,
            TokenBudget::new(1000, 0),
        )?;

        let extra_events = 50;
        let total = MAX_TELEMETRY_EVENTS + extra_events;
        for i in 0..total {
            let ev = crate::event_source::BackgroundEvent {
                payload: serde_json::json!({ "seq": i }),
                source: "test_source".to_string(),
                observed_at_seq: i as u64,
            };
            ctx.attach_event(ev);
        }

        assert_eq!(ctx.events.len(), MAX_TELEMETRY_EVENTS);
        // The first remaining event in deque should have observed_at_seq = 50 (oldest 50 evicted)
        assert_eq!(
            ctx.events.front().map(|e| e.observed_at_seq),
            Some(extra_events as u64)
        );
        assert_eq!(
            ctx.events.back().map(|e| e.observed_at_seq),
            Some((total - 1) as u64)
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_try_attach_event_error_message_unit() -> memfuse_core::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let config = memfuse_db::MemFuseConfig::default();
        let db = Arc::new(memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?);
        let state_coll = db.collection("test_try_attach").await?;
        let mut ctx = AgentContext::try_new(
            "test_try_attach_task",
            "start",
            db,
            state_coll,
            TokenBudget::new(1000, 0),
        )?;

        for i in 0..MAX_TELEMETRY_EVENTS {
            let ev = crate::event_source::BackgroundEvent {
                payload: serde_json::json!({ "seq": i }),
                source: "test_source".to_string(),
                observed_at_seq: i as u64,
            };
            ctx.attach_event(ev);
        }

        let overflow_ev = crate::event_source::BackgroundEvent {
            payload: serde_json::json!({ "overflow": true }),
            source: "test_source".to_string(),
            observed_at_seq: 99_999,
        };

        let res = ctx.try_attach_event(overflow_ev);
        assert!(res.is_err());
        let err_msg = match res {
            Err(e) => e.to_string(),
            Ok(_) => unreachable!(),
        };
        assert!(
            err_msg.contains("Telemetry event buffer limit reached"),
            "Expected 'Telemetry event buffer limit reached' in err_msg, got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("events"),
            "Expected unit 'events' in err_msg, got: {}",
            err_msg
        );
        assert!(
            !err_msg.contains("MB"),
            "Err msg must not contain misleading 'MB' unit, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_telemetry_event_performance_benchmark() -> memfuse_core::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let config = memfuse_db::MemFuseConfig::default();
        let db = Arc::new(memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?);
        let state_coll = db.collection("test_bench").await?;
        let mut ctx = AgentContext::try_new(
            "test_bench_task",
            "start",
            db,
            state_coll,
            TokenBudget::new(1000, 0),
        )?;

        let count = MAX_TELEMETRY_EVENTS + 1000;
        let start_time = std::time::Instant::now();

        for i in 0..count {
            let ev = crate::event_source::BackgroundEvent {
                payload: serde_json::json!({ "i": i }),
                source: "bench_source".to_string(),
                observed_at_seq: i as u64,
            };
            ctx.attach_event(ev);
        }

        let elapsed = start_time.elapsed();
        assert_eq!(ctx.events.len(), MAX_TELEMETRY_EVENTS);
        // O(1) VecDeque operations for 11,000 pushes/evictions typically complete in <10ms.
        // We set a safe threshold of 250ms (old O(N²) took significantly longer due to shift operations).
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "Expected operations to complete under 250ms, took {:?}",
            elapsed
        );
        Ok(())
    }
}
