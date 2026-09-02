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
    filter_fn: Option<Box<dyn Fn(DocId) -> bool + Send + Sync>>,
    #[cfg(feature = "reranking")]
    reranker: Option<&'a memfuse_embed::CrossEncoderReranker>,
    seq: Option<u64>,
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
            filter_fn: None,
            #[cfg(feature = "reranking")]
            reranker: None,
            seq: None,
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

    /// Sets snapshot sequence number for MVCC snapshot-isolated queries.
    pub fn seq(mut self, seq_no: u64) -> Self {
        self.seq = Some(seq_no);
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
        self.k = Some(query.k);
        self
    }

    /// Executes search query, delegating core signal retrieval to `Collection::hybrid_search_with_query()`.
    pub async fn execute(self) -> Result<Vec<crate::SearchResult>> {
        let k = self.k.unwrap_or(10);
        if k == 0 {
            return Ok(Vec::new());
        }

        let fetch_k = if self.filter.is_some()
            || self.memory_type_filter.is_some()
            || self.filter_fn.is_some()
        {
            (k * 5).min(memfuse_core::MAX_SEARCH_K).max(k)
        } else {
            k
        };

        let text_str = self.text.as_deref().unwrap_or("");
        let empty_vec = Vec::new();
        let vector_slice = self.vector.as_deref().unwrap_or(&empty_vec);

        let anchors = self.anchor_entities.as_deref();
        let weights = self.weights.as_ref();
        let graph_strat = self.strategy.as_ref().map(|s| s.to_graph_strategy());

        #[allow(deprecated)]
        let mut results = if self.filter.is_some() || self.memory_type_filter.is_some() {
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
                include_superseded: false,
                k: fetch_k,
            };
            self.collection
                .hybrid_search_with_query(&hybrid_query)
                .await?
        } else if let Some(seq) = self.seq {
            if text_str.is_empty() && anchors.is_none() {
                self.collection
                    .search_filtered_at(vector_slice, fetch_k, self.filter_fn.as_deref(), seq)
                    .await?
            } else {
                self.collection
                    .hybrid_search_with_strategy(
                        text_str,
                        vector_slice,
                        fetch_k,
                        anchors,
                        weights,
                        graph_strat.as_ref(),
                        self.same_community_as,
                    )
                    .await?
            }
        } else {
            self.collection
                .hybrid_search_with_strategy(
                    text_str,
                    vector_slice,
                    fetch_k,
                    anchors,
                    weights,
                    graph_strat.as_ref(),
                    self.same_community_as,
                )
                .await?
        };

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

        #[cfg(feature = "reranking")]
        if let Some(reranker) = self.reranker {
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

                if let Ok(ranked) = reranker.rerank(text_str, &candidate_texts).await {
                    let mut reranked_results = Vec::with_capacity(k);
                    for r in ranked.into_iter().take(k) {
                        if let Some(mut result) = results.get(r.original_index).cloned() {
                            if let Some(meta) = result.metadata.as_mut() {
                                if let Some(obj) = meta.as_object_mut() {
                                    obj.insert("ce_score".to_string(), serde_json::json!(r.score));
                                }
                            }
                            result.score = r.score;
                            reranked_results.push(result);
                        }
                    }
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
}
