use super::domain::{DocId, EntityId, PprConfig};
use super::filter::FilterExpr;
use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Strategy used for graph retrieval in hybrid search queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphTraversalStrategy {
    /// Multi-hop BFS traversal with hop score decay (default max_hops = 3).
    Hops {
        /// Maximum traversal hop depth.
        max_hops: usize,
    },
    /// Personalized PageRank power iteration starting from seed nodes.
    PersonalizedPageRank(PprConfig),
}

impl Default for GraphTraversalStrategy {
    fn default() -> Self {
        Self::Hops { max_hops: 3 }
    }
}

/// Normalized fusion weights for hybrid search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusionWeights {
    vector: f32,
    text: f32,
    graph: f32,
    /// Reserved for future metadata-signal fusion; currently always 0.0. Do not expose a public constructor parameter for this until the fusion logic is implemented.
    metadata: f32,
}

// ANCHOR[REFACTOR:CORE-FUSION-001] STATUS:DONE (TS: 2026-08-28T16:49:00Z) (SESSION: a3f29c1d)
// AUFGABE : Remove FusionWeights::new_with_metadata from public API (metadata fusion signal not implemented)
// GATE    : cargo test -p memfuse-core

impl FusionWeights {
    /// Creates 3-signal fusion weights (vector, text, graph) summing to 1.0.
    pub fn new(vector: f32, text: f32, graph: f32) -> Result<Self> {
        if vector.is_nan() || text.is_nan() || graph.is_nan() {
            return Err(MemFuseError::InvalidInput(
                "Fusion weights must not be NaN".into(),
            ));
        }
        if !vector.is_finite() || !text.is_finite() || !graph.is_finite() {
            return Err(MemFuseError::InvalidInput(
                "Fusion weights must be finite numbers".into(),
            ));
        }
        // FIND-COR-004: Guard against negative weights
        if vector < 0.0 || text < 0.0 || graph < 0.0 {
            return Err(MemFuseError::InvalidInput(
                "Fusion weights must be non-negative".into(),
            ));
        }
        let sum = vector + text + graph;
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
            metadata: 0.0,
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
    /// Reserved for future metadata-signal fusion; currently always 0.0. Do not expose a public constructor parameter for this until the fusion logic is implemented.
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
    /// LLM-generiertes Kontextpräfix (50–100 Tokens).
    /// None = kein Contextual Retrieval für diesen Chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contextual_prefix: Option<String>,
}

impl ContextChunk {
    /// Returns only the raw content (no contextual prefix).
    ///
    /// # Deprecated
    /// Use [`Self::combined_text_owned()`] for Contextual BM25 indexing
    /// (prefix + content). Use [`Self::content`] for raw content access.
    /// This method will be removed in the next breaking release.
    #[deprecated(
        since = "0.1.0",
        note = "Use combined_text_owned() for BM25 indexing with contextual prefix, \
                or access .content directly for raw content"
    )]
    pub fn combined_text_for_indexing(&self) -> &str {
        &self.content
    }

    /// Kombinierten Text für BM25-Indizierung und Embedding.
    /// Entspricht Anthropic Contextual BM25: prefix + "\n\n" + content.
    ///
    /// Allokiert nur wenn `contextual_prefix` gesetzt. Akzeptabel für
    /// Ingestion-Pipeline (1× pro Chunk, kein Hot-Path).
    pub fn combined_text_owned(&self) -> String {
        match &self.contextual_prefix {
            Some(prefix) if !prefix.is_empty() => {
                let mut combined = String::with_capacity(prefix.len() + 2 + self.content.len());
                combined.push_str(prefix);
                combined.push_str("\n\n");
                combined.push_str(&self.content);
                combined
            }
            _ => self.content.clone(),
        }
    }

    /// Kombinierter Token-Count (Heuristik: chars/4 ≈ Tokens).
    /// Verwendet für Budget-Management in `memfuse-db/src/context.rs`.
    pub fn combined_token_count(&self) -> usize {
        match &self.contextual_prefix {
            Some(p) if !p.is_empty() => self.token_count + p.len() / 4,
            _ => self.token_count,
        }
    }

    /// Gibt true zurück wenn contextual_prefix gesetzt und nicht leer.
    pub fn has_context_prefix(&self) -> bool {
        self.contextual_prefix
            .as_deref()
            .is_some_and(|p| !p.is_empty())
    }
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
    /// Graph traversal strategy (Hops or PersonalizedPageRank).
    #[serde(default)]
    pub graph_strategy: GraphTraversalStrategy,
    /// Fusion weights across vector, text, and graph signals.
    pub fusion_weights: FusionWeights,
    /// Optional metadata expression filter.
    pub filter: Option<FilterExpr>,
    /// Optional entity ID to filter/boost results in the same community.
    pub same_community_as: Option<EntityId>,
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
    graph_strategy: Option<GraphTraversalStrategy>,
    fusion_weights: Option<FusionWeights>,
    filter: Option<FilterExpr>,
    same_community_as: Option<EntityId>,
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

    /// Sets the graph traversal strategy (Hops or PersonalizedPageRank).
    pub fn with_graph_strategy(mut self, strategy: GraphTraversalStrategy) -> Self {
        self.graph_strategy = Some(strategy);
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

    /// Sets the community context entity filter/boost.
    pub fn with_same_community_as(mut self, entity_id: EntityId) -> Self {
        self.same_community_as = Some(entity_id);
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
            graph_strategy: self.graph_strategy.unwrap_or_default(),
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
            same_community_as: self.same_community_as,
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
        let w = FusionWeights::new(0.5, 0.5, 0.0).expect("valid"); // expect
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
            .expect("build ok"); // expect

        assert_eq!(query.text_query.unwrap(), "test query"); // unwrap
        assert_eq!(query.vector_query.unwrap(), vec![0.1, 0.2]); // unwrap
        assert_eq!(query.k, 5);
        // Default weights: vector=1.0, others=0.0
        assert_eq!(query.fusion_weights.vector(), 1.0);
    }

    #[test]
    fn test_hybrid_query_builder_custom_weights() {
        let weights = FusionWeights::new(0.4, 0.5, 0.1).unwrap(); // unwrap
        let query = HybridQuery::builder()
            .with_fusion_weights(weights.clone())
            .build()
            .unwrap(); // unwrap

        assert_eq!(query.fusion_weights, weights);
    }

    #[test]
    fn test_hybrid_query_builder_defaults() {
        let query = HybridQuery::builder().build().unwrap(); // unwrap
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
    fn test_fusion_weights_nan() {
        let res = FusionWeights::new(f32::NAN, 0.5, 0.5);
        assert!(res.is_err());
        if let Err(MemFuseError::InvalidInput(msg)) = res {
            assert_eq!(msg, "Fusion weights must not be NaN");
        } else {
            panic!("Expected InvalidInput error");
        }
    }

    proptest::proptest! {
        #[test]
        fn prop_fusion_weights_never_panics(
            v in proptest::num::f32::ANY,
            t in proptest::num::f32::ANY,
            g in proptest::num::f32::ANY,
        ) {
            let res = FusionWeights::new(v, t, g);
            if v.is_nan() || t.is_nan() || g.is_nan() {
                let err = res.expect_err("NaN inputs must return an error");
                if let MemFuseError::InvalidInput(msg) = err {
                    proptest::prop_assert_eq!(msg, "Fusion weights must not be NaN");
                } else {
                    proptest::prop_assert!(false, "Expected InvalidInput for NaN");
                }
            } else if !v.is_finite() || !t.is_finite() || !g.is_finite() {
                proptest::prop_assert!(res.is_err());
            }
        }
    }

    #[test]
    fn test_context_window_serialization() {
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "hello".to_string(),
            relevance: 1.0,
            token_count: 1,
            metadata: None,
            contextual_prefix: None,
        };
        let window = ContextWindow {
            chunks: vec![chunk],
            total_tokens: 1,
            truncated: false,
        };
        let ser = serde_json::to_string(&window).unwrap(); // unwrap
        let deser: ContextWindow = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(deser.total_tokens, 1);
        assert_eq!(deser.chunks.len(), 1);
        assert!(deser.chunks[0].contextual_prefix.is_none());
    }

    #[test]
    fn test_combined_text_with_prefix() {
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "Raw content".to_string(),
            relevance: 1.0,
            token_count: 2,
            metadata: None,
            contextual_prefix: Some("Dokument Kontext".to_string()),
        };
        assert_eq!(
            chunk.combined_text_owned(),
            "Dokument Kontext\n\nRaw content"
        );
        assert!(chunk.has_context_prefix());
    }

    #[test]
    fn test_combined_text_without_prefix_returns_content() {
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "Raw content".to_string(),
            relevance: 1.0,
            token_count: 2,
            metadata: None,
            contextual_prefix: None,
        };
        assert_eq!(chunk.combined_text_owned(), "Raw content");
        assert!(!chunk.has_context_prefix());
    }

    #[test]
    fn test_combined_text_empty_prefix_returns_content() {
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "Raw content".to_string(),
            relevance: 1.0,
            token_count: 2,
            metadata: None,
            contextual_prefix: Some("".to_string()),
        };
        assert_eq!(chunk.combined_text_owned(), "Raw content");
        assert!(!chunk.has_context_prefix());
    }

    #[test]
    fn test_serde_backward_compat_no_prefix_field() {
        let json = r#"{"doc_id":1,"content":"X","relevance":0.5,"token_count":1}"#;
        let chunk: ContextChunk = serde_json::from_str(json).expect("deserialize"); // expect
        assert!(chunk.contextual_prefix.is_none());
    }

    #[test]
    fn test_combined_token_count_with_prefix() {
        let chunk_no_prefix = ContextChunk {
            doc_id: DocId::new(1),
            content: "Raw content".to_string(),
            relevance: 1.0,
            token_count: 10,
            metadata: None,
            contextual_prefix: None,
        };
        assert_eq!(chunk_no_prefix.combined_token_count(), 10);

        let chunk_with_prefix = ContextChunk {
            doc_id: DocId::new(1),
            content: "Raw content".to_string(),
            relevance: 1.0,
            token_count: 10,
            metadata: None,
            contextual_prefix: Some("1234567812345678".to_string()), // 16 chars -> +4 tokens
        };
        assert_eq!(chunk_with_prefix.combined_token_count(), 14);
        assert!(chunk_with_prefix.combined_token_count() > chunk_with_prefix.token_count);
    }
}
