//! Query planner for hybrid queries.

use chimera_core::HybridQuery;
use tracing::debug;

/// Default weight for vector similarity search.
pub const DEFAULT_WEIGHT_VECTOR: f32 = 1.0;
/// Default weight for graph traversal (higher precision).
pub const DEFAULT_WEIGHT_GRAPH: f32 = 1.2;
/// Default weight for sparse lexical search (fallback).
pub const DEFAULT_WEIGHT_SPARSE: f32 = 0.8;

/// Query planner for optimizing hybrid queries.
///
/// The planner determines the optimal execution order for
/// filter, vector, and graph operations.
pub struct QueryPlanner {
    /// Enable filter pushdown optimization.
    _filter_pushdown: bool,
    /// Enable parallel execution.
    parallel_execution: bool,
    /// Weights for the fusion stage.
    weights: FusionWeights,
}

impl QueryPlanner {
    /// Creates a new query planner with default settings and standard weights.
    pub fn new() -> Self {
        Self {
            _filter_pushdown: true,
            parallel_execution: true,
            weights: FusionWeights::default(),
        }
    }

    /// Plans the execution of a hybrid query.
    ///
    /// # Arguments
    /// * `query` - The hybrid query to plan
    ///
    /// # Returns
    /// Execution plan
    pub fn plan(&self, query: HybridQuery) -> QueryPlan {
        debug!("Planning query with limit {}", query.limit);

        let mut stages = Vec::new();

        // Stage 1: Metadata filter (if present)
        if query.filter.is_some() {
            stages.push(QueryStage::MetadataFilter);
        }

        // Stage 1.5: Spatial filter (if present)
        // By running this before retrieval, we leverage the fast short-circuit
        // to prune the entire query if the spatial constraint yields zero hits.
        if query.spatial_box.is_some()
            || (query.spatial_center.is_some() && query.spatial_radius.is_some())
        {
            stages.push(QueryStage::SpatialFilter);
        }

        // Stage 2: Parallel retrieval
        let mut retrieval_ops = Vec::new();

        if let Some(tier) = &query.memory_tier {
            match tier {
                chimera_core::MemoryTier::Working => {
                    retrieval_ops.push(RetrievalOp::TxBufferSearch);
                }
                chimera_core::MemoryTier::Episodic => {
                    retrieval_ops.push(RetrievalOp::WalTimeIndexSearch);
                }
                chimera_core::MemoryTier::Semantic => {
                    if query.vector.is_some() {
                        retrieval_ops.push(RetrievalOp::VectorSearch);
                    }
                    if query.graph_seed.is_some() {
                        retrieval_ops.push(RetrievalOp::GraphTraversal);
                    }
                }
            }
        } else {
            // Default Semantic approach
            if query.vector.is_some() {
                retrieval_ops.push(RetrievalOp::VectorSearch);
            }
            if query.graph_seed.is_some() {
                retrieval_ops.push(RetrievalOp::GraphTraversal);
            }
        }

        if !retrieval_ops.is_empty() {
            stages.push(QueryStage::ParallelRetrieval(retrieval_ops));
        }

        // Stage 3: Fusion with weights
        stages.push(QueryStage::Fusion(self.weights.clone()));

        QueryPlan {
            query,
            stages,
            parallel: self.parallel_execution,
        }
    }
}

impl Default for QueryPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Execution plan for a hybrid query.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Original query.
    pub query: HybridQuery,
    /// Execution stages.
    pub stages: Vec<QueryStage>,
    /// Whether to execute in parallel.
    pub parallel: bool,
}

/// Weights for fusing different retrieval sources.
#[derive(Debug, Clone)]
pub struct FusionWeights {
    pub vector: f32,
    pub graph: f32,
    pub sparse: f32,
    pub spatial: f32,
}

impl Default for FusionWeights {
    fn default() -> Self {
        Self {
            vector: DEFAULT_WEIGHT_VECTOR,
            graph: DEFAULT_WEIGHT_GRAPH,
            sparse: DEFAULT_WEIGHT_SPARSE,
            spatial: 0.8,
        }
    }
}

/// Query execution stage.
#[derive(Debug, Clone)]
pub enum QueryStage {
    /// Metadata filtering (Stage 1).
    MetadataFilter,
    /// Spatial filtering (Stage 1.5).
    ///
    /// This stage leverages the fast spatial index to reduce the candidate set
    /// for subsequent vector and graph operations.
    SpatialFilter,
    /// Parallel retrieval operations (Stage 2).
    ParallelRetrieval(Vec<RetrievalOp>),
    /// Result fusion (Stage 3).
    Fusion(FusionWeights),
}

/// Retrieval operation type.
#[derive(Debug, Clone)]
pub enum RetrievalOp {
    /// Vector similarity search.
    VectorSearch,
    /// Graph traversal from seed.
    GraphTraversal,
    /// Sparse/lexical search.
    SparseSearch,
    /// Working memory search (TxBuffer).
    TxBufferSearch,
    /// Episodic memory search (WAL time-index).
    WalTimeIndexSearch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chimera_core::traits::QueryWeights;
    use chimera_core::Embedding;
    use chimera_core::NamespaceId;

    #[test]
    fn test_planner_vector_only() {
        let planner = QueryPlanner::new();

        let query = HybridQuery {
            namespace_id: NamespaceId::default_ns(),
            collection: "test".to_string(),
            vector: Some(Embedding::new(vec![1.0, 2.0, 3.0])),
            sparse: None,
            filter: None,
            graph_seed: None,
            graph_hops: 2,
            limit: 10,
            spatial_box: None,
            spatial_center: None,
            spatial_radius: None,
            weights: QueryWeights::default(),
            memory_tier: None,
            recency_weight: 0.5,
        };

        let plan = planner.plan(query);

        assert_eq!(plan.stages.len(), 2); // Retrieval + Fusion
        if let QueryStage::Fusion(weights) = &plan.stages[1] {
            assert_eq!(weights.vector, DEFAULT_WEIGHT_VECTOR);
        } else {
            panic!("Expected Fusion stage");
        }
    }

    #[test]
    fn test_planner_spatial_filter() {
        use chimera_core::{BoundingBox, Point3D};
        let planner = QueryPlanner::new();

        let query = HybridQuery {
            namespace_id: NamespaceId::default_ns(),
            collection: "test".to_string(),
            vector: Some(Embedding::new(vec![1.0, 2.0, 3.0])),
            sparse: None,
            filter: None,
            graph_seed: None,
            graph_hops: 2,
            limit: 10,
            spatial_box: Some(BoundingBox::new(
                Point3D::new(0.0, 0.0, 0.0),
                Point3D::new(10.0, 10.0, 10.0),
            )),
            spatial_center: None,
            spatial_radius: None,
            weights: QueryWeights::default(),
            memory_tier: None,
            recency_weight: 0.5,
        };

        let plan = planner.plan(query);

        // Stages: SpatialFilter -> ParallelRetrieval -> Fusion
        assert_eq!(plan.stages.len(), 3);
        assert!(matches!(plan.stages[0], QueryStage::SpatialFilter));
        if let QueryStage::ParallelRetrieval(ops) = &plan.stages[1] {
            assert_eq!(ops.len(), 1); // VectorSearch only
            assert!(matches!(ops[0], RetrievalOp::VectorSearch));
        }
    }

    #[test]
    fn test_planner_full_hybrid() {
        let planner = QueryPlanner::new();

        let query = HybridQuery {
            namespace_id: NamespaceId::default_ns(),
            collection: "test".to_string(),
            vector: Some(Embedding::new(vec![1.0, 2.0, 3.0])),
            sparse: None,
            filter: Some("category = 'test'".to_string()),
            graph_seed: None,
            spatial_box: None,
            spatial_center: None,
            spatial_radius: None,
            graph_hops: 2,
            limit: 10,
            weights: QueryWeights::default(),
            memory_tier: None,
            recency_weight: 0.5,
        };

        let plan = planner.plan(query);

        assert_eq!(plan.stages.len(), 3); // Filter + Retrieval + Fusion
        if let QueryStage::ParallelRetrieval(ops) = &plan.stages[1] {
            assert_eq!(ops.len(), 1); // Vector (Sparse removed)
        }
    }
}
