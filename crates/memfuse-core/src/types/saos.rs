use super::domain::DocId;
use super::filter::FilterExpr;
use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Normalized fusion weights for hybrid search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusionWeights {
    vector: f32,
    text: f32,
    graph: f32,
    metadata: f32,
}

impl FusionWeights {
    /// Creates 3-signal fusion weights (vector, text, graph) summing to 1.0.
    pub fn new(vector: f32, text: f32, graph: f32) -> Result<Self> {
        Self::new_with_metadata(vector, text, graph, 0.0)
    }

    /// Creates 4-signal fusion weights. Currently, metadata must be 0.0 until metadata signal ranking is implemented.
    pub fn new_with_metadata(vector: f32, text: f32, graph: f32, metadata: f32) -> Result<Self> {
        if !vector.is_finite() || !text.is_finite() || !graph.is_finite() || !metadata.is_finite() {
            return Err(MemFuseError::InvalidInput(
                "Fusion weights must be finite numbers".into(),
            ));
        }
        // FIND-COR-004: Guard against negative weights
        if vector < 0.0 || text < 0.0 || graph < 0.0 || metadata < 0.0 {
            return Err(MemFuseError::InvalidInput(
                "Fusion weights must be non-negative".into(),
            ));
        }
        if metadata > 0.0 {
            return Err(MemFuseError::InvalidInput(
                "Metadata signal is not yet implemented; metadata weight must be 0.0".into(),
            ));
        }
        let sum = vector + text + graph + metadata;
        if (sum - 1.0).abs() > 1e-6 {
            return Err(MemFuseError::InvalidInput(format!(
                "Fusion weights must sum exactly to 1.0, got {}",
                sum
            )));
        }
        Ok(Self {
            vector,
            text,
            graph,
            metadata,
        })
    }

    /// Returns the vector signal weight component.
    pub fn vector(&self) -> f32 {
        self.vector
    }

    /// Returns the text signal weight component.
    pub fn text(&self) -> f32 {
        self.text
    }

    /// Returns the graph signal weight component.
    pub fn graph(&self) -> f32 {
        self.graph
    }

    /// Returns the metadata signal weight component.
    pub fn metadata(&self) -> f32 {
        self.metadata
    }
}

/// A chunk of context for LLM budget allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    /// Document identifier associated with this chunk.
    pub doc_id: DocId,
    /// Text content of the chunk.
    pub content: String,
    /// Computed relevance score.
    pub relevance: f32,
    /// Estimated token count for LLM context calculation.
    pub token_count: usize,
    /// Optional metadata associated with the chunk.
    pub metadata: Option<serde_json::Value>,
}

/// An aggregated context window constrained by a token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    /// List of included context chunks.
    pub chunks: Vec<ContextChunk>,
    /// Total aggregated token count.
    pub total_tokens: usize,
    /// Indicates whether context chunks were truncated to fit the budget.
    pub truncated: bool,
}

impl std::fmt::Display for ContextWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, chunk) in self.chunks.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", chunk.content)?;
        }
        Ok(())
    }
}

/// Evaluated result for hybrid/4-signal search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredEntry {
    /// Unique result identifier.
    pub id: String,
    /// Combined final score after signal fusion.
    pub final_score: f32,
    /// Associated entry metadata.
    pub metadata: Option<serde_json::Value>,
}

/// A unified query traversing multiple index signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    /// Optional full-text query string.
    pub text_query: Option<String>,
    /// Optional vector query embedding.
    pub vector_query: Option<Vec<f32>>,
    /// Optional starting node ID for graph traversal search.
    pub graph_start_node: Option<String>,
    /// Fusion weights across vector, text, and graph signals.
    pub fusion_weights: FusionWeights,
    /// Optional metadata expression filter.
    pub filter: Option<FilterExpr>,
    /// Maximum number of search results to return.
    pub k: usize,
}

impl HybridQuery {
    /// Creates a new `HybridQueryBuilder`.
    pub fn builder() -> HybridQueryBuilder {
        HybridQueryBuilder::default()
    }
}

/// Builder for HybridQuery to improve DX.
#[derive(Default)]
pub struct HybridQueryBuilder {
    text_query: Option<String>,
    vector_query: Option<Vec<f32>>,
    graph_start_node: Option<String>,
    fusion_weights: Option<FusionWeights>,
    filter: Option<FilterExpr>,
    k: Option<usize>,
}

impl HybridQueryBuilder {
    /// Creates a new `HybridQueryBuilder` with empty default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the full-text search query string.
    pub fn with_text_query(mut self, q: impl Into<String>) -> Self {
        self.text_query = Some(q.into());
        self
    }

    /// Sets the vector query embedding.
    pub fn with_vector_query(mut self, v: Vec<f32>) -> Self {
        self.vector_query = Some(v);
        self
    }

    /// Sets the graph start node for graph traversal search.
    pub fn with_graph_start_node(mut self, start: impl Into<String>) -> Self {
        self.graph_start_node = Some(start.into());
        self
    }

    /// Sets custom signal fusion weights.
    pub fn with_fusion_weights(mut self, weights: FusionWeights) -> Self {
        self.fusion_weights = Some(weights);
        self
    }

    /// Sets a metadata expression filter on the query.
    pub fn with_filter(mut self, filter: FilterExpr) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Sets the top-K limit for the query.
    pub fn with_k(mut self, k: usize) -> Self {
        self.k = Some(k);
        self
    }

    /// Builds the `HybridQuery` instance.
    ///
    /// # Errors
    /// Returns `Err` if query parameters or fusion weights fail validation.
    pub fn build(self) -> Result<HybridQuery> {
        Ok(HybridQuery {
            text_query: self.text_query,
            vector_query: self.vector_query,
            graph_start_node: self.graph_start_node,
            fusion_weights: self.fusion_weights.unwrap_or(
                // Use a known-safe default to avoid unwrap()
                FusionWeights {
                    vector: 1.0,
                    text: 0.0,
                    graph: 0.0,
                    metadata: 0.0,
                },
            ),
            filter: self.filter,
            k: self.k.unwrap_or(10),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenBudget;

    #[test]
    fn test_fusion_weights_valid() {
        let w = FusionWeights::new(0.5, 0.5, 0.0).expect("valid");
        assert_eq!(w.vector(), 0.5);
        assert_eq!(w.text(), 0.5);
    }

    #[test]
    fn test_fusion_weights_invalid_sum() {
        let result = FusionWeights::new(0.5, 0.6, 0.0);
        assert!(result.is_err());
        if let Err(MemFuseError::InvalidInput(msg)) = result {
            assert!(msg.contains("must sum exactly to 1.0"));
        } else {
            panic!("Expected InvalidInput error");
        }
    }

    #[test]
    fn test_hybrid_query_builder_happy_path() {
        let query = HybridQuery::builder()
            .with_text_query("test query")
            .with_vector_query(vec![0.1, 0.2])
            .with_k(5)
            .build()
            .expect("build ok");

        assert_eq!(query.text_query.unwrap(), "test query");
        assert_eq!(query.vector_query.unwrap(), vec![0.1, 0.2]);
        assert_eq!(query.k, 5);
        // Default weights: vector=1.0, others=0.0
        assert_eq!(query.fusion_weights.vector(), 1.0);
    }

    #[test]
    fn test_hybrid_query_builder_custom_weights() {
        let weights = FusionWeights::new(0.4, 0.5, 0.1).unwrap();
        let query = HybridQuery::builder()
            .with_fusion_weights(weights.clone())
            .build()
            .unwrap();

        assert_eq!(query.fusion_weights, weights);
    }

    #[test]
    fn test_hybrid_query_builder_defaults() {
        let query = HybridQuery::builder().build().unwrap();
        assert_eq!(query.k, 10);
        assert_eq!(query.fusion_weights.vector(), 1.0);
        assert!(query.text_query.is_none());
        assert!(query.vector_query.is_none());
    }

    #[test]
    fn test_token_budget_edge_cases() {
        let mut budget = TokenBudget::new(100, 20);
        assert_eq!(budget.available(), 80);

        budget.consume(50);
        assert_eq!(budget.available(), 30);

        budget.consume(40); // Over consumption
        assert_eq!(budget.available(), 0);
        assert_eq!(budget.consumed(), 90);
    }

    #[test]
    fn test_context_window_serialization() {
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "hello".to_string(),
            relevance: 1.0,
            token_count: 1,
            metadata: None,
        };
        let window = ContextWindow {
            chunks: vec![chunk],
            total_tokens: 1,
            truncated: false,
        };
        let ser = serde_json::to_string(&window).unwrap();
        let deser: ContextWindow = serde_json::from_str(&ser).unwrap();
        assert_eq!(deser.total_tokens, 1);
        assert_eq!(deser.chunks.len(), 1);
    }
}
