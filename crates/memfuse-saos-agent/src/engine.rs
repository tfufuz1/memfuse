//! Deterministic, persistent graph-walker engine for agent workflows.
//!
//! Implements the core execution loop: checkpoint → execute → commit → audit → resolve-next.

use crate::context::AgentContext;
use crate::graph::{AgentNode, NodeType, StateGraph};
use crate::step::{AgentTool, StepResult};
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{MemFuseError, Result};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

/// Async executor engine applying nodes in Sequence.
pub struct OrchestratorEngine {
    pub tools: HashMap<String, Box<dyn AgentTool>>,
    pub checkpoint_store: Arc<PersistentCheckpointStore<LsmStorage>>,
}

impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        Self {
            tools: HashMap::new(),
            checkpoint_store: Arc::new(PersistentCheckpointStore::new(storage)),
        }
    }

    pub fn register_tool(&mut self, tool: Box<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
        loop {
            let node = graph.get_node(&ctx.current_node).ok_or_else(|| {
                MemFuseError::Internal(format!("Node {} not found", ctx.current_node))
            })?;

            match node.node_type {
                NodeType::End => {
                    self.persist_final_state(ctx).await?;
                    return Ok(());
                }
                NodeType::Start | NodeType::Task => {
                    // 1. Checkpoint BEFORE execution (AC-1)
                    self.checkpoint(ctx).await?;

                    // 2. Execute the registered tool
                    let handler_name = node.handler.as_ref().ok_or_else(|| {
                        MemFuseError::Internal(format!("Task Node {} lacks handler", node.id))
                    })?;

                    let tool = self.tools.get(handler_name).ok_or_else(|| {
                        MemFuseError::Internal(format!("Tool {} not registered", handler_name))
                    })?;

                    let input = ctx
                        .memory
                        .get("last_output")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    let result = tool.execute(ctx, input).await?;

                    // 3. Append to context memory
                    ctx.memory
                        .insert("last_output".to_string(), result.output.clone());

                    // 4. Atomic commit to LSM
                    self.commit_step(ctx, &result).await?;

                    // 5. Audit log (AC-3)
                    self.audit_log(ctx, &result).await?;

                    // 6. Consume tokens and check budget
                    ctx.budget.consume(result.tokens_consumed);
                    if ctx.budget.available() == 0 {
                        return Err(MemFuseError::Internal("Token budget exhausted".to_string()));
                    }

                    // 7. Resolve next edge
                    ctx.current_node = self.resolve_next_node(graph, &ctx.current_node, &result)?;
                    ctx.step_count += 1;
                }
                NodeType::Decision => {
                    let next = self.evaluate_decision(graph, node, ctx)?;
                    ctx.current_node = next;
                }
            }
        }
    }

    /// Setzt den AgentContext auf einen früheren Checkpoint zurück (AC-2).
    pub async fn replay_from(&self, ctx: &mut AgentContext, step_name: &str) -> Result<()> {
        let checkpoint_name = format!("task:{}:before:{}", ctx.task_id, step_name);
        let checkpoint = self
            .checkpoint_store
            .get_checkpoint(&checkpoint_name)
            .await?
            .ok_or_else(|| {
                MemFuseError::Internal(format!("Checkpoint {} not found", checkpoint_name))
            })?;

        // Restore state from checkpoint metadata
        if let Some(current_node) = checkpoint
            .metadata
            .get("current_node")
            .and_then(|v| v.as_str())
        {
            ctx.current_node = current_node.to_string();
        }
        if let Some(memory) = checkpoint
            .metadata
            .get("memory")
            .and_then(|v| v.as_object())
        {
            ctx.memory = memory.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        }
        // In a real implementation, we would also need to revert the storage to checkpoint.seq_no
        Ok(())
    }

    async fn checkpoint(&self, ctx: &AgentContext) -> Result<()> {
        let checkpoint_name = format!("task:{}:before:{}", ctx.task_id, ctx.current_node);
        let metadata = serde_json::json!({
            "current_node": ctx.current_node,
            "step_count": ctx.step_count,
            "memory": ctx.memory,
            "budget_available": ctx.budget.available()
        });

        let seq_no = ctx.db.last_committed_seq().await?;
        self.checkpoint_store
            .create_checkpoint(
                &checkpoint_name,
                ctx.state_collection.name(),
                seq_no,
                metadata,
            )
            .await?;
        Ok(())
    }

    async fn commit_step(&self, ctx: &AgentContext, result: &StepResult) -> Result<()> {
        let state_doc_id = format!("task:{}:step:{}", ctx.task_id, ctx.step_count);
        let metadata = serde_json::json!({
            "stage": "commit",
            "node": ctx.current_node,
            "memory": ctx.memory,
            "output": result.output,
            "tokens_consumed": result.tokens_consumed
        });

        let dummy_vec = vec![0.0; ctx.state_collection.dimension()];
        ctx.state_collection
            .insert(&state_doc_id, &dummy_vec, Some(metadata))
            .await
    }

    async fn audit_log(&self, ctx: &AgentContext, result: &StepResult) -> Result<()> {
        // Generate immutable audit trace and store it
        let entry = crate::audit::AuditEntry {
            task_id: ctx.task_id.clone(),
            step_count: ctx.step_count,
            node_id: ctx.current_node.clone(),
            tokens_consumed: result.tokens_consumed,
            payload: result.output.clone(),
        };

        let audit_log = crate::audit::AuditLog::new(ctx.state_collection.clone());
        audit_log.append(&entry).await
    }

    async fn persist_final_state(&self, _ctx: &AgentContext) -> Result<()> {
        Ok(())
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
