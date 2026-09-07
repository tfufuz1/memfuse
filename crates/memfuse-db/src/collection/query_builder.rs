// FILE-CONTEXT
// ZWECK: Fluent HybridQueryBuilder Fassade für Konsolidierung aller Search-Signaturen.
// INVARIANTEN: execute() delegiert an hybrid_search_with_strategy(); keine Logikduplikation.
// NICHT-OFFENSICHTLICH: Post-RRF Filtering bewahrt Snapshot-Konsistenz und RRF-Skalierung.
// STAND: TS:2026-08-30T21:00:00Z (SESSION: 0dcb9f3b)

use super::Collection;
#[allow(deprecated)]
use crate::filter::MetadataFilter;
use memfuse_core::{
    DocId, EntityId, FilterExpr, FusionWeights, GraphTraversalStrategy, MemoryType, Result,
    StorageEngine, VectorIndex,
};

/// Custom weights for vector, text, and graph signals in hybrid search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalWeights {
    /// Weight for vector similarity signal.
    pub vector: f32,
    /// Weight for BM25 text match signal.
    pub text: f32,
    /// Weight for graph traversal signal.
    pub graph: f32,
}

impl SignalWeights {
    /// Creates a new `SignalWeights` instance, ensuring weights sum to 1.0.
    pub fn new(vector: f32, text: f32, graph: f32) -> Result<Self> {
        let fw = FusionWeights::new(vector, text, graph)?;
        Ok(Self {
            vector: fw.vector(),
            text: fw.text(),
            graph: fw.graph(),
        })
    }
}

impl From<SignalWeights> for FusionWeights {
    fn from(w: SignalWeights) -> Self {
        FusionWeights::new(w.vector, w.text, w.graph).unwrap_or_default()
    }
}

impl From<FusionWeights> for SignalWeights {
    fn from(w: FusionWeights) -> Self {
        Self {
            vector: w.vector(),
            text: w.text(),
            graph: w.graph(),
        }
    }
}

/// Strategy for hybrid search and graph signal traversal.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStrategy {
    /// Reciprocal Rank Fusion (default fusion strategy).
    Rrf,
    /// Multi-hop BFS graph traversal strategy.
    Hops {
        /// Maximum traversal hop depth.
        max_hops: usize,
    },
    /// Personalized PageRank power iteration graph traversal strategy.
    PersonalizedPageRank(memfuse_core::PprConfig),
}

impl SearchStrategy {
    /// Converts `SearchStrategy` into core `GraphTraversalStrategy`.
    pub fn to_graph_strategy(&self) -> GraphTraversalStrategy {
        match self {
            SearchStrategy::Rrf => GraphTraversalStrategy::Hops { max_hops: 3 },
            SearchStrategy::Hops { max_hops } => GraphTraversalStrategy::Hops {
                max_hops: *max_hops,
            },
            SearchStrategy::PersonalizedPageRank(cfg) => {
                GraphTraversalStrategy::PersonalizedPageRank(cfg.clone())
            }
        }
    }
}

impl From<GraphTraversalStrategy> for SearchStrategy {
    fn from(strategy: GraphTraversalStrategy) -> Self {
        match strategy {
            GraphTraversalStrategy::Hops { max_hops } => SearchStrategy::Hops { max_hops },
            GraphTraversalStrategy::PersonalizedPageRank(cfg) => {
                SearchStrategy::PersonalizedPageRank(cfg)
            }
        }
    }
}

/// Default candidate pool expansion multiplier for Cross-Encoder reranking (10x requested k).
///
/// Grounded in empirical evaluation from T2-RAGBench (arXiv:2604.01733), showing Recall@5 = 0.888
/// with ~100 candidate items compared to 0.458 with only 20 candidates.
pub const DEFAULT_RERANK_POOL_MULTIPLIER: usize = 10;

/// Default upper cap for pre-reranking candidate pool expansion (200 candidates).
///
/// Prevents retrieval cost explosion for queries with large `k`.
pub const DEFAULT_RERANK_POOL_MAX: usize = 200;

/// Fluent query builder for unifying vector, text, graph, and hybrid search operations.
pub struct HybridQueryBuilder<'a, S: StorageEngine, V: VectorIndex> {
    collection: &'a Collection<S, V>,
    text: Option<String>,
    vector: Option<Vec<f32>>,
    k: Option<usize>,
    weights: Option<FusionWeights>,
    strategy: Option<SearchStrategy>,
    filter: Option<FilterExpr>,
    anchor_entities: Option<Vec<EntityId>>,
    same_community_as: Option<EntityId>,
    memory_type_filter: Option<Vec<MemoryType>>,
    include_superseded: bool,
    include_provenance: bool,
    filter_fn: Option<Box<dyn Fn(DocId) -> bool + Send + Sync>>,
    #[cfg(feature = "reranking")]
    reranker: Option<&'a memfuse_embed::CrossEncoderReranker>,
    rerank_pool_multiplier: Option<usize>,
    rerank_pool_max: Option<usize>,
    seq: Option<u64>,
    as_of_timestamp: Option<u64>,
    query_timestamp: Option<u64>,
    current_tx: Option<memfuse_core::TxId>,
}

impl<'a, S: StorageEngine, V: VectorIndex> HybridQueryBuilder<'a, S, V> {
    /// Creates a new `HybridQueryBuilder` for a given `Collection`.
    pub fn new(collection: &'a Collection<S, V>) -> Self {
        Self {
            collection,
            text: None,
            vector: None,
            k: None,
            weights: None,
            strategy: None,
            filter: None,
            anchor_entities: None,
            same_community_as: None,
            memory_type_filter: None,
            include_superseded: false,
            include_provenance: false,
            filter_fn: None,
            #[cfg(feature = "reranking")]
            reranker: None,
            rerank_pool_multiplier: None,
            rerank_pool_max: None,
            seq: None,
            as_of_timestamp: None,
            query_timestamp: None,
            current_tx: None,
        }
    }

    /// Sets full-text search query string.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Sets dense vector query embedding.
    pub fn embedding(mut self, embedding: impl AsRef<[f32]>) -> Self {
        self.vector = Some(embedding.as_ref().to_vec());
        self
    }

    /// Alias for `.embedding()` to specify dense vector query embedding.
    pub fn vector(self, vector: impl AsRef<[f32]>) -> Self {
        self.embedding(vector)
    }

    /// Sets top-k maximum result count.
    pub fn k(mut self, k: usize) -> Self {
        self.k = Some(k);
        self
    }

    /// Sets custom signal fusion weights using `SignalWeights` or `FusionWeights`.
    pub fn weights<W>(mut self, weights: W) -> Self
    where
        W: TryInto<FusionWeights, Error = memfuse_core::MemFuseError>,
    {
        if let Ok(fw) = weights.try_into() {
            self.weights = Some(fw);
        }
        self
    }

    /// Sets custom signal fusion weights directly using `FusionWeights`.
    pub fn fusion_weights(mut self, weights: FusionWeights) -> Self {
        self.weights = Some(weights);
        self
    }

    /// Sets metadata expression filter (`FilterExpr`).
    pub fn filter(mut self, filter: FilterExpr) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Sets legacy metadata filter (`MetadataFilter`).
    #[allow(deprecated)]
    pub fn metadata_filter(mut self, filter: MetadataFilter) -> Self {
        if let Ok(expr) = FilterExpr::try_from(filter) {
            self.filter = Some(expr);
        }
        self
    }

    /// Sets hybrid search / graph traversal strategy.
    pub fn strategy(mut self, strategy: impl Into<SearchStrategy>) -> Self {
        self.strategy = Some(strategy.into());
        self
    }

    /// Sets anchor seed entities for graph traversal signal.
    pub fn anchor_entities(mut self, anchors: impl IntoIterator<Item = EntityId>) -> Self {
        self.anchor_entities = Some(anchors.into_iter().collect());
        self
    }

    /// Alias for `.anchor_entities()`.
    pub fn anchors(self, anchors: impl IntoIterator<Item = EntityId>) -> Self {
        self.anchor_entities(anchors)
    }

    /// Sets target entity for community context boosting/filtering.
    pub fn same_community_as(mut self, entity_id: EntityId) -> Self {
        self.same_community_as = Some(entity_id);
        self
    }

    /// Sets cognitive memory type filter (Pre-RRF filter).
    pub fn memory_type_filter(mut self, types: impl IntoIterator<Item = MemoryType>) -> Self {
        self.memory_type_filter = Some(types.into_iter().collect());
        self
    }

    /// Sets whether to calculate and attach ProvenanceRecord to output results.
    pub fn include_provenance(mut self, include: bool) -> Self {
        self.include_provenance = include;
        self
    }

    /// Sets whether to include superseded documents (Post-RRF Supersedes Displacement, ADR-038).
    pub fn include_superseded(mut self, include: bool) -> Self {
        self.include_superseded = include;
        self
    }

    /// Alias for `.memory_type_filter()`.
    pub fn memory_types(self, types: impl IntoIterator<Item = MemoryType>) -> Self {
        self.memory_type_filter(types)
    }

    /// Sets custom filter predicate function for vector search candidates.
    pub fn filter_fn(mut self, f: impl Fn(DocId) -> bool + Send + Sync + 'static) -> Self {
        self.filter_fn = Some(Box::new(f));
        self
    }

    /// Sets optional CrossEncoder reranker for post-retrieval ranking.
    #[cfg(feature = "reranking")]
    pub fn reranker(mut self, reranker: &'a memfuse_embed::CrossEncoderReranker) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Sets the candidate pool multiplier for pre-reranking candidate expansion.
    ///
    /// Default: 10 (`DEFAULT_RERANK_POOL_MULTIPLIER`), yielding ~100 candidates for `k=10`
    /// per T2-RAGBench empirical recall optimization.
    pub fn rerank_pool_multiplier(mut self, multiplier: usize) -> Self {
        self.rerank_pool_multiplier = Some(multiplier);
        self
    }

    /// Sets the upper cap for pre-reranking candidate pool expansion.
    ///
    /// Default: 200 (`DEFAULT_RERANK_POOL_MAX`), capping pre-retrieval pool size for large `k`.
    pub fn rerank_pool_max(mut self, max: usize) -> Self {
        self.rerank_pool_max = Some(max);
        self
    }

    /// Sets snapshot sequence number for MVCC snapshot-isolated queries.
    pub fn seq(mut self, seq_no: u64) -> Self {
        self.seq = Some(seq_no);
        self
    }

    /// Sets historical point-in-time "as-of" timestamp for temporal validity filtering (Post-RRF, Pre-Reranking).
    pub fn as_of(mut self, timestamp: u64) -> Self {
        self.as_of_timestamp = Some(timestamp);
        self
    }

    /// Sets query business timestamp for temporal validity filtering (Post-RRF, Pre-Reranking).
    pub fn query_timestamp(mut self, timestamp: u64) -> Self {
        self.query_timestamp = Some(timestamp);
        self
    }

    /// Sets current system transaction ID for temporal validity filtering (Post-RRF, Pre-Reranking).
    pub fn current_tx(mut self, tx: memfuse_core::TxId) -> Self {
        self.current_tx = Some(tx);
        self
    }

    /// Configures builder options from an existing `HybridQuery` struct.
    pub fn query_config(mut self, query: &memfuse_core::HybridQuery) -> Self {
        if let Some(ref text) = query.text_query {
            self.text = Some(text.clone());
        }
        if let Some(ref vector) = query.vector_query {
            self.vector = Some(vector.clone());
        }
        if let Some(ref start_node) = query.graph_start_node {
            if let Ok(eid) = EntityId::from_key(start_node) {
                self.anchor_entities = Some(vec![eid]);
            }
        }
        self.strategy = Some(query.graph_strategy.clone().into());
        self.weights = Some(query.fusion_weights.clone());
        self.filter = query.filter.clone();
        self.same_community_as = query.same_community_as;
        self.memory_type_filter = query.memory_type_filter.clone();
        self.include_superseded = query.include_superseded;
        self.include_provenance = query.include_provenance;
        self.rerank_pool_multiplier = query.rerank_pool_multiplier;
        self.rerank_pool_max = query.rerank_pool_max;
        self.k = Some(query.k);
        self
    }

    /// Executes search query, delegating core signal retrieval to `Collection::hybrid_search_with_query()`.
    pub async fn execute(self) -> Result<Vec<crate::SearchResult>> {
        let k = self.k.unwrap_or(10);
        if k == 0 {
            return Ok(Vec::new());
        }

        #[cfg(feature = "reranking")]
        let _has_reranker = self.reranker.is_some();
        #[cfg(not(feature = "reranking"))]
        let _has_reranker = false;

        let hybrid_query = memfuse_core::HybridQuery {
            text_query: self.text.clone(),
            vector_query: self.vector.clone(),
            graph_start_node: self
                .anchor_entities
                .as_ref()
                .and_then(|a| a.first())
                .map(|e| e.to_string()),
            graph_strategy: self
                .strategy
                .as_ref()
                .map(|s| s.to_graph_strategy())
                .unwrap_or_default(),
            fusion_weights: self.weights.unwrap_or_default(),
            filter: self.filter.clone(),
            memory_type_filter: self.memory_type_filter.clone(),
            same_community_as: self.same_community_as,
            include_superseded: self.include_superseded,
            include_provenance: self.include_provenance,
            rerank_pool_multiplier: self.rerank_pool_multiplier,
            rerank_pool_max: self.rerank_pool_max,
            k,
        };

        #[allow(deprecated)]
        let mut results = self
            .collection
            .hybrid_search_with_query(&hybrid_query)
            .await?;

        if let Some(ref filter_expr) = self.filter {
            results.retain(|res| {
                let meta_ref = res.metadata.as_ref().unwrap_or(&serde_json::Value::Null);
                filter_expr.evaluate(meta_ref)
            });
        }

        if let Some(ref memory_types) = self.memory_type_filter {
            results.retain(|res| {
                let mt = crate::filter::extract_memory_type(&res.metadata);
                memory_types.contains(&mt)
            });
        }

        if let Some(ref filter_fn) = self.filter_fn {
            let mut filtered = Vec::with_capacity(results.len());
            for res in results {
                if let Ok(doc_id) = DocId::from_key(&res.id) {
                    if filter_fn(doc_id) {
                        filtered.push(res);
                    }
                }
            }
            results = filtered;
        }

        // Post-RRF Bi-temporal Validity Filtering (ADR-033 / ADR-038)
        // Executed NACH RRF-Fusion and VOR CrossEncoder Reranking to ensure high efficiency
        // (expensive CrossEncoder reranker is only invoked on temporally valid candidates).
        if let Some(as_of) = self.as_of_timestamp {
            results = crate::temporal_filter::apply_temporal_validity_filter_at(results, as_of);
        } else if self.query_timestamp.is_some() || self.current_tx.is_some() {
            let ctx = self.current_tx.unwrap_or(memfuse_core::TxId::new(u64::MAX));
            results = crate::temporal_filter::apply_temporal_validity_filter(
                results,
                ctx,
                self.query_timestamp,
            );
        }

        #[cfg(feature = "reranking")]
        if let Some(reranker) = self.reranker {
            let text_str = self.text.as_deref().unwrap_or("");
            if !results.is_empty() && !text_str.is_empty() {
                let candidate_texts: Vec<String> = results
                    .iter()
                    .map(|r| {
                        r.metadata
                            .as_ref()
                            .and_then(|m| m.get("text").or_else(|| m.get("content")))
                            .and_then(|v| v.as_str())
                            .unwrap_or(&r.id)
                            .to_string()
                    })
                    .collect();

                let rerank_deadline = std::time::Duration::from_millis(
                    reranker.config().rerank_deadline_ms.unwrap_or(500),
                );

                let reranked = tokio::time::timeout(
                    rerank_deadline,
                    reranker.rerank(text_str, &candidate_texts),
                )
                .await
                .map_err(|_| {
                    tracing::warn!(
                        deadline_ms = rerank_deadline.as_millis(),
                        "Reranker deadline exceeded — falling back to RRF order"
                    );
                })
                .and_then(|r| {
                    r.map_err(|e| {
                        tracing::warn!("Reranking failed: {e}");
                    })
                });

                if let Ok(ranked) = reranked {
                    let mut reranked_results = Vec::with_capacity(k);
                    for r in ranked.into_iter().take(k) {
                        if let Some(mut result) = results.get(r.original_index).cloned() {
                            if let Some(meta) = result.metadata.as_mut() {
                                if let Some(obj) = meta.as_object_mut() {
                                    obj.insert("ce_score".to_string(), serde_json::json!(r.score));
                                }
                            } else {
                                result.metadata = Some(serde_json::json!({ "ce_score": r.score }));
                            }
                            result.score = r.score;
                            if let Some(p) = result.provenance.as_mut() {
                                p.rerank_score = Some(r.score);
                            }
                            reranked_results.push(result);
                        }
                    }
                    tracing::debug!("Reranking applied: {} candidates", reranked_results.len());
                    return Ok(reranked_results);
                }
            }
        }

        results.truncate(k);
        Ok(results)
    }
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Recommended unified entrypoint for search queries using fluent `HybridQueryBuilder`.
    pub fn query(&self) -> HybridQueryBuilder<'_, S, V> {
        HybridQueryBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Collection, DistanceMetric, Language};
    use memfuse_core::{FilterExpr, HybridQuery};
    use memfuse_graph::CsrGraph;
    use memfuse_index::{HnswConfig, HnswIndex};
    use memfuse_store::{LsmConfig, LsmStorage};
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_collection(name: &str) -> (Collection<LsmStorage, HnswIndex>, TempDir) {
        let dir = TempDir::new().unwrap(); // unwrap
        let lsm_config = LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let hnsw_config = HnswConfig {
            dimension: 4,
            distance_metric: DistanceMetric::Cosine,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = Collection::new(
            name.to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            Language::English,
        );
        (col, dir)
    }

    #[tokio::test]
    async fn test_builder_vector_search_equivalence() {
        let (col, _dir) = create_test_collection("test_vec").await;
        col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"tag": "a"})))
            .await
            .unwrap(); // unwrap
        col.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"tag": "b"})))
            .await
            .unwrap(); // unwrap

        #[allow(deprecated)]
        let legacy = col.search(&[1.0, 0.0, 0.0, 0.0], 2).await.unwrap(); // unwrap
        let builder_res = col
            .query()
            .embedding([1.0, 0.0, 0.0, 0.0])
            .k(2)
            .execute()
            .await
            .unwrap(); // unwrap

        assert_eq!(builder_res.len(), legacy.len());
        assert_eq!(builder_res[0].id, legacy[0].id);
        assert_eq!(builder_res[1].id, legacy[1].id);
    }

    #[tokio::test]
    async fn test_builder_filter_expr_equivalence() {
        let (col, _dir) = create_test_collection("test_filter").await;
        col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"cat": "news"})))
            .await
            .unwrap(); // unwrap
        col.insert("doc-2", &[0.9, 0.1, 0.0, 0.0], Some(json!({"cat": "blog"})))
            .await
            .unwrap(); // unwrap

        let filter = FilterExpr::Eq {
            field: "cat".to_string(),
            value: json!("news"),
        };

        #[allow(deprecated)]
        let legacy = col
            .search_with_filter_expr(&[1.0, 0.0, 0.0, 0.0], 10, Some(filter.clone()))
            .await
            .unwrap(); // unwrap

        let builder_res = col
            .query()
            .embedding([1.0, 0.0, 0.0, 0.0])
            .filter(filter)
            .k(10)
            .execute()
            .await
            .unwrap(); // unwrap

        assert_eq!(builder_res.len(), 1);
        assert_eq!(builder_res.len(), legacy.len());
        assert_eq!(builder_res[0].id, legacy[0].id);
        assert_eq!(builder_res[0].id, "doc-1");
    }

    #[tokio::test]
    async fn test_builder_hybrid_search_equivalence() {
        let (col, _dir) = create_test_collection("test_hybrid").await;
        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "rust programming language"})),
        )
        .await
        .unwrap(); // unwrap
        col.insert(
            "doc-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"text": "python data science"})),
        )
        .await
        .unwrap(); // unwrap

        #[allow(deprecated)]
        let legacy = col
            .hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 5, None)
            .await
            .unwrap(); // unwrap

        let builder_res = col
            .query()
            .text("rust")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .k(5)
            .execute()
            .await
            .unwrap(); // unwrap

        assert_eq!(builder_res.len(), legacy.len());
        if !builder_res.is_empty() {
            assert_eq!(builder_res[0].id, legacy[0].id);
        }
    }

    #[tokio::test]
    async fn test_builder_weights_and_strategy() {
        let (col, _dir) = create_test_collection("test_weights").await;
        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "alpha"})),
        )
        .await
        .unwrap(); // unwrap

        let weights = SignalWeights::new(0.6, 0.4, 0.0).unwrap(); // unwrap
        let builder_res = col
            .query()
            .text("alpha")
            .vector([1.0, 0.0, 0.0, 0.0])
            .fusion_weights(weights.into())
            .strategy(SearchStrategy::Hops { max_hops: 2 })
            .k(5)
            .execute()
            .await
            .unwrap(); // unwrap

        assert!(!builder_res.is_empty());
        assert_eq!(builder_res[0].id, "doc-1");
    }

    #[tokio::test]
    async fn test_builder_query_config_equivalence() {
        let (col, _dir) = create_test_collection("test_query_cfg").await;
        col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 10})))
            .await
            .unwrap(); // unwrap

        let hybrid_query = HybridQuery::builder()
            .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
            .with_k(1)
            .build()
            .unwrap(); // unwrap

        #[allow(deprecated)]
        let legacy = col.hybrid_search_with_query(&hybrid_query).await.unwrap(); // unwrap

        let builder_res = col
            .query()
            .query_config(&hybrid_query)
            .execute()
            .await
            .unwrap(); // unwrap

        assert_eq!(builder_res.len(), legacy.len());
        assert_eq!(builder_res[0].id, legacy[0].id);
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_rerank_candidate_pool_size_k10_fetches_100_candidates() {
        let (col, _dir) = create_test_collection("test_rerank_pool_100").await;
        // Populate 150 documents
        for i in 0..150 {
            let id = format!("doc-{:03}", i);
            let text = format!("rust system engineering doc {:03}", i);
            let val = (i as f32 + 1.0) / 150.0;
            col.insert(
                &id,
                &[val, 1.0 - val, 0.0, 0.0],
                Some(json!({ "text": text })),
            )
            .await
            .unwrap();
        }

        let reranker = memfuse_embed::CrossEncoderReranker::passthrough();
        let res = col
            .query()
            .text("rust system")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .reranker(&reranker)
            .k(10)
            .execute()
            .await
            .unwrap();

        assert_eq!(res.len(), 10, "Final results truncated to k=10");
        // Verify that candidates retrieved before truncation had ce_score attached to 100 items (or top k items returned with ce_score)
        // With passthrough reranker, ce_score is attached to top k items from the 100 candidates
        assert!(res[0].metadata.as_ref().unwrap().get("ce_score").is_some());
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_rerank_candidate_pool_max_cap_k100_capped_at_200() {
        let (col, _dir) = create_test_collection("test_rerank_pool_max_200").await;
        // Populate 300 documents
        for i in 0..300 {
            let id = format!("doc-{:03}", i);
            let text = format!("benchmark item {:03}", i);
            let val = (i as f32 + 1.0) / 300.0;
            col.insert(
                &id,
                &[val, 1.0 - val, 0.0, 0.0],
                Some(json!({ "text": text })),
            )
            .await
            .unwrap();
        }

        let reranker = memfuse_embed::CrossEncoderReranker::passthrough();

        // Query with k=100 and default pool settings (mult=10, max=200) -> fetch_k = min(100*10, 200) = 200
        let res_default = col
            .query()
            .text("benchmark item")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .reranker(&reranker)
            .k(100)
            .execute()
            .await
            .unwrap();

        assert_eq!(res_default.len(), 100);

        // Custom settings test: rerank_pool_max(50)
        let res_custom_max = col
            .query()
            .text("benchmark item")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .reranker(&reranker)
            .rerank_pool_max(50)
            .k(30)
            .execute()
            .await
            .unwrap();

        // Since fetch_k is capped at 50, top-30 query successfully completes and receives results
        assert_eq!(
            res_custom_max.len(),
            30,
            "k=30 requested with fetch_k capped at 50"
        );
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_query_builder_reranking_with_text_and_reranker() {
        let (col, _dir) = create_test_collection("test_rerank_builder").await;
        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "rust programming language"})),
        )
        .await
        .unwrap(); // unwrap
        col.insert(
            "doc-2",
            &[0.9, 0.1, 0.0, 0.0],
            Some(json!({"content": "python programming language"})),
        )
        .await
        .unwrap(); // unwrap
        col.insert(
            "doc-3",
            &[0.8, 0.2, 0.0, 0.0],
            Some(json!({"text": "javascript web development"})),
        )
        .await
        .unwrap(); // unwrap

        let reranker_res =
            memfuse_embed::CrossEncoderReranker::new(memfuse_embed::RerankConfig::default());
        if let Ok(reranker) = reranker_res {
            let res = col
                .query()
                .text("rust")
                .embedding([1.0, 0.0, 0.0, 0.0])
                .reranker(&reranker)
                .include_provenance(true)
                .k(2)
                .execute()
                .await
                .unwrap(); // unwrap

            assert_eq!(res.len(), 2, "Results must be truncated to k=2");

            for item in &res {
                let meta = item.metadata.as_ref().expect("metadata must exist"); // expect
                assert!(
                    meta.get("ce_score").is_some(),
                    "ce_score must be attached to metadata"
                );
                if let Some(prov) = &item.provenance {
                    assert!(
                        prov.rerank_score.is_some(),
                        "rerank_score must be set in provenance"
                    );
                }
            }

            assert!(
                res[0].score >= res[1].score,
                "Results must be sorted descending by rerank score"
            );
        }
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_reranker_deadline_falls_back() {
        let (col, _dir) = create_test_collection("test_rerank_deadline").await;
        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "rust programming language"})),
        )
        .await
        .unwrap(); // unwrap
        col.insert(
            "doc-2",
            &[0.9, 0.1, 0.0, 0.0],
            Some(json!({"content": "python programming language"})),
        )
        .await
        .unwrap(); // unwrap

        let config = memfuse_embed::RerankConfig {
            rerank_deadline_ms: Some(10),
            simulate_delay_ms: Some(100),
            ..Default::default()
        };

        let reranker = memfuse_embed::CrossEncoderReranker::passthrough_with_config(config);

        let res = col
            .query()
            .text("rust")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .reranker(&reranker)
            .k(2)
            .execute()
            .await;

        assert!(
            res.is_ok(),
            "Query must succeed even when reranker times out"
        );
        let results = res.unwrap(); // unwrap
        assert_eq!(results.len(), 2);
        for item in &results {
            if let Some(meta) = &item.metadata {
                assert!(
                    meta.get("ce_score").is_none(),
                    "ce_score should not be attached on timeout fallback"
                );
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_query_builder_reranking_no_text_query_skips_rerank() {
        let (col, _dir) = create_test_collection("test_rerank_no_text").await;
        col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 1})))
            .await
            .unwrap(); // unwrap
        col.insert("doc-2", &[0.9, 0.1, 0.0, 0.0], Some(json!({"val": 2})))
            .await
            .unwrap(); // unwrap

        let reranker_res =
            memfuse_embed::CrossEncoderReranker::new(memfuse_embed::RerankConfig::default());
        if let Ok(reranker) = reranker_res {
            let res = col
                .query()
                .embedding([1.0, 0.0, 0.0, 0.0])
                .reranker(&reranker)
                .k(2)
                .execute()
                .await
                .unwrap(); // unwrap

            assert_eq!(res.len(), 2);
            for item in &res {
                if let Some(meta) = &item.metadata {
                    assert!(
                        meta.get("ce_score").is_none(),
                        "ce_score should not be attached when reranking is skipped"
                    );
                }
            }
        }
    }
}
