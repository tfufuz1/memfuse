//! Search dispatch, hybrid RRF search, and score weighting operations for `Collection`.

use super::{extract_effective_importance, Collection};
use crate::collection::{StoredDocument, StoredDocumentMeta};
use crate::filter::MetadataFilter;
use memfuse_core::{
    DocId, EntityId, FilterExpr, GraphIndex, Result, StorageEngine, TextIndex, TxId, VectorIndex,
};

impl<S: StorageEngine> Collection<S> {
    /// Performs semantic vector search.
    #[tracing::instrument(level = "trace", skip(self, query_embedding))]
    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        self.search_with_filter_expr(query_embedding, k, None).await
    }

    /// Performs semantic search with an advanced metadata filter.
    #[deprecated(
        since = "0.1.0",
        note = "Use search_with_filter_expr with memfuse_core::FilterExpr directly"
    )]
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<crate::SearchResult>> {
        let expr = match filter {
            Some(f) => Some(FilterExpr::try_from(f)?),
            None => None,
        };
        self.search_with_filter_expr(query, k, expr).await
    }

    /// Performs semantic search with an advanced metadata filter expression (`FilterExpr`).
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_with_filter_expr(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<FilterExpr>,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let seq = self.snapshot_seq().await?;
        self.storage.pin_checkpoint(seq).await?;

        let res = async {
            let filter = match filter {
                Some(f) => f,
                None => return self.search_filtered_at(query, k, None, seq).await,
            };

            let total_docs = self.len().await;

            if total_docs < 1000 {
                let matched_ids = self.get_matching_doc_ids_at(&filter, seq).await?;

                if matched_ids.is_empty() {
                    return Ok(Vec::new());
                }

                let filter_fn = move |id: DocId| matched_ids.contains(&id);
                let scored_docs = self
                    .index
                    .search_filtered(query, k, Some(&filter_fn))
                    .await?;
                self.hydrate_from_scored_at(scored_docs, seq).await
            } else {
                let oversample = (k * 10).min(total_docs).max(k);
                let scored_docs = self.index.search_filtered(query, oversample, None).await?;

                let mut results = Vec::new();
                for sd in scored_docs {
                    let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
                    if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                        let (id, doc_metadata) = if let Ok(meta) =
                            serde_json::from_slice::<StoredDocumentMeta>(&bytes)
                        {
                            (meta.id, meta.metadata)
                        } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                            (full.id, full.metadata)
                        } else {
                            tracing::warn!(doc_id = ?sd.doc_id, "Could not deserialize doc_key");
                            continue;
                        };
                        let meta_ref = doc_metadata.as_ref().unwrap_or(&serde_json::Value::Null);
                        if filter.evaluate(meta_ref) {
                            results.push(crate::SearchResult {
                                id,
                                score: sd.score,
                                metadata: doc_metadata,
                                matched_signals: vec![],
                            });
                            if results.len() >= k {
                                break;
                            }
                        }
                    }
                }
                Ok(results)
            }
        }
        .await;

        if let Err(e) = self.storage.unpin_checkpoint(seq).await {
            tracing::error!(
                seq_no = seq,
                "Checkpoint seq={seq} konnte nicht unpinnt werden: {e}. SSTable-GC wird blockiert."
            );
        }
        res
    }

    /// Performs semantic search using a raw text query (automatically embedded).
    #[tracing::instrument(level = "trace", skip(self, query_text))]
    pub async fn search_text(
        &self,
        query_text: &str,
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let embedding = {
            let embedder = {
                let guard = self.embedder.read();
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        memfuse_core::MemFuseError::Internal(
                            "No embedder configured for this collection".into(),
                        )
                    })?
                    .clone()
            };
            embedder.embed(query_text).await?
        };
        self.search(&embedding, k).await
    }

    async fn get_matching_doc_ids_at(
        &self,
        filter: &FilterExpr,
        seq: u64,
    ) -> Result<std::collections::HashSet<DocId>> {
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1);
            p
        };

        let entries = self.storage.scan_prefix_at(&prefix, seq).await?;
        let mut matched = std::collections::HashSet::new();

        for (_, v) in entries {
            let (id, doc_metadata) =
                if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&v) {
                    (meta.id, meta.metadata)
                } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&v) {
                    (full.id, full.metadata)
                } else {
                    continue;
                };
            let metadata = doc_metadata.as_ref().unwrap_or(&serde_json::Value::Null);
            if filter.evaluate(metadata) {
                matched.insert(DocId::from_key(&id)?);
            }
        }

        Ok(matched)
    }

    /// Performs filtered semantic vector search in the collection.
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let seq = self.snapshot_seq().await?;
        self.search_filtered_at(query, k, filter, seq).await
    }

    pub async fn search_filtered_at(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
        self.hydrate_from_scored_at(scored_docs, seq).await
    }

    async fn hydrate_from_scored_at(
        &self,
        scored_docs: Vec<memfuse_core::ScoredDocument>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_docs.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_docs.len());
        for sd in scored_docs {
            let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                let (id, metadata) =
                    if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                        (meta.id, meta.metadata)
                    } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                        (full.id, full.metadata)
                    } else {
                        tracing::warn!(doc_id = ?sd.doc_id, "Could not deserialize doc_key");
                        continue;
                    };
                results.push(crate::SearchResult {
                    id,
                    score: sd.score,
                    metadata,
                    matched_signals: vec![],
                });
            }
        }
        Ok(results)
    }

    async fn hydrate_from_tuples_at(
        &self,
        scored_tuples: Vec<(DocId, f32)>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_tuples.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_tuples.len());
        for (doc_id, score) in scored_tuples {
            let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                let (id, metadata) =
                    if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                        (meta.id, meta.metadata)
                    } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                        (full.id, full.metadata)
                    } else {
                        tracing::warn!(doc_id = ?doc_id, "Could not deserialize doc_key");
                        continue;
                    };
                results.push(crate::SearchResult {
                    id,
                    score,
                    metadata,
                    matched_signals: vec![],
                });
            }
        }
        Ok(results)
    }

    /// Performs hybrid search combining BM25, vector search, and graph traversal results via RRF.
    #[tracing::instrument(level = "trace", skip(self, text, vector))]
    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<crate::SearchResult>> {
        self.hybrid_search_with_weights(text, vector, k, anchor_entities, None)
            .await
    }

    /// Performs hybrid search combining BM25, vector search, and graph traversal, followed by optional Cross-Encoder reranking.
    #[cfg(feature = "reranking")]
    #[tracing::instrument(level = "trace", skip(self, text, vector, reranker, anchor_entities))]
    pub async fn hybrid_search_reranked(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        reranker: Option<&memfuse_embed::CrossEncoderReranker>,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let pre_rerank_k = if reranker.is_some() { k * 3 } else { k };
        let mut results = self
            .hybrid_search(text, vector, pre_rerank_k, anchor_entities)
            .await?;

        if let Some(reranker) = reranker {
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

            match reranker.rerank(text, &candidate_texts).await {
                Ok(ranked) => {
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
                    tracing::debug!("Reranking applied: {} candidates", reranked_results.len());
                    return Ok(reranked_results);
                }
                Err(e) => {
                    tracing::warn!("Reranking failed (using RRF order): {e}");
                }
            }
        }

        results.truncate(k);
        Ok(results)
    }

    /// Performs hybrid search with custom fusion weights for vector, text, and graph signals,
    /// and optional community filtering/boosting.
    #[tracing::instrument(level = "trace", skip(self, text, vector))]
    pub async fn hybrid_search_with_weights(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
    ) -> Result<Vec<crate::SearchResult>> {
        self.hybrid_search_with_strategy(text, vector, k, anchor_entities, weights, None, None)
            .await
    }

    /// Performs hybrid search with custom signal fusion weights and graph traversal strategy.
    #[tracing::instrument(level = "trace", skip(self, text, vector, strategy))]
    #[allow(clippy::too_many_arguments)]
    pub async fn hybrid_search_with_strategy(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
        strategy: Option<&memfuse_core::GraphTraversalStrategy>,
        same_community_as: Option<EntityId>,
    ) -> Result<Vec<crate::SearchResult>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let k = k.min(memfuse_core::MAX_SEARCH_K);

        let seq = self.snapshot_seq().await?;
        let is_vector_zero = vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

        let default_strategy = memfuse_core::GraphTraversalStrategy::default();
        let graph_strat = strategy.unwrap_or(&default_strategy);

        // 1. Vector Signal
        let vector_results = if is_vector_zero {
            Vec::new()
        } else {
            self.search_filtered_at(vector, k, None, seq).await?
        };

        // 2. Text Signal
        let text_results = if is_text_empty {
            Vec::new()
        } else {
            let bm25_results = self.text_index.search_at(text, k, seq).await?;
            self.hydrate_from_tuples_at(
                bm25_results
                    .into_iter()
                    .map(|sd| (sd.doc_id, sd.score))
                    .collect(),
                seq,
            )
            .await?
        };

        // 3. Graph Signal
        let implicit_anchors: Vec<memfuse_core::EntityId>;
        let anchors_ref: Option<&[memfuse_core::EntityId]> = if let Some(anchors) = anchor_entities
        {
            if anchors.is_empty() {
                None
            } else {
                Some(anchors)
            }
        } else if !text_results.is_empty() {
            implicit_anchors = text_results
                .iter()
                .map(|r| memfuse_core::EntityId::from_key(r.id.as_str()))
                .collect::<Result<Vec<_>>>()?;
            Some(&implicit_anchors)
        } else {
            None
        };

        let graph_results = if let Some(anchors) = anchors_ref {
            let tuples = match graph_strat {
                memfuse_core::GraphTraversalStrategy::Hops { max_hops } => {
                    self.graph_index.multi_traverse(anchors, *max_hops).await?
                }
                memfuse_core::GraphTraversalStrategy::PersonalizedPageRank(ppr_config) => {
                    self.graph_index
                        .personalized_page_rank(anchors, ppr_config)
                        .await?
                }
            };
            let doc_tuples = tuples
                .into_iter()
                .map(|(eid, score)| (memfuse_core::DocId::new(eid.inner()), score))
                .collect();
            self.hydrate_from_tuples_at(doc_tuples, seq).await?
        } else {
            Vec::new()
        };

        if vector_results.is_empty() && text_results.is_empty() && graph_results.is_empty() {
            return Ok(Vec::new());
        }

        let (vw, tw, gw) = crate::fusion::weights_to_signal_factors(weights);

        let target_community_id: Option<u64> = if let Some(same_comm_entity) = same_community_as {
            self.get_community(same_comm_entity).await.ok().flatten()
        } else {
            None
        };

        let filter_or_boost = |list: Vec<crate::SearchResult>| async {
            if let Some(target_comm) = target_community_id {
                let mut filtered = Vec::new();
                for mut res in list {
                    if let Ok(eid) = memfuse_core::EntityId::from_key(&res.id) {
                        if let Ok(Some(comm)) = self.get_community(eid).await {
                            if comm == target_comm {
                                res.score *= 1.2;
                                filtered.push(res);
                            }
                        }
                    }
                }
                filtered
            } else {
                list
            }
        };

        let vector_results = filter_or_boost(vector_results).await;
        let text_results = filter_or_boost(text_results).await;
        let graph_results = filter_or_boost(graph_results).await;

        let mut signal_sets = Vec::new();
        if !vector_results.is_empty() {
            signal_sets.push(("vector".to_string(), vector_results, vw));
        }
        if !text_results.is_empty() {
            signal_sets.push(("text".to_string(), text_results, tw));
        }
        if !graph_results.is_empty() {
            signal_sets.push(("graph".to_string(), graph_results, gw));
        }

        Ok(crate::fusion::weighted_reciprocal_rank_fusion(
            signal_sets,
            k,
        ))
    }

    /// Filters a candidate list of search results by effective importance score threshold.
    pub fn filter_by_importance(
        results: Vec<crate::SearchResult>,
        min_threshold: f32,
        now_tx: TxId,
    ) -> Vec<crate::SearchResult> {
        results
            .into_iter()
            .filter(|r| {
                let eff = extract_effective_importance(&r.metadata, now_tx);
                eff >= min_threshold
            })
            .collect()
    }
}
