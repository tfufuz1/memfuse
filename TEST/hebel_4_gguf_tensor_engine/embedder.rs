// SPEC-041 §2: IngestEmbedder — IndexObserver integration
//
// Hooks into the SyncManager 2PC pipeline via `IndexObserver::on_prepare()`.
//
// # Data Flow
//
//   SyncManager::commit(tx)
//     └─► for each observer: on_prepare(ctx, ns, tx)
//           └─► IngestEmbedder::on_prepare()
//                 ├── Extract RawContent from TxBuffer<EmbedTask>
//                 ├── Provider::embed(content) [spawn_blocking inside provider]
//                 └── Stage Embedding in VectorIndex TxBuffer
//
// # INVARIANTS
//   INV-S5: spawn_blocking used inside GgufEmbeddingModel::embed()
//   INV-SEC5: NamespaceId passed through to VectorIndex staging
//   INV-C1: Embedding must be staged before on_commit is called on VectorIndex

use crate::autolinker::AutoLinker;
use crate::config::ComputeConfig;
use async_trait::async_trait;
use chimera_core::budget::{BudgetStatus, Domain, ResourceTracker};
use chimera_core::context::ChimeraContext;
use chimera_core::error::{ChimeraError, Result};
use chimera_core::traits::{EmbeddingProvider, IndexObserver};
use chimera_core::types::{DocId, MemoryTier, NamespaceId, RawContent, TxId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::instrument;

/// Staged embedding task waiting for 2PC prepare.
#[derive(Debug, Clone)]
pub struct EmbedTask {
    pub doc_id: DocId,
    pub content: RawContent,
    pub tier: MemoryTier,
}

/// Staging buffer: maps `TxId` to pending embed tasks.
type EmbedBuffer = Arc<RwLock<HashMap<TxId, Vec<EmbedTask>>>>;

/// The `IngestEmbedder` glues `EmbeddingProvider` into the 2PC pipeline.
pub struct IngestEmbedder {
    provider: Arc<dyn EmbeddingProvider>,
    linker: Option<Arc<dyn AutoLinker>>,
    tracker: Arc<ResourceTracker>,
    config: ComputeConfig,
    /// Buffer: TxId → Vec<EmbedTask>
    buffer: EmbedBuffer,
    /// Throttling: Limits concurrent embedding computations to avoid OOM/CPU saturation.
    semaphore: Arc<Semaphore>,
}

impl IngestEmbedder {
    /// Creates a new IngestEmbedder wrapping the given embedding provider.
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        linker: Option<Arc<dyn AutoLinker>>,
        tracker: Arc<ResourceTracker>,
        config: ComputeConfig,
    ) -> Self {
        let concurrency = if config.max_concurrent_tasks == 0 {
            // Auto: Use half of available parallelism (max 1 for small CPUs, more for larger)
            std::thread::available_parallelism()
                .map(|p| (p.get() / 2).max(1))
                .unwrap_or(1)
        } else {
            config.max_concurrent_tasks
        };

        Self {
            provider,
            linker,
            tracker,
            config,
            buffer: Arc::new(RwLock::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(concurrency)),
        }
    }

    /// Stages a raw content item for embedding under a given transaction.
    pub fn stage(&self, tx: TxId, doc_id: DocId, content: RawContent, tier: MemoryTier) {
        self.buffer.write().entry(tx).or_default().push(EmbedTask {
            doc_id,
            content,
            tier,
        });
    }

    /// Discards all staged tasks for a transaction (called on rollback or timeout).
    fn discard(&self, tx: TxId) {
        self.buffer.write().remove(&tx);
    }

    /// Drains all staged tasks for a transaction.
    fn drain(&self, tx: TxId) -> Vec<EmbedTask> {
        self.buffer.write().remove(&tx).unwrap_or_default()
    }
}

#[async_trait]
impl chimera_core::traits::IdempotentApply for IngestEmbedder {
    async fn apply_idempotent(
        &self,
        _ctx: &ChimeraContext,
        _ns: &NamespaceId,
        _tx: TxId,
        _payload: &[u8],
    ) -> Result<()> {
        // IngestEmbedder doesn't need to replay as embeddings are stored in VectorIndex.
        Ok(())
    }
}

#[async_trait]
impl IndexObserver for IngestEmbedder {
    #[instrument(skip(self), fields(tx_id = tx.inner(), ns = %ns))]
    async fn on_prepare(&self, ctx: &ChimeraContext, ns: &NamespaceId, tx: TxId) -> Result<()> {
        let budget_status = self.tracker.status();

        // 1. Budget pre-check
        if matches!(budget_status, BudgetStatus::Reject) {
            return Err(ChimeraError::BudgetExceeded {
                resource: "compute",
                used: self.tracker.memory_used(),
                limit: self.tracker.domain_limit(Domain::Compute),
            });
        }

        // 2. Drain staged tasks for this transaction
        let tasks = self.drain(tx);
        if tasks.is_empty() {
            return Ok(());
        }

        // 3. Adaptive Tier Handling & Throttling
        let mut working_docs = Vec::new();
        let mut working_content = Vec::new();
        let mut episodic_docs = Vec::new();
        let mut episodic_content = Vec::new();
        let mut semantic_docs = Vec::new();
        let mut semantic_content = Vec::new();

        for task in tasks {
            match task.tier {
                MemoryTier::Working => {
                    working_docs.push(task.doc_id);
                    working_content.push(task.content);
                }
                MemoryTier::Episodic => {
                    episodic_docs.push(task.doc_id);
                    episodic_content.push(task.content);
                }
                MemoryTier::Semantic => {
                    semantic_docs.push(task.doc_id);
                    semantic_content.push(task.content);
                }
            }
        }

        let total_tasks = working_docs.len() + episodic_docs.len() + semantic_docs.len();
        let mut all_embeddings = Vec::with_capacity(total_tasks);
        let mut all_doc_ids = Vec::with_capacity(total_tasks);

        // WCET Guard: Total timeout for all embeddings in this prepare phase.
        let total_timeout = Duration::from_millis(self.config.embedding_timeout_ms);

        let result: Result<()> =
            timeout(total_timeout, async {
                // High Priority: Working (Volatile, In-Memory only)
                if !working_content.is_empty() {
                    // Working tier always gets a permit if available, or stalls.
                    let _permit =
                        self.semaphore.acquire().await.map_err(|_| {
                            ChimeraError::Compute("Compute semaphore closed".into())
                        })?;
                    let embeddings = self.provider.embed_batch(&working_content).await?;
                    all_doc_ids.extend(working_docs);
                    all_embeddings.extend(embeddings);
                }

                // Medium Priority: Episodic (Action History)
                if !episodic_content.is_empty() {
                    // If in Stall mode, we add extra latency to discourage heavy episodic ingest.
                    if matches!(budget_status, BudgetStatus::Stall) {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }

                    let _permit =
                        self.semaphore.acquire().await.map_err(|_| {
                            ChimeraError::Compute("Compute semaphore closed".into())
                        })?;
                    let embeddings = self.provider.embed_batch(&episodic_content).await?;
                    all_doc_ids.extend(episodic_docs);
                    all_embeddings.extend(embeddings);
                }

                // Low Priority: Semantic (Long-term Knowledge)
                if !semantic_content.is_empty() {
                    // In Stall mode, we might even skip semantic embeddings for some requests
                    // or severely throttle them. For now, we just enforce the semaphore.
                    if matches!(budget_status, BudgetStatus::Stall) {
                        tracing::warn!(
                            tx_id = tx.inner(),
                            "Memory pressure: Semantic embedding throttled"
                        );
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }

                    let _permit =
                        self.semaphore.acquire().await.map_err(|_| {
                            ChimeraError::Compute("Compute semaphore closed".into())
                        })?;
                    let embeddings = self.provider.embed_batch(&semantic_content).await?;
                    all_doc_ids.extend(semantic_docs);
                    all_embeddings.extend(embeddings);
                }

                Ok(())
            })
            .await
            .map_err(|_| {
                ChimeraError::Compute(format!(
                    "Embedding timed out after {}ms (WCET Guard)",
                    self.config.embedding_timeout_ms
                ))
            })?;

        result?;

        // 4. Log + metrics
        tracing::debug!(
            tx_id = tx.inner(),
            count = all_embeddings.len(),
            model = self.provider.model_name(),
            "IngestEmbedder: embeddings generated with 3-tier prioritization"
        );
        metrics::counter!(
            "chimera_compute_embeddings_total",
            "phase" => "on_prepare"
        )
        .increment(all_embeddings.len() as u64);

        // 5. Bounded AutoLinking (SPEC-041)
        if let Some(linker) = &self.linker {
            for (i, embedding) in all_embeddings.iter().enumerate() {
                linker
                    .auto_link(ctx, ns, tx, all_doc_ids[i], embedding)
                    .await?;
            }
        }

        // 6. Store generated embeddings in context for downstream VectorIndex
        for (i, embedding) in all_embeddings.iter().enumerate() {
            let key = format!("compute:embedding:{i}");
            let bytes: Vec<u8> = embedding
                .as_slice()
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            let encoded = base64_encode(&bytes);
            ctx.set_metadata(key, encoded);
        }
        ctx.set_metadata(
            "compute:embedding_count".to_string(),
            all_embeddings.len().to_string(),
        );
        ctx.set_metadata(
            "compute:dimension".to_string(),
            self.provider.dimension().to_string(),
        );

        Ok(())
    }

    async fn on_commit(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        // Embeddings are committed by the VectorIndex observer.
        // We only clean up any residual staging state here.
        self.discard(tx);
        Ok(())
    }

    async fn on_rollback(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        self.discard(tx);
        metrics::counter!("chimera_compute_rollbacks_total").increment(1);
        Ok(())
    }

    fn serialize_pending_ops(&self, _ns: &NamespaceId, _tx: TxId) -> Vec<u8> {
        Vec::new()
    }

    fn get_involved_docs(&self, _ns: &NamespaceId, _tx: TxId) -> Vec<chimera_core::QualifiedDocId> {
        Vec::new()
    }
}

/// Minimal base64 encoder (no external dependency — avoids pulling in base64 crate).
/// Only used for embedding transport through ChimeraContext metadata.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let v = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARS[(v >> 18) as usize] as char);
        out.push(CHARS[((v >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((v >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(v & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MockEmbeddingModel;
    use chimera_core::budget::ResourceBudget;
    use chimera_core::context::ChimeraContext;
    use chimera_core::types::{AgentId, NamespaceId};

    fn make_embedder(dim: usize) -> IngestEmbedder {
        let model = Arc::new(MockEmbeddingModel::new(dim));
        let tracker = Arc::new(ResourceTracker::unlimited());
        let config = ComputeConfig {
            max_concurrent_tasks: 2,
            ..Default::default()
        };
        IngestEmbedder::new(model, None, tracker, config)
    }

    #[tokio::test]
    async fn test_on_prepare_stages_embedding_into_context() -> Result<()> {
        let embedder = make_embedder(32);
        let tx = TxId::new(42);
        let ns = NamespaceId::default_ns();
        let ctx = ChimeraContext::with_ids(AgentId::new(1), ns.clone());

        embedder.stage(
            tx,
            DocId::new(1),
            RawContent::Text("Hello ChimeraDB".into()),
            MemoryTier::Working,
        );
        embedder.on_prepare(&ctx, &ns, tx).await?;

        // Should have written embedding count + base64-encoded embedding to ctx
        let count = ctx
            .get_metadata("compute:embedding_count")
            .ok_or_else(|| ChimeraError::Internal("Missing embedding count in ctx".into()))?
            .parse::<usize>()
            .map_err(|e| ChimeraError::Internal(format!("Parse error: {e}")))?;
        assert_eq!(count, 1);
        assert!(ctx.get_metadata("compute:embedding:0").is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_on_prepare_noop_when_no_staged_content() -> Result<()> {
        let embedder = make_embedder(32);
        let tx = TxId::new(99);
        let ns = NamespaceId::default_ns();
        let ctx = ChimeraContext::default();

        // No staging → on_prepare should be a no-op
        embedder.on_prepare(&ctx, &ns, tx).await?;
        assert!(ctx.get_metadata("compute:embedding_count").is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_on_rollback_discards_staged_tasks() -> Result<()> {
        let embedder = make_embedder(16);
        let tx = TxId::new(7);
        let ns = NamespaceId::default_ns();
        let ctx = ChimeraContext::default();

        embedder.stage(
            tx,
            DocId::new(1),
            RawContent::Text("test".into()),
            MemoryTier::Episodic,
        );
        embedder.on_rollback(&ctx, &ns, tx).await?;

        // Buffer should be empty after rollback
        assert!(embedder.buffer.read().get(&tx).is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_budget_reject_blocks_prepare() {
        let model = Arc::new(MockEmbeddingModel::new(8));
        // Create a tracker with zero memory budget → Reject on any prepare
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget {
            memory_limit: 1, // 1 byte — will be Reject immediately
            cpu_cycle_limit: u64::MAX,
        }));
        // Fill the budget
        let _ = tracker.consume_memory(1);

        let config = ComputeConfig {
            max_concurrent_tasks: 1,
            ..Default::default()
        };
        let embedder = IngestEmbedder::new(model, None, tracker, config);
        let tx = TxId::new(1);
        let ns = NamespaceId::default_ns();
        let ctx = ChimeraContext::default();
        embedder.stage(
            tx,
            DocId::new(1),
            RawContent::Text("test".into()),
            MemoryTier::Semantic,
        );

        let result = embedder.on_prepare(&ctx, &ns, tx).await;
        assert!(
            matches!(result, Err(ChimeraError::BudgetExceeded { .. })),
            "Expected BudgetExceeded, got: {result:?}"
        );
    }
}
