//! Deterministic, persistent graph-walker engine for agent workflows.
//!
//! Implements the core execution loop: checkpoint → execute → commit → audit → resolve-next.

use crate::context::AgentContext;
use crate::graph::{AgentNode, NodeType, StateGraph};
use crate::step::{AgentTool, StepResult};
use memfuse_checkpoint::{CheckpointMeta, CheckpointRegistry, PersistentCheckpointStore};
use memfuse_core::traits::StorageEngine;
use memfuse_core::{MemFuseError, Result};
use memfuse_store::LsmStorage;
use std::collections::HashMap;
use std::sync::Arc;

/// Async executor engine applying nodes in Sequence.
pub struct OrchestratorEngine {
    pub tools: HashMap<String, Box<dyn AgentTool>>,
    pub checkpoint_store: Arc<dyn CheckpointRegistry>,
}

impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        Self {
            tools: HashMap::new(),
            checkpoint_store: Arc::new(PersistentCheckpointStore::new(storage, "agent")),
        }
    }

    pub fn register_tool(&mut self, tool: Box<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
        ctx.status = crate::context::AgentStatus::Running;
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
                    // 1. Checkpoint BEFORE execution (AC-1)
                    self.checkpoint(ctx).await?;

                    // 2. Resolve handler (Optional for Start nodes)
                    let result = if let Some(handler_name) = &node.handler {
                        let tool = self.tools.get(handler_name).ok_or_else(|| {
                            MemFuseError::Internal(format!("Tool {} not registered", handler_name))
                        })?;

                        let input = ctx
                            .memory
                            .get("last_output")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);

                        let res = tool.execute(ctx, input).await?;

                        // Append to context memory
                        ctx.memory
                            .insert("last_output".to_string(), res.output.clone());

                        res
                    } else if node.node_type == NodeType::Start {
                        // Pass-through result for start nodes without handlers
                        StepResult {
                            node_id: node.id.clone(),
                            output: serde_json::Value::Null,
                            tokens_consumed: 0,
                            next_edge: None,
                        }
                    } else {
                        return Err(MemFuseError::Internal(format!(
                            "Task Node {} lacks handler",
                            node.id
                        )));
                    };

                    // 3. Atomic commit to LSM
                    self.commit_step(ctx, &result).await?;

                    // 4. Audit log (AC-3)
                    self.audit_log(ctx, &result).await?;

                    // 5. Consume tokens and check budget
                    ctx.budget.consume(result.tokens_consumed);
                    if ctx.budget.available() == 0 && node.node_type != NodeType::Start {
                        return Err(MemFuseError::Internal("Token budget exhausted".to_string()));
                    }

                    // 6. Resolve next edge
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
    pub async fn replay_from(&self, ctx: &mut AgentContext, identifier: &str) -> Result<()> {
        let checkpoints = self.checkpoint_store.list_checkpoints().await?;

        let checkpoint = checkpoints
            .iter()
            .rfind(|c| {
                if !c.name.starts_with(&format!("task:{}:", ctx.task_id)) {
                    return false;
                }
                if let Ok(step) = identifier.parse::<u64>() {
                    c.name.contains(&format!(":step:{}:", step))
                } else {
                    c.name.ends_with(&format!(":node:{}", identifier))
                }
            })
            .ok_or_else(|| {
                MemFuseError::Internal(format!(
                    "Checkpoint '{}' für Task '{}' nicht gefunden",
                    identifier, ctx.task_id
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
        if let Some(memory) = checkpoint
            .metadata
            .get("memory")
            .and_then(|v| v.as_object())
        {
            ctx.memory = memory.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
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

        // Use a metadata-only storage pattern for workflow history (zero-vector)
        let zero_vec = vec![0.0; ctx.state_collection.dimension()];
        ctx.state_collection
            .insert(&state_doc_id, &zero_vec, Some(metadata))
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

        let zero_vec = vec![0.0; ctx.state_collection.dimension()];
        ctx.state_collection
            .insert(&final_id, &zero_vec, Some(metadata))
            .await
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
