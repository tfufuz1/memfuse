//! SPEC-041: Bounded Eager AutoLinking (Cognitive OS)
//!
//! Automatically generates semantic edges in the graph index during ingest
//! based on vector similarity.

use async_trait::async_trait;
use chimera_core::context::ChimeraContext;
use chimera_core::error::Result;
use chimera_core::traits::{GraphIndex, VectorIndex};
use chimera_core::types::{DocId, Edge, EdgeId, Embedding, NamespaceId, TxId};
use chimera_rt::{Deadline, TimeoutGuard};
use std::sync::Arc;
use std::time::Duration;

/// MIN_CONFIDENCE threshold for automatic edge creation (INV-COG2).
pub const MIN_CONFIDENCE: f32 = 0.85;

/// AutoLinker trait for generating semantic edges during ingest.
#[async_trait]
pub trait AutoLinker: Send + Sync {
    /// Performs automatic linking for a newly embedded document.
    ///
    /// # Arguments
    /// * `ctx` - The chimera context
    /// * `ns` - The namespace ID (strict isolation enforced)
    /// * `tx` - The transaction ID
    /// * `doc_id` - The document ID being linked
    /// * `embedding` - The generated embedding for the document
    async fn auto_link(
        &self,
        ctx: &ChimeraContext,
        ns: &NamespaceId,
        tx: TxId,
        doc_id: DocId,
        embedding: &Embedding,
    ) -> Result<()>;
}

/// Implementation of Bounded Eager AutoLinking.
pub struct BoundedAutoLinker {
    vector_index: Arc<dyn VectorIndex>,
    graph_index: Arc<dyn GraphIndex>,
    max_neighbors: usize,
    timeout: Duration,
}

#[async_trait]
impl AutoLinker for BoundedAutoLinker {
    async fn auto_link(
        &self,
        ctx: &ChimeraContext,
        _ns: &NamespaceId,
        tx: TxId,
        doc_id: DocId,
        embedding: &Embedding,
    ) -> Result<()> {
        let deadline = Deadline::from_duration(self.timeout).map_err(|_| {
            chimera_core::error::ChimeraError::Internal("Failed to create deadline".into())
        })?;
        let guard = TimeoutGuard::new(&deadline);

        // 1. Perform KNN search on HNSW index (INV-SEC5: context handles namespace)
        // INV-COG1: Use tokio::time::timeout for hard cut-off during the search.
        let neighbors = tokio::time::timeout(
            deadline.remaining(),
            self.vector_index
                .search(ctx, embedding, self.max_neighbors, None),
        )
        .await
        .map_err(|_| {
            tracing::warn!(
                doc_id = doc_id.inner(),
                "AutoLinking search timed out (INV-COG1)"
            );
            chimera_core::error::ChimeraError::Compute("AutoLinking search timeout".into())
        })??;

        // 2. Filter and create edges (INV-COG1: check timeout)
        const EDGE_TYPE: &str = "semantic_similarity";

        for neighbor in neighbors {
            if let Err(_e) = guard.check() {
                tracing::warn!(
                    doc_id = doc_id.inner(),
                    "AutoLinking edge insertion timed out (INV-COG1)"
                );
                break;
            }

            // INV-COG2: Confidence Score > MIN_CONFIDENCE
            if neighbor.score > MIN_CONFIDENCE {
                // Prevent self-linking
                if neighbor.doc_id == doc_id {
                    continue;
                }

                // Create semantic edge in graph index
                let edge = Edge::new(
                    EdgeId::new(rand::random()),
                    chimera_core::types::EntityId::new(doc_id.inner()),
                    chimera_core::types::EntityId::new(neighbor.doc_id.inner()),
                    EDGE_TYPE.to_string(),
                )
                .with_weight(neighbor.score);

                self.graph_index.insert_edge(ctx, tx, doc_id, &edge).await?;
            }
        }

        Ok(())
    }
}

impl BoundedAutoLinker {
    /// Creates a new BoundedAutoLinker.
    pub fn new(
        vector_index: Arc<dyn VectorIndex>,
        graph_index: Arc<dyn GraphIndex>,
        max_neighbors: usize,
    ) -> Self {
        Self {
            vector_index,
            graph_index,
            max_neighbors,
            // INV-COG1: Assoziationslatenz bei < 500µs hart abschneiden.
            timeout: Duration::from_micros(500),
        }
    }

    /// Sets a custom timeout for the AutoLinker.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chimera_core::types::ScoredDocument;
    use chimera_core::{IdempotentApply, IndexObserver, VectorIndexStats};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockVectorIndex {
        neighbors: Vec<ScoredDocument>,
    }

    #[async_trait]
    impl IdempotentApply for MockVectorIndex {
        async fn apply_idempotent(
            &self,
            _: &ChimeraContext,
            _: &NamespaceId,
            _: TxId,
            _: &[u8],
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl VectorIndex for MockVectorIndex {
        async fn insert(
            &self,
            _ctx: &ChimeraContext,
            _tx: TxId,
            _id: DocId,
            _embedding: &Embedding,
        ) -> Result<()> {
            Ok(())
        }
        async fn search(
            &self,
            _ctx: &ChimeraContext,
            _query: &Embedding,
            _k: usize,
            _filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
        ) -> Result<Vec<ScoredDocument>> {
            Ok(self.neighbors.clone())
        }
        async fn delete(&self, _ctx: &ChimeraContext, _tx: TxId, _id: DocId) -> Result<()> {
            Ok(())
        }
        async fn stats(&self) -> Result<VectorIndexStats> {
            Ok(VectorIndexStats::default())
        }
    }

    #[async_trait]
    impl IndexObserver for MockVectorIndex {
        async fn on_prepare(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
        async fn on_commit(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
        async fn on_rollback(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
    }

    struct MockGraphIndex {
        edge_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl IdempotentApply for MockGraphIndex {
        async fn apply_idempotent(
            &self,
            _: &ChimeraContext,
            _: &NamespaceId,
            _: TxId,
            _: &[u8],
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl GraphIndex for MockGraphIndex {
        async fn insert_entity(
            &self,
            _ctx: &ChimeraContext,
            _tx: TxId,
            _doc_id: DocId,
            _entity: &Entity,
        ) -> Result<()> {
            Ok(())
        }
        async fn insert_edge(
            &self,
            _ctx: &ChimeraContext,
            _tx: TxId,
            _doc_id: DocId,
            _edge: &Edge,
        ) -> Result<()> {
            self.edge_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn neighbors(
            &self,
            _ctx: &ChimeraContext,
            _entity_id: EntityId,
        ) -> Result<Vec<EntityId>> {
            Ok(vec![])
        }
        async fn traverse_k_hop(
            &self,
            _ctx: &ChimeraContext,
            _start: EntityId,
            _k: u8,
        ) -> Result<Vec<EntityId>> {
            Ok(vec![])
        }
        async fn delete_entity(
            &self,
            _ctx: &ChimeraContext,
            _tx: TxId,
            _doc_id: DocId,
            _entity_id: EntityId,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl IndexObserver for MockGraphIndex {
        async fn on_prepare(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
        async fn on_commit(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
        async fn on_rollback(
            &self,
            _ctx: &ChimeraContext,
            _ns: &NamespaceId,
            _tx: TxId,
        ) -> Result<()> {
            Ok(())
        }
    }

    use chimera_core::types::{Entity, EntityId};

    #[tokio::test]
    async fn test_auto_link_creates_edges() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let vector_index = Arc::new(MockVectorIndex {
            neighbors: vec![
                ScoredDocument::new(DocId::new(2), 0.95), // Above threshold
                ScoredDocument::new(DocId::new(3), 0.90), // Above threshold
                ScoredDocument::new(DocId::new(4), 0.70), // Below threshold
            ],
        });
        let edge_count = Arc::new(AtomicUsize::new(0));
        let graph_index = Arc::new(MockGraphIndex {
            edge_count: edge_count.clone(),
        });

        // Use a longer timeout for tests to avoid flakiness under load
        let linker = BoundedAutoLinker::new(vector_index, graph_index, 5)
            .with_timeout(Duration::from_millis(100));
        let ctx = ChimeraContext::default();
        let ns = NamespaceId::default();
        let tx = TxId::new(1);
        let doc_id = DocId::new(1);
        let embedding = Embedding::new(vec![0.1, 0.2]);

        linker.auto_link(&ctx, &ns, tx, doc_id, &embedding).await?;

        assert_eq!(edge_count.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_auto_link_timeout() -> std::result::Result<(), Box<dyn std::error::Error>> {
        struct SlowVectorIndex;

        #[async_trait]
        impl IdempotentApply for SlowVectorIndex {
            async fn apply_idempotent(
                &self,
                _: &ChimeraContext,
                _: &NamespaceId,
                _: TxId,
                _: &[u8],
            ) -> Result<()> {
                Ok(())
            }
        }

        #[async_trait]
        impl VectorIndex for SlowVectorIndex {
            async fn insert(
                &self,
                _: &ChimeraContext,
                _: TxId,
                _: DocId,
                _: &Embedding,
            ) -> Result<()> {
                Ok(())
            }
            async fn search(
                &self,
                _: &ChimeraContext,
                _: &Embedding,
                _: usize,
                _: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
            ) -> Result<Vec<ScoredDocument>> {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(vec![ScoredDocument::new(DocId::new(2), 0.99)])
            }
            async fn delete(&self, _: &ChimeraContext, _: TxId, _: DocId) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<VectorIndexStats> {
                Ok(VectorIndexStats::default())
            }
        }
        #[async_trait]
        impl IndexObserver for SlowVectorIndex {
            async fn on_prepare(&self, _: &ChimeraContext, _: &NamespaceId, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn on_commit(&self, _: &ChimeraContext, _: &NamespaceId, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn on_rollback(
                &self,
                _: &ChimeraContext,
                _: &NamespaceId,
                _: TxId,
            ) -> Result<()> {
                Ok(())
            }
        }

        let vector_index = Arc::new(SlowVectorIndex);
        let edge_count = Arc::new(AtomicUsize::new(0));
        let graph_index = Arc::new(MockGraphIndex {
            edge_count: edge_count.clone(),
        });

        let linker = BoundedAutoLinker::new(vector_index, graph_index, 5);
        let ctx = ChimeraContext::default();
        let ns = NamespaceId::default();
        let tx = TxId::new(1);
        let doc_id = DocId::new(1);
        let embedding = Embedding::new(vec![0.1]);

        let result = linker.auto_link(&ctx, &ns, tx, doc_id, &embedding).await;

        // Since we now return an error on search timeout, we should check that.
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains("timeout"));

        // Should have timed out before inserting the edge
        assert_eq!(edge_count.load(Ordering::SeqCst), 0);
        Ok(())
    }
}
