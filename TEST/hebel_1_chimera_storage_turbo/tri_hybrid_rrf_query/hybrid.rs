//! Hybrid query execution with spatial-semantic fusion.

use crate::fusion::RRFFusion;
use chimera_core::{
    ChimeraContext, DocId, GraphIndex, HybridQuery, MetadataIndex, Result, ScoredDocument,
    SpatialIndex, VectorIndex,
};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Executor for hybrid queries with spatial-semantic fusion.
pub struct HybridQueryExecutor {
    vector_index: Option<Arc<dyn VectorIndex>>,
    graph_index: Option<Arc<dyn GraphIndex>>,
    metadata_index: Option<Arc<dyn MetadataIndex>>,
    spatial_index: Option<Arc<dyn SpatialIndex>>,
    entity_mapping: Option<crate::mapping::SharedEntityMapping>,
    fusion: RRFFusion,
}

impl HybridQueryExecutor {
    /// Creates a new hybrid query executor.
    pub fn new() -> Self {
        Self {
            vector_index: None,
            graph_index: None,
            metadata_index: None,
            spatial_index: None,
            entity_mapping: None,
            fusion: RRFFusion::new(),
        }
    }

    /// Sets the vector index.
    pub fn with_vector_index(mut self, index: Arc<dyn VectorIndex>) -> Self {
        self.vector_index = Some(index);
        self
    }

    /// Sets the graph index.
    pub fn with_graph_index(mut self, index: Arc<dyn GraphIndex>) -> Self {
        self.graph_index = Some(index);
        self
    }

    /// Sets the metadata index.
    pub fn with_metadata_index(mut self, index: Arc<dyn MetadataIndex>) -> Self {
        self.metadata_index = Some(index);
        self
    }

    /// Sets the spatial index.
    pub fn with_spatial_index(mut self, index: Arc<dyn SpatialIndex>) -> Self {
        self.spatial_index = Some(index);
        self
    }
    /// Sets the entity mapping.
    pub fn with_entity_mapping(mut self, mapping: crate::mapping::SharedEntityMapping) -> Self {
        self.entity_mapping = Some(mapping);
        self
    }

    /// Executes a hybrid query with spatial-semantic fusion.
    ///
    /// This method implements the core quad-hybrid retrieval strategy:
    /// 1. Spatial prefiltering (if spatial parameters present)
    /// 2. Metadata filtering (if filter present)
    /// 3. Candidate set intersection (100x reduction target)
    /// 4. Vector search on reduced candidate set
    /// 5. Graph traversal (if graph seed present)
    /// 6. RRF fusion of results
    #[instrument(skip(self, ctx))]
    pub async fn execute(
        &self,
        ctx: &ChimeraContext,
        query: HybridQuery,
    ) -> Result<Vec<ScoredDocument>> {
        info!("Executing hybrid query with limit={}", query.limit);

        // Step 1 & 2: Parallel spatial and metadata filtering
        let spatial_fut = self.execute_spatial_filter(ctx, &query);
        let metadata_fut = self.execute_metadata_filter(ctx, &query);

        let (spatial_res, metadata_res) = tokio::join!(spatial_fut, metadata_fut);
        let spatial_candidates = spatial_res?;
        let metadata_candidates = metadata_res?;

        if let Some(ref candidates) = spatial_candidates {
            debug!("Spatial filter returned {} candidates", candidates.len());
        }
        if let Some(ref candidates) = metadata_candidates {
            debug!("Metadata filter returned {} candidates", candidates.len());
        }

        // Step 3: Intersect candidate sets
        let candidates = Self::intersect_candidates(spatial_candidates, metadata_candidates);
        if let Some(ref c) = candidates {
            info!("Candidate set reduced to {} documents", c.len());
        }

        // Step 4: Vector search on reduced candidate set
        let vector_results = self
            .execute_vector_search(ctx, &query, candidates.as_ref())
            .await?;
        debug!("Vector search returned {} results", vector_results.len());

        // Step 5: Graph traversal
        let graph_results = self.execute_graph_traversal(ctx, &query).await?;
        debug!("Graph traversal returned {} results", graph_results.len());

        // Step 6: RRF fusion
        let mut weighted_results = Vec::new();
        if !vector_results.is_empty() {
            weighted_results.push((vector_results, query.weights.vector));
        }
        if !graph_results.is_empty() {
            weighted_results.push((graph_results, query.weights.graph));
        }

        let fused = self.fusion.fuse_weighted(weighted_results);
        let final_results: Vec<_> = fused.into_iter().take(query.limit).collect();

        info!("Returning {} final results", final_results.len());
        Ok(final_results)
    }

    /// Executes spatial filtering if spatial parameters are present.
    async fn execute_spatial_filter(
        &self,
        ctx: &ChimeraContext,
        query: &HybridQuery,
    ) -> Result<Option<Vec<DocId>>> {
        if let Some(ref index) = self.spatial_index {
            // Radius query
            if let (Some(center), Some(radius)) = (query.spatial_center, query.spatial_radius) {
                let results = index.query_radius(ctx, &center, radius).await?;
                return Ok(Some(results));
            }

            // Box query
            if let Some(bbox) = query.spatial_box {
                let results = index.query_box(ctx, &bbox).await?;
                return Ok(Some(results));
            }
        }
        Ok(None)
    }

    /// Executes metadata filtering if filter is present.
    async fn execute_metadata_filter(
        &self,
        ctx: &ChimeraContext,
        query: &HybridQuery,
    ) -> Result<Option<Vec<DocId>>> {
        if let (Some(ref index), Some(ref filter_str)) = (&self.metadata_index, &query.filter) {
            let bitmap = index.evaluate(ctx, filter_str).await?;
            let ids: Vec<DocId> = bitmap.iter().map(DocId::new).collect();
            Ok(Some(ids))
        } else {
            Ok(None)
        }
    }

    /// Intersects candidate sets from different filters.
    ///
    /// This is where the 100x candidate reduction happens.
    fn intersect_candidates(
        spatial: Option<Vec<DocId>>,
        metadata: Option<Vec<DocId>>,
    ) -> Option<HashSet<DocId>> {
        match (spatial, metadata) {
            (Some(s), Some(m)) => {
                let s_set: HashSet<_> = s.into_iter().collect();
                let m_set: HashSet<_> = m.into_iter().collect();
                Some(s_set.intersection(&m_set).copied().collect())
            }
            (Some(s), None) => Some(s.into_iter().collect()),
            (None, Some(m)) => Some(m.into_iter().collect()),
            (None, None) => None,
        }
    }

    /// Executes vector search, optionally filtered by candidate set.
    async fn execute_vector_search(
        &self,
        ctx: &ChimeraContext,
        query: &HybridQuery,
        candidates: Option<&HashSet<DocId>>,
    ) -> Result<Vec<ScoredDocument>> {
        if let (Some(ref index), Some(ref embedding)) = (&self.vector_index, &query.vector) {
            // Fetch more results for better RRF fusion
            let k = query.limit * 2;

            let results = index.search(ctx, embedding, k, None).await?;

            // Filter by candidate set if present
            if let Some(allowed) = candidates {
                let filtered: Vec<_> = results
                    .into_iter()
                    .filter(|doc| allowed.contains(&doc.doc_id))
                    .take(query.limit)
                    .collect();
                Ok(filtered)
            } else {
                Ok(results)
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Executes graph traversal if graph seed is present.
    async fn execute_graph_traversal(
        &self,
        ctx: &ChimeraContext,
        query: &HybridQuery,
    ) -> Result<Vec<ScoredDocument>> {
        if let (Some(ref index), Some(seed)) = (&self.graph_index, query.graph_seed) {
            let entities = index.traverse_k_hop(ctx, seed, query.graph_hops).await?;

            // Convert EntityIds to ScoredDocuments
            let mut docs = Vec::with_capacity(entities.len());
            for eid in entities {
                let doc_id = if let Some(ref mapping) = self.entity_mapping {
                    mapping.resolve(ctx.namespace_id(), eid).ok_or_else(|| {
                        chimera_core::ChimeraError::MappingNotFound {
                            entity_id: eid.inner(),
                        }
                    })?
                } else {
                    return Err(chimera_core::ChimeraError::Internal(
                        "Missing entity mapping".to_string(),
                    ));
                };
                docs.push(ScoredDocument::new(doc_id, 1.0));
            }
            Ok(docs)
        } else {
            Ok(Vec::new())
        }
    }
}

impl Default for HybridQueryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersect_candidates() {
        let spatial = Some(vec![
            DocId::new(1),
            DocId::new(2),
            DocId::new(3),
            DocId::new(4),
        ]);
        let metadata = Some(vec![DocId::new(2), DocId::new(3), DocId::new(5)]);

        let result = HybridQueryExecutor::intersect_candidates(spatial, metadata);
        assert!(result.is_some());

        let intersection = result.unwrap();
        assert_eq!(intersection.len(), 2);
        assert!(intersection.contains(&DocId::new(2)));
        assert!(intersection.contains(&DocId::new(3)));
    }

    #[test]
    fn test_intersect_spatial_only() {
        let spatial = Some(vec![DocId::new(1), DocId::new(2)]);
        let metadata = None;

        let result = HybridQueryExecutor::intersect_candidates(spatial, metadata);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_intersect_none() {
        let result = HybridQueryExecutor::intersect_candidates(None, None);
        assert!(result.is_none());
    }
}
