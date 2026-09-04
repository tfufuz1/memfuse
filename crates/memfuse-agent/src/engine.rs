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
use crate::graph::{AgentNode, NodeType, StateGraph};
use crate::step::{AgentTool, StepResult};
use memfuse_checkpoint::{
    CheckpointGuard, CheckpointMeta, CheckpointRegistry, PersistentCheckpointStore,
};
use memfuse_core::traits::StorageEngine;
use memfuse_core::{MemFuseError, Result};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

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
}

impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        let store = PersistentCheckpointStore::new(storage, "agent")
            .unwrap_or_else(|e| panic!("Failed to initialize PersistentCheckpointStore: {e}"));
        Self {
            tools: HashMap::new(),
            checkpoint_store: Arc::new(
                PersistentCheckpointStore::new(storage, "agent")
                    .expect("Failed to initialize PersistentCheckpointStore for agent"),
            ),
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
                            self.audit_log_failure(ctx, &err.to_string()).await?;
                            return Err(err);
                        }
                    }

                    // 2. Resolve handler (Optional for Start nodes)
                    let result_res = if let Some(handler_name) = &node.handler {
                        if let Some(tool) = self.tools.get(handler_name) {
                            tool.execute(ctx, input).await
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

                    // 3. Atomic commit to LSM
                    self.commit_step(ctx, &result).await?;

                    // 4. Audit log (AC-3)
                    self.audit_log(ctx, &result).await?;

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
                                _ = source.wait_until_ready() => {}
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
        _ctx: &AgentContext,
    ) -> Result<String> {
        // Find outgoing edges, sort by priority
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

        let edge = edges
            .iter()
            .max_by_key(|e| e.priority)
            .ok_or_else(|| MemFuseError::Internal("No edges found".to_string()))?;
        Ok(edge.to.to_string())
    }
}
