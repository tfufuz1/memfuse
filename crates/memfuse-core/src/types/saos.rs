use super::domain::DocId;
use super::filter::FilterExpr;
use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Unique identifier for a Namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespaceId(u64);

impl NamespaceId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn inner(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NS-{}", self.0)
    }
}

/// Token budget configuration for LLM context management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    pub max_tokens: usize,
    pub reserve_tokens: usize,
}

impl TokenBudget {
    pub fn new(max_tokens: usize, reserve_tokens: usize) -> Self {
        Self {
            max_tokens,
            reserve_tokens,
        }
    }

    pub fn available(&self) -> usize {
        self.max_tokens.saturating_sub(self.reserve_tokens)
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            reserve_tokens: 512,
        }
    }
}

/// Normalized fusion weights for hybrid search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusionWeights {
    vector: f32,
    text: f32,
    graph: f32,
    metadata: f32,
}

impl FusionWeights {
    pub fn new(vector: f32, text: f32, graph: f32, metadata: f32) -> Result<Self> {
        let sum = vector + text + graph + metadata;
        if (sum - 1.0).abs() > 1e-6 {
            // C-2 issue resolved! f32::EPSILON -> 1e-6
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

    pub fn vector(&self) -> f32 {
        self.vector
    }

    pub fn text(&self) -> f32 {
        self.text
    }
}

/// Defines cross-namespace isolation guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolationLevel {
    Strict,
    SharedRead,
    Logical,
}

/// A chunk of context for LLM budget allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    pub doc_id: DocId,
    pub content: String,
    pub relevance: f32,
    pub token_count: usize,
}

/// An aggregated context window constrained by a token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub chunks: Vec<ContextChunk>,
    pub total_tokens: usize,
    pub truncated: bool,
}

/// Evaluated result for hybrid/4-signal search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredEntry {
    pub id: String,
    pub final_score: f32,
    pub metadata: Option<serde_json::Value>,
}

/// A unified query traversing multiple index signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    pub text_query: Option<String>,
    pub vector_query: Option<Vec<f32>>,
    pub graph_start_node: Option<String>,
    pub fusion_weights: FusionWeights,
    pub filter: Option<FilterExpr>,
    pub k: usize,
}

impl HybridQuery {
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text_query(mut self, q: impl Into<String>) -> Self {
        self.text_query = Some(q.into());
        self
    }

    pub fn with_vector_query(mut self, v: Vec<f32>) -> Self {
        self.vector_query = Some(v);
        self
    }

    pub fn with_graph_start_node(mut self, start: impl Into<String>) -> Self {
        self.graph_start_node = Some(start.into());
        self
    }

    pub fn with_fusion_weights(mut self, weights: FusionWeights) -> Self {
        self.fusion_weights = Some(weights);
        self
    }

    pub fn with_filter(mut self, filter: FilterExpr) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_k(mut self, k: usize) -> Self {
        self.k = Some(k);
        self
    }

    pub fn build(self) -> Result<HybridQuery> {
        Ok(HybridQuery {
            text_query: self.text_query,
            vector_query: self.vector_query,
            graph_start_node: self.graph_start_node,
            fusion_weights: self.fusion_weights.unwrap_or(
                // Use a known-safe default to avoid expect("test")
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

    #[test]
    fn test_fusion_weights_valid() {
        let w = FusionWeights::new(0.5, 0.5, 0.0, 0.0).expect("valid");
        assert_eq!(w.vector(), 0.5);
        assert_eq!(w.text(), 0.5);
    }

    #[test]
    fn test_fusion_weights_invalid_sum() {
        let result = FusionWeights::new(0.5, 0.6, 0.0, 0.0);
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

        assert_eq!(query.text_query.expect("test"), "test query");
        assert_eq!(query.vector_query.expect("test"), vec![0.1, 0.2]);
        assert_eq!(query.k, 5);
        // Default weights: vector=1.0, others=0.0
        assert_eq!(query.fusion_weights.vector(), 1.0);
    }

    #[test]
    fn test_hybrid_query_builder_custom_weights() {
        let weights = FusionWeights::new(0.4, 0.4, 0.1, 0.1).expect("test");
        let query = HybridQuery::builder()
            .with_fusion_weights(weights.clone())
            .build()
            .expect("test");

        assert_eq!(query.fusion_weights, weights);
    }

    #[test]
    fn test_hybrid_query_builder_defaults() {
        let query = HybridQuery::builder().build().expect("test");
        assert_eq!(query.k, 10);
        assert_eq!(query.fusion_weights.vector(), 1.0);
        assert!(query.text_query.is_none());
        assert!(query.vector_query.is_none());
    }
}
