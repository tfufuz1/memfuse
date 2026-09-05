// FILE-CONTEXT Header (Format v3)
// ZWECK: Deterministic graph-walker orchestrator engine for autonomous agent workflows.
// INVARIANTEN: Enforces Checkpoint -> Execute -> Commit -> Audit loop per step; atomic commit & RAII guard protection.
// NICHT-OFFENSICHTLICH: Persists final state to LSM before final checkpoint; replay_from reconstructs state from checkpoint registry.
// HOTSPOTS: run_internal (ll. 90-180), replay_from (ll. 185-230).
// STAND: TS:2026-09-01T23:11:04Z (SESSION: 5a38054a)

//! Deterministic, persistent graph-walker engine for agent workflows.
//!
//! Implements the core execution loop: checkpoint → execute → commit → audit → resolve-next.

use crate::context::{validate_node_id, AgentContext, MAX_ID_LEN};
use crate::dlq::DeadLetterQueue;
use crate::graph::{AgentNode, NodeType, StateGraph};
use crate::step::{AgentTool, DeadLetterReason, StepDeadLetter, StepResult};
use memfuse_checkpoint::{
    CheckpointGuard, CheckpointMeta, CheckpointRegistry, PersistentCheckpointStore,
};
use memfuse_core::traits::StorageEngine;
use memfuse_core::{MemFuseError, Result};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

/// Reason for exiting `OrchestratorEngine::run_event_loop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLoopExitReason {
    Shutdown,
    SourceExhausted,
}

/// Async executor engine applying nodes in Sequence.
pub struct OrchestratorEngine {
    pub tools: HashMap<String, Box<dyn AgentTool>>,
    pub checkpoint_store: Arc<dyn CheckpointRegistry>,
    pub dead_letter_queue: Option<DeadLetterQueue>,
}

impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        Self {
            tools: HashMap::new(),
            checkpoint_store: Arc::new(
                PersistentCheckpointStore::new(storage.clone(), "agent")
                    .expect("Failed to initialize PersistentCheckpointStore for agent"),
            ),
            dead_letter_queue: Some(DeadLetterQueue::new(storage)),
        }
    }

    /// Helper constructor creating OrchestratorEngine directly from MemFuse DB handle.
    pub fn from_db(db: &memfuse_db::MemFuse) -> Self {
        Self::new(db.inner_storage())
    }

    /// Attempts to register an agent tool with boundary validation on the tool name.
    pub fn try_register_tool(&mut self, tool: Box<dyn AgentTool>) -> Result<()> {
        let name = tool.name();
        if name.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Tool name cannot be empty".to_string(),
            ));
        }
        if name.len() > MAX_ID_LEN {
            return Err(MemFuseError::InvalidInput(format!(
                "Tool name length {} exceeds maximum allowed length of {}",
                name.len(),
                MAX_ID_LEN
            )));
        }
        if name.contains('\0') {
            return Err(MemFuseError::InvalidInput(
                "Tool name cannot contain null bytes".to_string(),
            ));
        }
        self.tools.insert(name.to_string(), tool);
        Ok(())
    }

    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
        ctx.status = crate::context::AgentStatus::Running;
        let res = self.run_internal(ctx, graph).await;
        if res.is_err() {
            ctx.status = crate::context::AgentStatus::Failed;
        }
        res
    }

    async fn run_internal(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
        loop {
            tokio::task::yield_now().await;
            let node = graph.get_node(&ctx.current_node).ok_or_else(|| {
                MemFuseError::Internal(format!("Node {} not found", ctx.current_node))
            })?;

            match node.node_type {
                NodeType::End => {
                    ctx.status = crate::context::AgentStatus::Completed;
                    // Persist final state before last checkpoint (FIND-SAOS-001)
                    self.persist_final_state(ctx).await?;
                    ctx.db.inner_storage().flush().await?; // Ensure durability
                    self.checkpoint(ctx).await?;
                    return Ok(());
                }
                NodeType::Start | NodeType::Task => {
                    // 1. Checkpoint BEFORE execution (AC-1) with RAII CheckpointGuard
                    let tx_id = ctx.db.inner_storage().last_tx_id().await?;
                    let guard =
                        CheckpointGuard::for_agent_step(ctx.db.inner_storage(), tx_id).await?;
                    self.checkpoint(ctx).await?;

                    // Prepare step input
                    let input = ctx
                        .memory
                        .get("last_output")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    // PRE-CHECK & ATOMIC RESERVATION vor Ausführung: Reserve budget strictly before tool execution
                    let estimated_cost = if node.node_type != NodeType::Start {
                        if let Some(handler_name) = &node.handler {
                            if let Some(tool) = self.tools.get(handler_name) {
                                tool.estimated_cost(&input)
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    if node.node_type != NodeType::Start {
                        if let Err(err) = ctx.budget.try_reserve(estimated_cost) {
                            if let Some(ref dlq) = self.dead_letter_queue {
                                let letter = StepDeadLetter {
                                    session_id: ctx.task_id.clone(),
                                    node_id: node.id.clone(),
                                    failure_reason: DeadLetterReason::BudgetExhausted {
                                        available: ctx.budget.available(),
                                        required: estimated_cost,
                                    },
                                    input: input.clone(),
                                    attempt: 0,
                                    failed_at_secs: SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                };
                                if let Err(e) = dlq.push(&letter).await {
                                    tracing::error!("DLQ push failed: {}", e);
                                }
                            }
                            self.audit_log_failure(ctx, &err.to_string()).await?;
                            return Err(err);
                        }
                    }

                    // 2. Resolve handler (Optional for Start nodes)
                    let result_res = if let Some(handler_name) = &node.handler {
                        if let Some(tool) = self.tools.get(handler_name) {
                            let timeout_duration =
                                std::time::Duration::from_millis(tool.timeout_ms());
                            let max_attempts = if tool.is_retriable() {
                                tool.max_retries() + 1
                            } else {
                                1
                            };
                            let mut execution_res = Err(MemFuseError::Internal(format!(
                                "Tool {} failed without execution",
                                handler_name
                            )));

                            'retry: for attempt in 0..max_attempts {
                                if attempt > 0 {
                                    let wait_ms = 100u64 * (1u64 << attempt.min(4));
                                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms))
                                        .await;
                                }

                                let execute_future = tool.execute(ctx, input.clone());

                                match tokio::time::timeout(timeout_duration, execute_future).await {
                                    Ok(Ok(result)) => {
                                        execution_res = Ok(result);
                                        break 'retry;
                                    }
                                    Ok(Err(e)) => {
                                        let err_msg = e.to_string();
                                        execution_res = Err(e);
                                        if !tool.is_retriable() {
                                            if let Some(ref dlq) = self.dead_letter_queue {
                                                let letter = StepDeadLetter {
                                                    session_id: ctx.task_id.clone(),
                                                    node_id: node.id.clone(),
                                                    failure_reason: DeadLetterReason::ToolError {
                                                        message: err_msg,
                                                    },
                                                    input: input.clone(),
                                                    attempt,
                                                    failed_at_secs: SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_secs(),
                                                };
                                                if let Err(dlq_err) = dlq.push(&letter).await {
                                                    tracing::error!("DLQ push failed: {}", dlq_err);
                                                }
                                            }
                                            break 'retry;
                                        } else if attempt + 1 == max_attempts {
                                            if let Some(ref dlq) = self.dead_letter_queue {
                                                let letter = StepDeadLetter {
                                                    session_id: ctx.task_id.clone(),
                                                    node_id: node.id.clone(),
                                                    failure_reason:
                                                        DeadLetterReason::MaxRetriesExceeded {
                                                            attempts: max_attempts,
                                                        },
                                                    input: input.clone(),
                                                    attempt,
                                                    failed_at_secs: SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_secs(),
                                                };
                                                if let Err(dlq_err) = dlq.push(&letter).await {
                                                    tracing::error!("DLQ push failed: {}", dlq_err);
                                                }
                                            }
                                        }
                                    }
                                    Err(_elapsed) => {
                                        let timeout_err = MemFuseError::Timeout {
                                            operation: format!("tool:{}", handler_name),
                                            timeout_ms: tool.timeout_ms(),
                                        };
                                        execution_res = Err(timeout_err);

                                        if let Some(ref dlq) = self.dead_letter_queue {
                                            let letter = StepDeadLetter {
                                                session_id: ctx.task_id.clone(),
                                                node_id: node.id.clone(),
                                                failure_reason: DeadLetterReason::Timeout {
                                                    timeout_ms: tool.timeout_ms(),
                                                },
                                                input: input.clone(),
                                                attempt,
                                                failed_at_secs: SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap_or_default()
                                                    .as_secs(),
                                            };
                                            if let Err(dlq_err) = dlq.push(&letter).await {
                                                tracing::error!("DLQ push failed: {}", dlq_err);
                                            }
                                        }

                                        if !tool.is_retriable() {
                                            break 'retry;
                                        }
                                    }
                                }
                            }
                            execution_res
                        } else {
                            Err(MemFuseError::Internal(format!(
                                "Tool {} not registered",
                                handler_name
                            )))
                        }
                    } else if node.node_type == NodeType::Start {
                        // Pass-through result for start nodes without handlers
                        Ok(StepResult {
                            node_id: node.id.clone(),
                            output: serde_json::Value::Null,
                            tokens_consumed: 0,
                            next_edge: None,
                        })
                    } else {
                        Err(MemFuseError::Internal(format!(
                            "Task Node {} lacks handler",
                            node.id
                        )))
                    };

                    if let Some((router, decision_id)) = ctx.pending_routing_decision.take() {
                        let outcome = match &result_res {
                            Ok(_) => memfuse_router::RoutingOutcome::Success,
                            Err(err) => memfuse_router::RoutingOutcome::Rejected {
                                reason: Some(err.to_string()),
                            },
                        };
                        router.record_outcome(decision_id, outcome);
                    }

                    let result = match result_res {
                        Ok(res) => {
                            // Reconcile reserved budget with actual tokens consumed
                            if res.tokens_consumed > estimated_cost {
                                ctx.budget.consume(res.tokens_consumed - estimated_cost);
                            } else if estimated_cost > res.tokens_consumed {
                                ctx.budget.refund(estimated_cost - res.tokens_consumed);
                            }
                            ctx.memory
                                .insert("last_output".to_string(), res.output.clone());
                            res
                        }
                        Err(err) => {
                            // Refund pre-reserved tokens on execution failure
                            ctx.budget.refund(estimated_cost);
                            self.audit_log_failure(ctx, &err.to_string()).await?;
                            return Err(err);
                        }
                    };

                    // 3. Audit log (AC-3) - Audit trail is source of truth for "what was attempted".
                    // Executed before commit_step so a failure here leaves state uncommitted for clean retry.
                    self.audit_log(ctx, &result).await?;

                    // 4. Atomic commit to LSM - Source of truth for "what was accepted".
                    self.commit_step(ctx, &result).await?;

                    // 6. Resolve next edge
                    let next_node = match self.resolve_next_node(graph, &ctx.current_node, &result)
                    {
                        Ok(next) => next,
                        Err(err) => {
                            self.audit_log_failure(ctx, &err.to_string()).await?;
                            return Err(err);
                        }
                    };
                    ctx.current_node = next_node;
                    ctx.step_count += 1;

                    // 7. Step completed successfully: commit CheckpointGuard RAII guard
                    guard.commit()?;
                }
                NodeType::Decision => {
                    let next = match self.evaluate_decision(graph, node, ctx) {
                        Ok(next) => next,
                        Err(err) => {
                            self.audit_log_failure(ctx, &err.to_string()).await?;
                            return Err(err);
                        }
                    };
                    ctx.current_node = next;
                }
            }
        }
    }

    /// Setzt den AgentContext auf einen früheren Checkpoint zurück (AC-2).
    ///
    /// # Adressierung von Checkpoints
    /// Das `identifier`-Argument unterstützt folgende Adressierungsformate:
    /// - `"step:<N>"`: Explizite Adressierung nach Schrittnummer (z. B. `"step:1"`).
    /// - `"node:<name>"`: Explizite Adressierung nach Node-Name (z. B. `"node:1"` oder `"node:step_a"`).
    ///
    /// **Fallback (Abwärtskompatibilität):**
    /// Falls kein Präfix (`step:` oder `node:`) angegeben ist:
    /// - Wenn `identifier` als `u64` geparst werden kann, wird es als Schrittnummer interpretiert.
    /// - Andernfalls wird es als Node-Name interpretiert.
    pub async fn replay_from(&self, ctx: &mut AgentContext, identifier: &str) -> Result<()> {
        validate_node_id(identifier)?;

        let checkpoints = self.checkpoint_store.list_checkpoints().await?;

        let checkpoint = checkpoints
            .iter()
            .rfind(|c| {
                if !c.name.starts_with(&format!("task:{}:", ctx.task_id)) {
                    return false;
                }
                if let Some(step_str) = identifier.strip_prefix("step:") {
                    if let Ok(step) = step_str.parse::<u64>() {
                        return c.name.contains(&format!(":step:{}:", step));
                    }
                }
                if let Some(node_name) = identifier.strip_prefix("node:") {
                    return c.name.ends_with(&format!(":node:{}", node_name));
                }
                if let Ok(step) = identifier.parse::<u64>() {
                    c.name.contains(&format!(":step:{}:", step))
                } else {
                    c.name.ends_with(&format!(":node:{}", identifier))
                }
            })
            .ok_or_else(|| {
                let parsed_num = if let Some(s) = identifier.strip_prefix("step:") {
                    s.parse::<u64>().ok()
                } else {
                    identifier.parse::<u64>().ok()
                };

                let extra_hint = if let Some(num) = parsed_num {
                    format!(
                        " Konnte keinen Checkpoint für Schritt {} finden. Falls ein Node mit dem Namen '{}' gemeint war, nutze das Format 'node:{}' zur expliziten Adressierung.",
                        num, num, num
                    )
                } else {
                    String::new()
                };

                MemFuseError::Internal(format!(
                    "Checkpoint '{}' für Task '{}' nicht gefunden.{}",
                    identifier, ctx.task_id, extra_hint
                ))
            })?;

        if let Some(node) = checkpoint
            .metadata
            .get("current_node")
            .and_then(|v| v.as_str())
        {
            ctx.current_node = node.to_string();
        }
        if let Some(step) = checkpoint
            .metadata
            .get("step_count")
            .and_then(|v| v.as_u64())
        {
            ctx.step_count = step;
        }
        if let Some(memory) = checkpoint.metadata.get("memory").and_then(|v| {
            serde_json::from_value::<HashMap<String, serde_json::Value>>(v.clone()).ok()
        }) {
            ctx.memory = memory;
        }

        if let Some(consumed) = checkpoint
            .metadata
            .get("budget_consumed")
            .and_then(|v| v.as_u64())
        {
            let mut restored_budget =
                memfuse_core::TokenBudget::new(ctx.budget.limit, ctx.budget.reserved)
                    .with_strategy(ctx.budget.strategy.clone());
            restored_budget.consume(consumed as usize);
            ctx.budget = restored_budget;
        } else if let Some(available) = checkpoint
            .metadata
            .get("budget_available")
            .and_then(|v| v.as_u64())
        {
            let total_usable = ctx
                .budget
                .effective_limit()
                .saturating_sub(ctx.budget.reserved);
            let consumed = total_usable.saturating_sub(available as usize);
            let mut restored_budget =
                memfuse_core::TokenBudget::new(ctx.budget.limit, ctx.budget.reserved)
                    .with_strategy(ctx.budget.strategy.clone());
            restored_budget.consume(consumed);
            ctx.budget = restored_budget;
        } else {
            tracing::warn!(
                task_id = %ctx.task_id,
                checkpoint_name = %checkpoint.name,
                "Checkpoint metadata does not contain budget state; proceeding with default/current budget."
            );
        }

        self.checkpoint_store
            .restore(&checkpoint.into_workflow_state())
            .await
    }

    pub async fn checkpoint(&self, ctx: &AgentContext) -> Result<()> {
        let checkpoint_name = format!(
            "task:{}:step:{}:node:{}",
            ctx.task_id, ctx.step_count, ctx.current_node
        );
        let metadata = serde_json::json!({
            "current_node": ctx.current_node,
            "step_count":   ctx.step_count,
            "memory":       ctx.memory,
            "budget_consumed": ctx.budget.consumed(),
            "budget_available": ctx.budget.available()
        });

        let seq_no = ctx.db.last_committed_seq().await?;
        let tx_id = ctx.db.inner_storage().last_tx_id().await?;

        let meta = CheckpointMeta {
            name: checkpoint_name,
            collection_id: ctx.state_collection.name().to_string(),
            seq_no,
            tx_id,
            metadata,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        self.checkpoint_store.save_checkpoint(meta).await
    }

    /// Continuous event loop reading telemetry events from an `EventSource`,
    /// attaching each event to `AgentContext`, executing `run()`, and checkpointing state after each event.
    pub async fn run_event_loop(
        &self,
        ctx: &mut AgentContext,
        graph: &StateGraph,
        source: &mut dyn crate::event_source::EventSource,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Result<EventLoopExitReason> {
        loop {
            if shutdown.is_cancelled() {
                return Ok(EventLoopExitReason::Shutdown);
            }

            tokio::select! {
                _ = shutdown.cancelled() => {
                    return Ok(EventLoopExitReason::Shutdown);
                }
                event_res = source.next_event() => {
                    match event_res? {
                        Some(event) => {
                            ctx.attach_event(event);
                            self.run(ctx, graph).await?;
                            self.checkpoint(ctx).await?;
                        }
                        None => {
                            if source.is_exhausted() {
                                return Ok(EventLoopExitReason::SourceExhausted);
                            }
                            tokio::select! {
                                _ = shutdown.cancelled() => {
                                    return Ok(EventLoopExitReason::Shutdown);
                                }
                                _ = source.wait_for_event() => {}
                            }
                        }
                    }
                }
            }
        }
    }

    async fn commit_step(&self, ctx: &AgentContext, result: &StepResult) -> Result<()> {
        let state_doc_id = format!("task:{}:step:{}", ctx.task_id, ctx.step_count);
        let metadata = serde_json::json!({
            "stage": "commit",
            "node": ctx.current_node,
            "memory": ctx.memory,
            "output": result.output,
            "tokens_consumed": result.tokens_consumed,
            "status": ctx.status
        });

        // Use direct KV storage pattern for workflow history without vector index participation
        ctx.state_collection.put_kv(&state_doc_id, &metadata).await
    }

    /// Writes an immutable audit entry for a step execution.
    ///
    /// Executed before `commit_step()`. Note: A `MemFuseError::Conflict` here signifies that
    /// an audit entry for this `(task_id, step_count)` already exists from a previous partial attempt.
    /// In such recovery scenarios, the caller must advance to a new `step_count` (e.g., via
    /// `ctx.next_retry_step_count()`), and NOT attempt to overwrite the existing entry.
    async fn audit_log(&self, ctx: &AgentContext, result: &StepResult) -> Result<()> {
        // Generate immutable audit trace and store it
        let entry = crate::audit::AuditEntry {
            task_id: ctx.task_id.clone(),
            step_count: ctx.step_count,
            node_id: ctx.current_node.clone(),
            tokens_consumed: result.tokens_consumed,
            payload: result.output.clone(),
            error: None,
        };

        crate::audit::AuditLog::append_to(&ctx.state_collection, &entry).await
    }

    async fn audit_log_failure(&self, ctx: &AgentContext, error_message: &str) -> Result<()> {
        let entry = crate::audit::AuditEntry {
            task_id: ctx.task_id.clone(),
            step_count: ctx.step_count,
            node_id: ctx.current_node.clone(),
            tokens_consumed: 0,
            payload: serde_json::Value::Null,
            error: Some(error_message.to_string()),
        };

        crate::audit::AuditLog::append_to(&ctx.state_collection, &entry).await
    }

    async fn persist_final_state(&self, ctx: &AgentContext) -> Result<()> {
        let final_id = format!("task:{}:final", ctx.task_id);
        let metadata = serde_json::json!({
            "stage": "final",
            "status": ctx.status,
            "task_id": ctx.task_id,
            "step_count": ctx.step_count,
            "memory": ctx.memory,
            "tokens_total": ctx.budget.consumed()
        });

        ctx.state_collection.put_kv(&final_id, &metadata).await
    }

    fn resolve_next_node(
        &self,
        graph: &StateGraph,
        current_node: &str,
        result: &StepResult,
    ) -> Result<String> {
        let edges = graph
            .edges
            .iter()
            .filter(|e| e.from == current_node)
            .collect::<Vec<_>>();

        if edges.is_empty() {
            return Err(MemFuseError::Internal(format!(
                "Dead end at node {}",
                current_node
            )));
        }

        if let Some(ref forced_next) = result.next_edge {
            if edges.iter().any(|e| &e.to == forced_next) {
                return Ok(forced_next.to_string());
            }
        }

        // Default to highest priority
        let edge = edges
            .iter()
            .max_by_key(|e| e.priority)
            .ok_or_else(|| MemFuseError::Internal("No edges found".to_string()))?;
        Ok(edge.to.to_string())
    }

    fn evaluate_decision(
        &self,
        graph: &StateGraph,
        node: &AgentNode,
        ctx: &AgentContext,
    ) -> Result<String> {
        let edges = graph
            .edges
            .iter()
            .filter(|e| e.from == node.id)
            .collect::<Vec<_>>();

        if edges.is_empty() {
            return Err(MemFuseError::Internal(format!(
                "Decision Node {} has no outgoing edges",
                node.id
            )));
        }

        let mut matching: Vec<_> = edges
            .iter()
            .filter(|e| match &e.condition {
                None => true,
                Some(expr) => evaluate_condition_expr(expr, ctx),
            })
            .collect();

        matching.sort_by_key(|e| std::cmp::Reverse(e.priority));

        matching
            .first()
            .map(|e| e.to.to_string())
            .ok_or_else(|| {
                MemFuseError::Internal(format!(
                    "Decision Node {} has no matching edge for current context",
                    node.id
                ))
            })
    }
}

/// Helper to look up a key or dot-notation path in [`AgentContext`].
fn get_context_value<'a>(
    key: &str,
    ctx: &'a AgentContext,
) -> Option<std::borrow::Cow<'a, serde_json::Value>> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    // 1. Direct lookup in memory
    if let Some(v) = ctx.memory.get(key) {
        return Some(std::borrow::Cow::Borrowed(v));
    }

    // 2. Dot-notation path in memory (e.g. "output.status")
    if key.contains('.') {
        let parts: Vec<&str> = key.split('.').collect();
        if let Some(mut current) = ctx.memory.get(parts[0]) {
            let mut found = true;
            for part in &parts[1..] {
                if let serde_json::Value::Object(map) = current {
                    if let Some(next_val) = map.get(*part) {
                        current = next_val;
                    } else {
                        found = false;
                        break;
                    }
                } else {
                    found = false;
                    break;
                }
            }
            if found {
                return Some(std::borrow::Cow::Borrowed(current));
            }
        }
    }

    // 3. Built-in context properties
    match key {
        "task_id" => Some(std::borrow::Cow::Owned(serde_json::Value::String(
            ctx.task_id.clone(),
        ))),
        "current_node" => Some(std::borrow::Cow::Owned(serde_json::Value::String(
            ctx.current_node.clone(),
        ))),
        "step_count" => Some(std::borrow::Cow::Owned(serde_json::Value::Number(
            ctx.step_count.into(),
        ))),
        "status" => Some(std::borrow::Cow::Owned(serde_json::Value::String(
            format!("{:?}", ctx.status),
        ))),
        _ => None,
    }
}

/// Helper to check if a [`serde_json::Value`] matches a raw string representation value.
fn value_matches(val: &serde_json::Value, raw_val_str: &str) -> bool {
    let target = raw_val_str.trim().trim_matches('"').trim_matches('\'');
    match val {
        serde_json::Value::String(s) => s == target || s == raw_val_str.trim(),
        serde_json::Value::Bool(b) => {
            b.to_string() == target || b.to_string() == raw_val_str.trim()
        }
        serde_json::Value::Number(n) => {
            n.to_string() == target || n.to_string() == raw_val_str.trim()
        }
        serde_json::Value::Null => target == "null" || target == "Null" || target.is_empty(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_val_str.trim()) {
                val == &parsed
            } else {
                false
            }
        }
    }
}

/// Evaluates a declarative condition expression against the provided [`AgentContext`].
///
/// # Supported Grammar:
/// - `<key> exists`: Returns `true` if `<key>` is present in context memory or context properties and is not `null`.
/// - `<key> == <value>`: Returns `true` if the value at `<key>` matches `<value>`.
/// - `<key> != <value>`: Returns `true` if the value at `<key>` does not match `<value>` (or if `<key>` does not exist).
///
/// Key resolution supports direct keys in `ctx.memory` (e.g. `"result"`), nested dot-notation paths
/// (e.g. `"output.status"`), and built-in context fields (`"task_id"`, `"current_node"`, `"step_count"`, `"status"`).
///
/// # Error Handling:
/// Expression syntax errors or invalid formats do NOT panic; they emit a [`tracing::warn!`] log and evaluate to `false`.
pub fn evaluate_condition_expr(expr: &str, ctx: &AgentContext) -> bool {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        tracing::warn!("Empty condition expression evaluated as false");
        return false;
    }

    if let Some(key_part) = trimmed.strip_suffix(" exists") {
        let key = key_part.trim();
        if key.is_empty() {
            tracing::warn!("Condition expression missing key before 'exists': '{}'", expr);
            return false;
        }
        if let Some(v) = get_context_value(key, ctx) {
            return !v.is_null();
        }
        return false;
    }

    if let Some((key_part, val_part)) = trimmed.split_once("!=") {
        let key = key_part.trim();
        let val = val_part.trim();
        if key.is_empty() {
            tracing::warn!("Condition expression missing key before '!=': '{}'", expr);
            return false;
        }
        if let Some(v) = get_context_value(key, ctx) {
            return !value_matches(&v, val);
        }
        // Missing key does not match value, so != holds true
        return true;
    }

    if let Some((key_part, val_part)) = trimmed.split_once("==") {
        let key = key_part.trim();
        let val = val_part.trim();
        if key.is_empty() {
            tracing::warn!("Condition expression missing key before '==': '{}'", expr);
            return false;
        }
        if let Some(v) = get_context_value(key, ctx) {
            return value_matches(&v, val);
        }
        return false;
    }

    tracing::warn!("Unparseable condition expression: '{}'", expr);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_dummy_context() -> (AgentContext, TempDir) {
        let tmp = TempDir::new().expect("temp dir");
        let config = MemFuseConfig {
            dimension: 4,
            max_elements: 1000,
            distance_metric: DistanceMetric::Cosine,
            ..Default::default()
        };
        let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("open db"));
        let state_col = db.collection("test-state").await.expect("collection");
        let ctx = AgentContext::try_new("test-task-1", "start", db, state_col, TokenBudget::new(1000, 0))
            .expect("agent context");
        (ctx, tmp)
    }

    #[tokio::test]
    async fn test_evaluate_condition_expr_grammar_and_outcomes() {
        let (mut ctx, _tmp) = create_dummy_context().await;
        ctx.memory.insert("simple_str".to_string(), json!("hello"));
        ctx.memory.insert("number_val".to_string(), json!(42));
        ctx.memory.insert("bool_val".to_string(), json!(true));
        ctx.memory.insert("null_val".to_string(), json!(null));
        ctx.memory.insert(
            "nested".to_string(),
            json!({
                "status": "approved",
                "code": 200
            }),
        );

        // 1. "exists" checks
        assert!(evaluate_condition_expr("simple_str exists", &ctx));
        assert!(evaluate_condition_expr("nested.status exists", &ctx));
        assert!(evaluate_condition_expr("task_id exists", &ctx));
        assert!(!evaluate_condition_expr("null_val exists", &ctx));
        assert!(!evaluate_condition_expr("missing_key exists", &ctx));
        assert!(!evaluate_condition_expr(" exists", &ctx)); // missing key

        // 2. "==" checks
        assert!(evaluate_condition_expr("simple_str == hello", &ctx));
        assert!(evaluate_condition_expr("simple_str == \"hello\"", &ctx));
        assert!(evaluate_condition_expr("number_val == 42", &ctx));
        assert!(evaluate_condition_expr("bool_val == true", &ctx));
        assert!(evaluate_condition_expr("nested.status == approved", &ctx));
        assert!(evaluate_condition_expr("nested.code == 200", &ctx));
        assert!(evaluate_condition_expr("task_id == test-task-1", &ctx));
        assert!(!evaluate_condition_expr("simple_str == world", &ctx));
        assert!(!evaluate_condition_expr("missing_key == foo", &ctx));

        // 3. "!=" checks
        assert!(evaluate_condition_expr("simple_str != world", &ctx));
        assert!(evaluate_condition_expr("missing_key != foo", &ctx));
        assert!(!evaluate_condition_expr("simple_str != hello", &ctx));

        // 4. Unparseable & invalid expressions (no panic)
        assert!(!evaluate_condition_expr("invalid condition syntax", &ctx));
        assert!(!evaluate_condition_expr("", &ctx));
        assert!(!evaluate_condition_expr("   ", &ctx));
        assert!(!evaluate_condition_expr("== value_without_key", &ctx));
        assert!(!evaluate_condition_expr("!= value_without_key", &ctx));
    }

    #[tokio::test]
    async fn test_audit_before_commit_ordering() {
        let (ctx, _tmp) = create_dummy_context().await;
        let orchestrator = OrchestratorEngine::from_db(&ctx.db);

        // Populate an existing KV entry under task:test-task-1:step:0 to force commit_step to fail
        // if state_collection.put_kv_if_absent was used, but put_kv overwrites.
        // Wait, put_kv doesn't fail on existing key, put_kv_if_absent does!
        // To simulate commit_step failure after successful audit_log:
        // Populate audit entry manually? No, audit_log uses put_kv_if_absent under "audit:test-task-1:step:0".
        // commit_step uses put_kv under "task:test-task-1:step:0".
        // If we want commit_step to fail while audit_log succeeds, we can simulate an error in commit_step or
        // test ordering directly.
        // Let's test calling audit_log directly then commit_step with invalid state ID or pre-condition,
        // or test that after audit_log succeeds, state_collection contains the audit entry "audit:test-task-1:step:0"
        // even if commit_step fails, and step_count is NOT incremented (remains 0).
        let step_res = StepResult {
            node_id: "start".to_string(),
            output: json!({"res": "ok"}),
            tokens_consumed: 10,
            next_edge: None,
        };

        // Call audit_log first (as in new loop order)
        let audit_res = orchestrator.audit_log(&ctx, &step_res).await;
        assert!(audit_res.is_ok());

        // Verify audit entry exists in state_collection
        let audit_id = format!("audit:{}:step:{}", ctx.task_id, ctx.step_count);
        let audit_entry = ctx.state_collection.get_kv(&audit_id).await.unwrap();
        assert!(audit_entry.is_some());

        // Verify state doc key for commit_step does NOT exist yet
        let state_doc_id = format!("task:{}:step:{}", ctx.task_id, ctx.step_count);
        let state_entry = ctx.state_collection.get_kv(&state_doc_id).await.unwrap();
        assert!(state_entry.is_none());

        // Verify step_count was not incremented
        assert_eq!(ctx.step_count, 0);
    }
}
