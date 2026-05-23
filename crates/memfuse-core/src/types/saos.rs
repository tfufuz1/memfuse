//! # MemFuse SAOS Data Structures
//!
//! This module defines the high-level data structures and types used by the
//! Sovereign Agentic Operating System (SAOS) layer of MemFuse, including
//! namespaces, token budgets, fusion weights, and hybrid query builders.

use super::domain::DocId;
use super::filter::FilterExpr;
use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// Unique identifier for a Namespace.
pub struct NamespaceId(u64);

impl NamespaceId {
    /// Creates a new NamespaceId from a raw u64.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw u64 value.
    pub fn inner(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NS-{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Token budget configuration for LLM context management.
pub struct TokenBudget {
    /// Maximum allowed tokens.
    pub max_tokens: usize,
    /// Reserved tokens for system prompts or output.
    pub reserve_tokens: usize,
}

impl TokenBudget {
    /// Creates a new TokenBudget.
    pub fn new(max_tokens: usize, reserve_tokens: usize) -> Self {
        Self {
            max_tokens,
            reserve_tokens,
        }
    }

    /// Returns the available tokens (max - reserve).
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Normalized fusion weights for hybrid search.
pub struct FusionWeights {
    vector: f32,
    text: f32,
    graph: f32,
    metadata: f32,
}

impl FusionWeights {
    /// Creates a new FusionWeights instance.
    ///
    /// Weights must sum exactly to 1.0.
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

    /// Returns the vector signal weight.
    pub fn vector(&self) -> f32 {
        self.vector
    }

    /// Returns the text (BM25) signal weight.
    pub fn text(&self) -> f32 {
        self.text
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Defines cross-namespace isolation guarantees.
pub enum IsolationLevel {
    /// No shared data between namespaces.
    Strict,
    /// Namespaces can read shared global data.
    SharedRead,
    /// Logical isolation within the same physical storage.
    Logical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A chunk of context for LLM budget allocation.
pub struct ContextChunk {
    /// Document ID.
    pub doc_id: DocId,
    /// Text content.
    pub content: String,
    /// Relevance score.
    pub relevance: f32,
    /// Calculated token count.
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// An aggregated context window constrained by a token budget.
pub struct ContextWindow {
    /// List of context chunks.
    pub chunks: Vec<ContextChunk>,
    /// Total tokens in the window.
    pub total_tokens: usize,
    /// Whether the context was truncated to fit the budget.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Evaluated result for hybrid/4-signal search.
pub struct ScoredEntry {
    /// Unique identifier.
    pub id: String,
    /// Aggregated similarity score.
    pub final_score: f32,
    /// Associated metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A unified query traversing multiple index signals.
pub struct HybridQuery {
    /// Optional keyword search query.
    pub text_query: Option<String>,
    /// Optional semantic search vector.
    pub vector_query: Option<Vec<f32>>,
    /// Optional starting node for graph traversal.
    pub graph_start_node: Option<String>,
    /// Weights for signal fusion.
    pub fusion_weights: FusionWeights,
    /// Optional metadata filter.
    pub filter: Option<FilterExpr>,
    /// Number of results to return.
    pub k: usize,
}

impl HybridQuery {
    /// Returns a new HybridQueryBuilder.
    pub fn builder() -> HybridQueryBuilder {
        HybridQueryBuilder::default()
    }
}

#[derive(Default)]
/// Builder for HybridQuery to improve DX.
pub struct HybridQueryBuilder {
    text_query: Option<String>,
    vector_query: Option<Vec<f32>>,
    graph_start_node: Option<String>,
    fusion_weights: Option<FusionWeights>,
    filter: Option<FilterExpr>,
    k: Option<usize>,
}

impl HybridQueryBuilder {
    /// Creates a new HybridQueryBuilder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text query.
    pub fn with_text_query(mut self, q: impl Into<String>) -> Self {
        self.text_query = Some(q.into());
        self
    }

    /// Sets the vector query.
    pub fn with_vector_query(mut self, v: Vec<f32>) -> Self {
        self.vector_query = Some(v);
        self
    }

    /// Sets the graph starting node.
    pub fn with_graph_start_node(mut self, start: impl Into<String>) -> Self {
        self.graph_start_node = Some(start.into());
        self
    }

    /// Sets the fusion weights.
    pub fn with_fusion_weights(mut self, weights: FusionWeights) -> Self {
        self.fusion_weights = Some(weights);
        self
    }

    /// Sets the metadata filter.
    pub fn with_filter(mut self, filter: FilterExpr) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Sets the number of results to return.
    pub fn with_k(mut self, k: usize) -> Self {
        self.k = Some(k);
        self
    }

    /// Builds the HybridQuery.
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

        assert_eq!(query.text_query.unwrap() // unwrap, "test query");
        assert_eq!(query.vector_query.unwrap() // unwrap, vec![0.1, 0.2]);
        assert_eq!(query.k, 5);
        // Default weights: vector=1.0, others=0.0
        assert_eq!(query.fusion_weights.vector(), 1.0);
    }

    #[test]
    fn test_hybrid_query_builder_custom_weights() {
        let weights = FusionWeights::new(0.4, 0.4, 0.1, 0.1).unwrap() // unwrap;
        let query = HybridQuery::builder()
            .with_fusion_weights(weights.clone())
            .build()
            .unwrap() // unwrap;

        assert_eq!(query.fusion_weights, weights);
    }

    #[test]
    fn test_hybrid_query_builder_defaults() {
        let query = HybridQuery::builder().build().unwrap() // unwrap;
        assert_eq!(query.k, 10);
        assert_eq!(query.fusion_weights.vector(), 1.0);
        assert!(query.text_query.is_none());
        assert!(query.vector_query.is_none());
    }
}
