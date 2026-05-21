//! Core type definitions for MemFuse.
//!
//! Simplified from ChimeraDB — no rkyv, no namespaces, string-based IDs.

// ANCHOR:ARCH:TYPES-001 — Zentrale Datentypen für den gesamten Workspace.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// INVARIANTEN: DocId=#[repr(transparent)] u64 via blake3, TOMBSTONE_BIT=Bit63 in SeqNo.
// ACHTUNG: Änderungen an DocId::from_key() brechen ALLE bestehenden Datenbanken!

use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Defines a frozen workflow state acting as a savepoint.
#[derive(Debug, Clone)]
pub struct WorkflowState {
    /// Associated transaction.
    pub tx: TxId,
    /// Agent memory graph state footprint.
    pub graph_hash: String,
}

// ANCHOR:ARCH:TOMBSTONE-001 — Bit 63 der SeqNo markiert Tombstones.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Verwendet in: lsm.rs, compaction.rs. Raw-SeqNo = seq & !TOMBSTONE_BIT.
/// Bit mask for identifying tombstones in sequence numbers.
pub const TOMBSTONE_BIT: u64 = 1 << 63;

/// Internal document identifier (u64, not exposed to users).
///
/// `DocId` is typically derived from a string key via hashing (blake3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct DocId(pub u64);

impl DocId {
    pub const MAX: Self = Self(u64::MAX);
    pub const MIN: Self = Self(0);

    /// Creates a new DocId from a raw u64.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Derive a DocId from a user-provided string key via blake3 hash.
    pub fn from_key(key: &str) -> Result<Self> {
        // ANCHOR:DEBT:TYPES-002 AGENT:01 STATUS:DONE PRIO:3
        // SAFETY: blake3::hash() always returns a 32-byte hash.
        // try_from_key() only fails if the hash is shorter than 8 bytes.
        Self::try_from_key(key)
    }

    /// Safely derive a DocId from a user-provided string key.
    ///
    /// Uses blake3 hash and safe slice indexing.
    pub fn try_from_key(key: &str) -> Result<Self> {
        let hash = blake3::hash(key.as_bytes());
        let bytes = hash
            .as_bytes()
            .get(..8)
            .ok_or_else(|| MemFuseError::Internal("Blake3 hash too short".to_string()))?;

        let buf: [u8; 8] = bytes.try_into().map_err(|_| {
            MemFuseError::Internal("Failed to convert hash slice to array".to_string())
        })?;
        Ok(Self(u64::from_le_bytes(buf)))
    }
}

impl From<u64> for DocId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for DocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DocId({})", self.0)
    }
}

/// Internal entity identifier for graph nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Creates a new EntityId from a raw u64.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl From<u64> for EntityId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

/// Transaction identifier used to coordinate atomic writes and isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct TxId(pub u64);

impl TxId {
    /// Creates a new TxId from a raw u64.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TxId({})", self.0)
    }
}

/// Distance metric for vector comparison.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum DistanceMetric {
    /// Cosine distance (1 - cosine similarity).
    #[default]
    Cosine,
    /// Euclidean distance (L2).
    Euclidean,
    /// Dot product.
    DotProduct,
}

/// Vector embedding representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// Raw vector data.
    pub data: Vec<f32>,
}

impl Embedding {
    /// Creates a new Embedding from a vector of floats.
    pub fn new(data: Vec<f32>) -> Self {
        Self { data }
    }

    /// Returns the dimension of the embedding.
    #[inline]
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    /// Returns the data as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Computes the L2 norm of the embedding.
    pub fn l2_norm(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Returns a normalized copy of this embedding.
    pub fn normalize(&self) -> Self {
        let norm = self.l2_norm();
        if norm == 0.0 {
            return self.clone();
        }
        Self::new(self.data.iter().map(|x| x / norm).collect())
    }
}

/// A scored search result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScoredDocument {
    /// The document identifier.
    pub doc_id: DocId,
    /// The similarity score.
    pub score: f32,
}

impl ScoredDocument {
    /// Creates a new ScoredDocument.
    pub fn new(doc_id: DocId, score: f32) -> Self {
        Self { doc_id, score }
    }
}

/// Graph entity (node) representing a concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier for the entity.
    pub id: EntityId,
    /// Human-readable name of the entity.
    pub name: String,
    /// Type categorization of the entity.
    pub entity_type: String,
}

impl Entity {
    /// Creates a new Entity.
    pub fn new(id: EntityId, name: impl Into<String>, entity_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            entity_type: entity_type.into(),
        }
    }
}

/// Graph edge (relation) between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source entity identifier.
    pub from: EntityId,
    /// Target entity identifier.
    pub to: EntityId,
    /// Relationship label.
    pub label: String,
    /// Strength or weight of the relationship.
    pub weight: f32,
}

impl Edge {
    /// Creates a new Edge with default weight 1.0.
    pub fn new(from: EntityId, to: EntityId, label: impl Into<String>) -> Self {
        Self {
            from,
            to,
            label: label.into(),
            weight: 1.0,
        }
    }

    /// Builder pattern to set the weight.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

// ANCHOR:ARCH:BUDGET-001 — Memory-Budgeting verhindert OOM in Produktionsumgebungen.
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Backpressure: >80% → 5ms Sleep, >95% → MemFuseError::MemoryBudgetExceeded.
//
// ANCHOR:SPEC:WP-4.1-MMAP-001 — Auf Memory-Mapped I/O umstellen für zero-copy Zugriff.
// WP:WP-4.1 PRIO:4 NEEDS:NONE
// AGENT:02 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
/// Resource budget for memory management.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Maximum allowed memory usage in bytes.
    pub memory_limit: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            memory_limit: 2 * 1024 * 1024 * 1024, // 2GB
        }
    }
}

/// Tracks resource usage against a budget.
#[derive(Debug)]
pub struct ResourceTracker {
    /// The configured budget.
    budget: ResourceBudget,
    /// Current memory usage in bytes.
    memory_used: std::sync::atomic::AtomicU64,
}

impl ResourceTracker {
    /// Creates a new ResourceTracker with the given budget.
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            budget,
            memory_used: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        loop {
            let current = self.memory_used.load(std::sync::atomic::Ordering::Acquire);
            if current + bytes > self.budget.memory_limit {
                return Err(MemFuseError::MemoryBudgetExceeded {
                    used_mb: (current + bytes) / (1024 * 1024),
                    limit_mb: self.budget.memory_limit / (1024 * 1024),
                });
            }
            if self
                .memory_used
                .compare_exchange(
                    current,
                    current + bytes,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    pub fn release_memory(&self, bytes: u64) {
        self.memory_used
            .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn memory_used(&self) -> u64 {
        self.memory_used.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn budget(&self) -> &ResourceBudget {
        &self.budget
    }

    /// Returns true if memory usage is below 95% of the limit.
    pub fn has_memory_capacity(&self) -> bool {
        self.memory_used() < (self.budget.memory_limit as f64 * 0.95) as u64
    }

    /// Suspends execution briefly if memory usage exceeds 80% to apply backpressure.
    pub async fn apply_backpressure(&self) {
        if self.memory_used() >= (self.budget.memory_limit as f64 * 0.80) as u64 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
}

// ANCHOR:ARCH:TYPES-001 — SAOS Domain Types (WP-6.x)

/// Unique identifier for a Namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespaceId(u64);

impl NamespaceId {
    /// Creates a new NamespaceId.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
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
    /// Maximum total tokens allowed.
    pub max_tokens: usize,
    /// Number of tokens to reserve.
    pub reserve_tokens: usize,
}

impl TokenBudget {
    /// Creates a new token budget.
    pub fn new(max_tokens: usize, reserve_tokens: usize) -> Self {
        Self {
            max_tokens,
            reserve_tokens,
        }
    }

    /// Returns the number of tokens available for allocation without underflowing.
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

// ANCHOR:DEBT:TYPES-003 AGENT:01 STATUS:DONE PRIO:3
// Missing getters for graph and metadata weights.
/// Normalized fusion weights for hybrid search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusionWeights {
    vector: f32,
    text: f32,
    graph: f32,
    metadata: f32,
}

impl FusionWeights {
    /// Creates normalized fusion weights. Returns error if weights do not sum exactly to 1.0.
    pub fn new(vector: f32, text: f32, graph: f32, metadata: f32) -> Result<Self> {
        let sum = vector + text + graph + metadata;
        if (sum - 1.0).abs() > f32::EPSILON {
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

    /// Returns the vector weight.
    pub fn vector(&self) -> f32 {
        self.vector
    }

    /// Returns the text weight.
    pub fn text(&self) -> f32 {
        self.text
    }

    /// Returns the graph weight.
    pub fn graph(&self) -> f32 {
        self.graph
    }

    /// Returns the metadata weight.
    pub fn metadata(&self) -> f32 {
        self.metadata
    }
}

/// Defines cross-namespace isolation guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolationLevel {
    /// No cross-namespace access allowed.
    Strict,
    /// Shared read access allowed, but strict write isolation.
    SharedRead,
    /// Logical separation, full cross-access allowed.
    Logical,
}

/// Metadata filter expressions for pre/post filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Exact match: field == value
    Eq {
        field: String,
        value: serde_json::Value,
    },
    /// Greater than: field > value
    Gt {
        field: String,
        value: serde_json::Value,
    },
    /// Less than: field < value
    Lt {
        field: String,
        value: serde_json::Value,
    },
    /// In set: field IN (values)
    In {
        field: String,
        values: Vec<serde_json::Value>,
    },
    /// Logical AND
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT
    Not(Box<FilterExpr>),
}

/// A chunk of context for LLM budget allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    /// Document ID this chunk belongs to.
    pub doc_id: DocId,
    /// Raw text content.
    pub content: String,
    /// Relevance score (0.0 to 1.0).
    pub relevance: f32,
    /// Estimated token count.
    pub token_count: usize,
}

/// An aggregated context window constrained by a token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    /// The selected chunks.
    pub chunks: Vec<ContextChunk>,
    /// Total tokens across all chunks.
    pub total_tokens: usize,
    /// Whether chunks were truncated to meet budget.
    pub truncated: bool,
}

/// Evaluated result for hybrid/4-signal search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredEntry {
    /// The document or entity ID.
    pub id: String,
    /// The finalized score after weight fusion.
    pub final_score: f32,
    /// Optional structured metadata.
    pub metadata: Option<serde_json::Value>,
}

/// A unified query traversing multiple index signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    /// Natural language text query (for BM25).
    pub text_query: Option<String>,
    /// Dense vector embedding (for HNSW).
    pub vector_query: Option<Vec<f32>>,
    /// Node ID for graph traversal (for CSR).
    pub graph_start_node: Option<String>,
    /// Weights dictating signal fusion.
    pub fusion_weights: FusionWeights,
    /// Metadata filter to apply.
    pub filter: Option<FilterExpr>,
    /// Number of results to return.
    pub k: usize,
}

// ----------------------------------------------------------------------------
// TESTING
// ----------------------------------------------------------------------------

#[cfg(test)]
mod saos_tests {
    use super::*;

    // --- TokenBudget Tests ---
    #[test]
    fn test_token_budget_available_calculation() {
        let budget = TokenBudget::new(100, 20);
        assert_eq!(
            budget.available(),
            80,
            "Available tokens should subtract reserve."
        );

        let tight_budget = TokenBudget::new(10, 15);
        assert_eq!(
            tight_budget.available(),
            0,
            "Available tokens should not underflow."
        );
    }

    #[test]
    fn test_token_budget_defaults() {
        let budget = TokenBudget::default();
        assert_eq!(budget.max_tokens, 4096);
        assert_eq!(budget.reserve_tokens, 512);
        assert_eq!(budget.available(), 3584);
    }

    // --- FusionWeights Tests ---
    #[test]
    fn test_fusion_weights_normalization_valid() {
        let weights = FusionWeights::new(0.6, 0.4, 0.0, 0.0).expect("valid weights");
        assert_eq!(weights.vector(), 0.6);
        assert_eq!(weights.text(), 0.4);
    }

    #[test]
    fn test_fusion_weights_invalid_sum() {
        let err = FusionWeights::new(1.0, 1.0, 0.0, 0.0).expect_err("should reject >1.0 sum");
        match err {
            MemFuseError::InvalidInput(msg) => {
                assert!(msg.contains("must sum exactly to 1.0"));
            }
            _ => panic!("Expected InvalidInput, got {:?}", err),
        }
    }

    // --- NamespaceId Tests ---
    #[test]
    fn test_namespace_id_format() {
        let ns = NamespaceId::new(42);
        assert_eq!(ns.inner(), 42);
        assert_eq!(format!("{}", ns), "NS-42");
    }

    // --- IsolationLevel Tests ---
    #[test]
    fn test_isolation_level_equality() {
        assert_eq!(IsolationLevel::Strict, IsolationLevel::Strict);
        assert_ne!(IsolationLevel::Strict, IsolationLevel::SharedRead);
    }

    // --- FilterExpr Tests ---
    #[test]
    fn test_filter_expr_and_construction() {
        let eq = FilterExpr::Eq {
            field: "lang".to_string(),
            value: serde_json::Value::String("rust".to_string()),
        };
        let expr = FilterExpr::Not(Box::new(eq.clone()));

        let complex = FilterExpr::And(Box::new(eq), Box::new(expr));

        match complex {
            FilterExpr::And(left, right) => {
                assert!(matches!(*left, FilterExpr::Eq { .. }));
                assert!(matches!(*right, FilterExpr::Not(..)));
            }
            _ => panic!("Expected And expression"),
        }
    }

    // --- ContextChunk & Window Tests ---
    #[test]
    fn test_context_window_truncation_flag() {
        let window = ContextWindow {
            chunks: vec![],
            total_tokens: 500,
            truncated: true,
        };
        assert!(window.truncated);
        assert_eq!(window.total_tokens, 500);
    }

    // --- ScoredEntry Tests ---
    #[test]
    fn test_scored_entry_metadata() {
        let entry = ScoredEntry {
            id: "doc-x".to_string(),
            final_score: 0.99,
            metadata: Some(serde_json::json!({"version": 2})),
        };
        assert_eq!(entry.final_score, 0.99);
        assert_eq!(
            entry.metadata.expect("metadata should be present")["version"],
            2
        );
    }
}
