//! Domain types for MemFuse.

// FILE-CONTEXT
// STAND: 2026-08-30T21:51:46Z (SESSION: a43b7682)
// ZWECK: Kanonische Domain-Typen (DocId, EntityId, TxId, Embedding, DistanceMetric, Edge, Entity).
// INVARIANTEN: TxId Base Ranges trennen System- (>= INTERNAL_BASE) von Collection-TxIds. TxId NIEMALS aus SystemTime erzeugen.
// HOTSPOTS: 80-600
// NICHT-OFFENSICHTLICH: DocId::from_key nutzt BLAKE3 8-Byte Präfix für deterministisches Slicing.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md (ADR-016, ADR-025, ADR-041)

//! # Architektur
//! Enthält die zentralen Domänen-Modelle wie `DocId`, `TxId` und `WorkflowState`.
//! Diese Typen sind die "Lingua Franca" zwischen allen Crates.
//!
//! # Invarianten
//! - `DocId` und `TxId` sind Wrapper um primitive Typen mit deterministischer Hash-Generierung.

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

// INVARIANT: Bit 63 der SeqNo markiert Tombstones.
/// Bit mask for identifying tombstones in sequence numbers.
pub const TOMBSTONE_BIT: u64 = 1 << 63;

/// Reserved metadata key for sequence-based document TTL expiration.
pub const EXPIRY_METADATA_KEY: &str = "__expires_at_seq";

/// Maximum number of search results that any hybrid/vector/text search may return.
///
/// Callers in memfuse-mcp and memfuse-db both enforce this limit before forwarding k to HNSW/BM25.
/// Duplicating the literal 1000 anywhere else in the workspace is prohibited — always import this constant.
///
/// This cap is applied at the orchestration layer (memfuse-db) before forwarding `k`
/// to HNSW and BM25 sub-searches. All upstream layers (memfuse-mcp, memfuse-tauri,
/// memfuse-py) MUST reference this constant — never duplicate the literal `1000`.
///
/// # DECISION-REF
/// AGT-DB-003 — Boundary defence at Layer 2 against unbounded `k` from untrusted JSON-RPC.
pub const MAX_SEARCH_K: usize = 1_000;

/// Internal tenant identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct TenantId(pub u64);

impl TenantId {
    /// Invalid tenant identifier sentinel value (`0`).
    pub const INVALID: Self = Self(0);
    /// SYSTEM tenant identifier (`0`).
    pub const SYSTEM: Self = Self(0);
    /// Default tenant identifier (`0`).
    pub const DEFAULT: Self = Self(0);

    /// Creates a new `TenantId` wrapping the provided `u64` identifier.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Creates a new `TenantId`, ensuring `id != 0`.
    pub fn try_new(id: u64) -> Result<Self> {
        if id == 0 {
            Err(MemFuseError::InvalidInput("TenantId cannot be 0".to_string()))
        } else {
            Ok(Self(id))
        }
    }

    /// Returns the inner raw `u64` identifier.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Returns `true` if this tenant ID is `SYSTEM` (0).
    #[inline]
    pub fn is_system(self) -> bool {
        self.0 == 0
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<u64> for TenantId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TenantId({})", self.0)
    }
}

/// Internal collection identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct CollectionId(pub u64);

impl CollectionId {
    /// Invalid collection identifier sentinel value (`0`).
    pub const INVALID: Self = Self(0);

    /// Creates a new `CollectionId` wrapping the provided `u64` identifier.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Creates a new `CollectionId`, ensuring `id != 0`.
    pub fn try_new(id: u64) -> Result<Self> {
        if id == 0 {
            Err(MemFuseError::InvalidInput("CollectionId cannot be 0".to_string()))
        } else {
            Ok(Self(id))
        }
    }

    /// Returns the inner raw `u64` identifier.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl From<u64> for CollectionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for CollectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CollectionId({})", self.0)
    }
}

/// Internal document identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct DocId(pub u64);

impl DocId {
    /// Maximum possible `DocId` value (`u64::MAX`).
    pub const MAX: Self = Self(u64::MAX);
    /// Minimum possible `DocId` value (`0`).
    ///
    /// Note: `DocId(0)` is conventionally treated as a sentinel/null value
    /// by the MCP layer (`memfuse-mcp/src/lib.rs` line 214 uses it as a
    /// fallback). Callers MUST propagate `from_key()` errors rather than
    /// silently producing `DocId(0)`.
    pub const MIN: Self = Self(0);

    /// Creates a new `DocId` wrapping the provided `u64` identifier.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner raw `u64` identifier.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Derives a `DocId` from a string key using the first 8 bytes (64 bits) of its BLAKE3 hash.
    ///
    /// # Kollisionssicherheit & Entwurfsentscheidung (ADR-016)
    /// Durch die Trunkierung auf 64 Bit besteht bei sehr großen Dokumentenmengen (ca. 2^32 bzw. ~4 Milliarden Keys)
    /// eine theoretische Kollisionswahrscheinlichkeit von ~50% (Geburtstagsparadoxon).
    ///
    /// Um stille Datenkorruption zu verhindern, führt die Orchestrierungsschicht (`memfuse-db::Collection`)
    /// bei Einfügeoperationen (`insert_op` / `update_op`) eine Kollisionsprüfung durch (Reverse-Lookup des `doc_key`).
    /// Sollte eine Kollision mit einem abweichenden Originalschlüssel erkannt werden, wird die Operation
    /// mit `MemFuseError::Internal` abgelehnt (Fail-Safe statt Fail-Silent).
    ///
    /// See **ADR-016** in `DECISIONS.md`.
    pub fn from_key(key: &str) -> Result<Self> {
        if key.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Key cannot be empty".to_string(),
            ));
        }
        Ok(Self(hash_key_u64(key)))
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
    /// Creates a new `EntityId` wrapping the provided `u64` identifier.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner raw `u64` identifier.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Converts the entity identifier into its string byte representation.
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.to_string().into_bytes()
    }

    /// Creates an `EntityId` directly from a `DocId`.
    pub fn from_doc_id(doc_id: DocId) -> Self {
        Self(doc_id.inner())
    }

    /// Derives an `EntityId` from a string key using the first 8 bytes of its BLAKE3 hash.
    ///
    /// # Errors
    /// Returns `MemFuseError::InvalidInput` if `key` is empty, mirroring `DocId::from_key`.
    ///
    /// # Infallible Fallback
    /// If you need the old infallible behaviour (parse-as-u64 or hash), use `EntityId::from(key)` directly.
    /// Prefer this fallible variant for consistency with `DocId` at API boundaries.
    pub fn from_key(key: &str) -> Result<Self> {
        DocId::from_key(key).map(|d| Self(d.inner()))
    }
}

impl From<u64> for EntityId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

#[inline]
fn hash_key_u64(s: &str) -> u64 {
    let hash = blake3::hash(s.as_bytes());
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

impl From<&str> for EntityId {
    /// Infallible conversion: parses as `u64` first, then falls back to BLAKE3 hash.
    /// For consistent error handling at API boundaries, prefer `EntityId::from_key`.
    fn from(s: &str) -> Self {
        if let Ok(val) = s.parse::<u64>() {
            Self(val)
        } else {
            Self(hash_key_u64(s))
        }
    }
}

impl From<String> for EntityId {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

/// Transaction identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct TxId(pub u64);

impl TxId {
    /// Invalid or uninitialized transaction identifier sentinel value (`0`).
    ///
    /// `TxId(0)` is reserved as an uninitialized sentinel or null identifier across the system.
    /// Transaction allocation sequences start at 1 (`Collection::allocate_tx()`), making `0`
    /// an explicit indicator of unassigned or invalid transaction context.
    pub const INVALID: Self = Self(0);

    /// Upper bound of the Collection-sequenced transaction ID range (`10^12`).
    ///
    /// Transaction allocation sequences managed by `Collection::allocate_tx()` operate
    /// in the range `[1, MAX_COLLECTION_SEQUENCE]`.
    pub const MAX_COLLECTION_SEQUENCE: u64 = 1_000_000_000_000;

    /// Lower bound of the internal system transaction ID range.
    ///
    /// Exact numeric value: `u64::MAX - 1_000_000` (`18_446_744_073_708_551_615`).
    ///
    /// Dieser Grenzwert definiert die Trennlinie zwischen Collection-sequenzierten TxIds
    /// (`[1, MAX_COLLECTION_SEQUENCE]`, verwaltet von `Collection::allocate_tx()`) und system-internen TxIds
    /// (`[INTERNAL_BASE, u64::MAX]`, verwaltet von `INTERNAL_BASE + atomic counter`).
    ///
    /// Wall-clock-abgeleitete TxIds (Unix-Nanos `~1.7e18`) fallen in den Zwischenbereich
    /// (`10^12 < ~1.7e18 < INTERNAL_BASE`) und korrumpieren `rollback_to_tx()`-Kausalität,
    /// da bereichsbasierte Transaktions-Rollbacks und Graph-Pruning nicht zwischen
    /// Benutzertransaktionen und internen Snapshot-Grenzen unterscheiden können.
    ///
    /// See also: AGT-GRAPH-001, DECISIONS.md ADR-016.
    pub const INTERNAL_BASE: u64 = u64::MAX - 1_000_000;

    /// Creates a new `TxId` wrapping the provided `u64` transaction identifier.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner raw `u64` transaction identifier.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Returns a new internal/system transaction ID.
    #[inline]
    pub const fn internal() -> Self {
        Self(Self::INTERNAL_BASE)
    }

    /// Safely constructs an internal system TxId from an offset relative to `TxId::INTERNAL_BASE`.
    ///
    /// # Domain Range Separation (ADR-028)
    /// Valid system transaction range is `[INTERNAL_BASE, u64::MAX]`.
    /// Offsets must not cause integer overflow beyond `u64::MAX`, nor can wraparound occur
    /// into the regular collection sequence range `[1, MAX_COLLECTION_SEQUENCE]`.
    ///
    /// # Errors
    /// Returns `MemFuseError::Transaction` if `INTERNAL_BASE + offset` overflows `u64::MAX`.
    pub fn try_from_internal_offset(offset: u64) -> Result<Self> {
        Self::INTERNAL_BASE
            .checked_add(offset)
            .map(Self)
            .ok_or_else(|| {
                MemFuseError::Transaction(format!(
                    "Internal TxId allocation offset {offset} overflows u64::MAX (INTERNAL_BASE={})",
                    Self::INTERNAL_BASE
                ))
            })
    }

    /// Checks if the transaction ID originates from a valid range per AGT-GRAPH-001.
    ///
    /// Returns `true` if `self.0 <= MAX_COLLECTION_SEQUENCE` (Collection sequence range)
    /// OR `self.0 >= INTERNAL_BASE` (Internal system range).
    /// Returns `false` for wall-clock derived TxIds (~1.7×10^18) in the unmanaged gap.
    #[inline]
    pub fn is_valid_origin(&self) -> bool {
        self.0 <= Self::MAX_COLLECTION_SEQUENCE || self.0 >= Self::INTERNAL_BASE
    }
}

const _: () = assert!(
    TxId::INTERNAL_BASE > TxId::MAX_COLLECTION_SEQUENCE,
    "INTERNAL_BASE must be above the collection-sequence range"
);
const _: () = assert!(TxId::INTERNAL_BASE < u64::MAX);

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TxId({})", self.0)
    }
}

/// Distance metric for vector comparison.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[non_exhaustive]
pub enum DistanceMetric {
    /// Cosine distance (`1.0 - cos(angle)`).
    #[default]
    Cosine,
    /// Euclidean (L2) distance.
    Euclidean,
    /// Dot product distance (negated inner product for f32).
    DotProduct,
}

impl DistanceMetric {
    /// Computes the distance between two f32 vectors using this metric.
    pub fn compute(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(MemFuseError::invalid_input("Vector dimensions must match"));
        }

        for val in a.iter().chain(b.iter()) {
            if !val.is_finite() {
                return Err(MemFuseError::InvalidInput(
                    "Input vector contains non-finite values (NaN or Inf)".into(),
                ));
            }
        }

        let dist = match self {
            Self::Cosine => {
                let mut dot = 0.0;
                let mut norm_a = 0.0;
                let mut norm_b = 0.0;
                for (x, y) in a.iter().zip(b.iter()) {
                    dot += x * y;
                    norm_a += x * x;
                    norm_b += y * y;
                }
                if norm_a == 0.0 || norm_b == 0.0 {
                    1.0
                } else {
                    // Floating-point rounding errors on nearly parallel or identical vectors can cause dot / (norm_a.sqrt() * norm_b.sqrt())
                    // to slightly exceed 1.0, producing negative distances.
                    // Cosine distance is mathematically restricted to [0.0, 2.0] as cosine similarity ∈ [-1.0, 1.0].
                    let dist = 1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()));
                    dist.clamp(0.0, 2.0)
                }
            }
            Self::Euclidean => {
                let mut sum = 0.0;
                for (x, y) in a.iter().zip(b.iter()) {
                    let diff = x - y;
                    sum += diff * diff;
                }
                sum.max(0.0).sqrt()
            }
            Self::DotProduct => {
                let mut dot = 0.0;
                for (x, y) in a.iter().zip(b.iter()) {
                    dot += x * y;
                }
                -dot // Negative dot product for distance
            }
        };

        if !dist.is_finite() {
            return Err(MemFuseError::InvalidInput(
                "Distance computation resulted in a non-finite value (NaN or Inf)".into(),
            ));
        }

        Ok(dist)
    }

    /// Computes the distance between two u8 vectors using this metric.
    ///
    /// Returns a `u32` distance value where smaller values indicate higher similarity / smaller distance
    /// for ALL metric variants.
    ///
    /// # Metric Semantics & Precision
    /// - **Cosine**: Scaled by `1_000_000` for fixed-point ranking (`[0, 2_000_000]`).
    /// - **Euclidean**: Computes `sqrt(Σ diff²)` in `f64` precision before rounding to nearest integer (`round()`)
    ///   and casting/saturating to `u32`. This matches `compute()`'s f32 square-root behavior.
    /// - **DotProduct**: Inverts the sign convention using `u32::MAX - dot` (saturated at `u32::MAX`), matching
    ///   `compute()`'s `-dot` convention so that smaller return values consistently mean higher similarity.
    ///
    /// # Cross-Crate Caller Dependency Notice
    /// Callers in `memfuse-index` (such as `hnsw.rs` or `quantize.rs`) expecting raw squared Euclidean distance or
    /// non-inverted raw dot products must account for this unified "smaller = closer" distance metric semantics.
    // DECISION-REF: AGT-CORE-001 — Overflow-Schutz in compute_u8() bereits implementiert.
    // KONTEXT: Alle drei Zweige akkumulieren in u64 (Euclidean: diff²-Summe, DotProduct: Produkt-Summe)
    //          bzw. f64 (Cosine) und sättigen per .min(u32::MAX as u64). Kein Overflow möglich.
    //          Regressionstest: test_distance_metrics_u8_overflow (100_000 Elemente à 255).
    // ID: AGT-CORE-001
    pub fn compute_u8(&self, a: &[u8], b: &[u8]) -> Result<u32> {
        if a.len() != b.len() {
            return Err(MemFuseError::invalid_input("Vector dimensions must match"));
        }

        match self {
            Self::Cosine => {
                // FIND-COR-002: Correct cosine distance using f64 arithmetic
                // cos_dist = 1.0 - (dot(a,b) / (||a|| * ||b||))
                let mut dot = 0f64;
                let mut norm_a = 0f64;
                let mut norm_b = 0f64;
                for (&x, &y) in a.iter().zip(b.iter()) {
                    let xf = x as f64;
                    let yf = y as f64;
                    dot += xf * yf;
                    norm_a += xf * xf;
                    norm_b += yf * yf;
                }
                let denom = norm_a.sqrt() * norm_b.sqrt();
                let dist = if denom == 0.0 { 1.0 } else { 1.0 - dot / denom };
                // Scale to u32 fixed-point (×1_000_000) for ranking
                Ok((dist.clamp(0.0, 2.0) * 1_000_000.0) as u32)
            }
            Self::Euclidean => {
                let mut sum = 0u64;
                for (&x, &y) in a.iter().zip(b.iter()) {
                    let diff = (x as i64) - (y as i64);
                    sum += (diff * diff) as u64;
                }
                let dist_f64 = (sum as f64).sqrt();
                let dist_rounded = dist_f64.round();
                Ok(dist_rounded.min(u32::MAX as f64) as u32)
            }
            Self::DotProduct => {
                // Inverted dot product (u32::MAX - dot) so smaller distance = higher similarity.
                let mut dot = 0u64;
                for (&x, &y) in a.iter().zip(b.iter()) {
                    dot += (x as u64) * (y as u64);
                }
                let dot_clamped = dot.min(u32::MAX as u64) as u32;
                Ok(u32::MAX - dot_clamped)
            }
        }
    }
}

impl crate::traits::DistanceCalculator for DistanceMetric {
    fn compute_f32(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        self.compute(a, b)
    }

    fn compute_u8(&self, a: &[u8], b: &[u8]) -> Result<u32> {
        self.compute_u8(a, b)
    }
}

/// Vector embedding representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// Raw vector data elements.
    pub data: Vec<f32>,
}

impl Embedding {
    /// Creates a new vector embedding from `f32` slice or vector.
    pub fn new(data: Vec<f32>) -> Self {
        Self { data }
    }

    /// Returns the dimension (length) of the embedding vector.
    #[inline]
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    /// Returns the vector data as an `f32` slice.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Computes the Euclidean L2 norm of the vector.
    pub fn l2_norm(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Returns an L2-unit-normalized clone of this embedding.
    ///
    /// Guarded against zero and subnormal/near-zero vector norms (`norm < 1e-12`).
    /// Vectors with `norm < 1e-12` are returned unchanged to avoid division by subnormals
    /// resulting in `Inf` or `NaN`.
    pub fn normalize(&self) -> Self {
        let norm = self.l2_norm();
        // Threshold 1e-12 is chosen to safely catch subnormal or near-zero floats
        // across high-dimensional embeddings while preventing Inf/NaN after division.
        if norm < 1e-12 {
            return self.clone();
        }
        Self::new(self.data.iter().map(|x| x / norm).collect())
    }
}

/// A scored search result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScoredDocument {
    /// Identifier of the scored document.
    pub doc_id: DocId,
    /// Relevance or similarity score.
    pub score: f32,
}

impl ScoredDocument {
    /// Creates a new `ScoredDocument` with document ID and score.
    pub fn new(doc_id: DocId, score: f32) -> Self {
        Self { doc_id, score }
    }
}

/// Relationship types between Zettelkasten memory chunks (A-MEM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkRelation {
    /// Target memory provides more detail or context.
    Elaborates,
    /// Target memory contradicts this memory.
    Contradicts,
    /// Target memory supersedes this memory, replacing its context.
    Supersedes,
    /// General reference without specific semantics.
    References,
}

/// A directional link to another memory chunk in the Zettelkasten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryLink {
    /// The target document ID.
    pub target: DocId,
    /// The type of relationship.
    pub relation: LinkRelation,
    /// The transaction ID when this link was created.
    pub created_at_tx: TxId,
}

/// Represents a canonical node in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Unique entity ID.
    pub id: EntityId,
    /// Canonical human-readable name.
    pub name: String,
    /// Categorical entity type.
    pub entity_type: String,
    /// Flexible key-value attribute metadata map.
    #[serde(default)]
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
}

impl Entity {
    /// Creates a new `Entity` with id, name, and type.
    pub fn new(id: EntityId, name: impl Into<String>, entity_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            entity_type: entity_type.into(),
            attributes: Default::default(),
        }
    }

    /// Creates a new `Entity`, validating that `name` and `entity_type` are non-empty.
    ///
    /// # Errors
    /// Returns `MemFuseError::InvalidInput` if `name` or `entity_type` is empty or whitespace-only.
    pub fn try_new(
        id: EntityId,
        name: impl Into<String>,
        entity_type: impl Into<String>,
    ) -> Result<Self> {
        let name_str = name.into();
        let type_str = entity_type.into();
        if name_str.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Entity name cannot be empty".to_string(),
            ));
        }
        if type_str.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Entity type cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            id,
            name: name_str,
            entity_type: type_str,
            attributes: Default::default(),
        })
    }
}

/// Graph directed edge representation with explicit bitemporal axis separation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source entity identifier.
    pub from: EntityId,
    /// Destination entity identifier.
    pub to: EntityId,
    /// Relationship label.
    pub label: String,
    /// Numeric relationship weight (default 1.0).
    pub weight: f32,
    /// Start of transaction validity (system time / MVCC); None = valid from beginning of transaction history.
    #[serde(default, alias = "valid_from")]
    pub tx_valid_from: Option<TxId>,
    /// End of transaction validity (system time / MVCC); None = currently valid transaction state.
    #[serde(default, alias = "valid_to")]
    pub tx_valid_to: Option<TxId>,
    /// Start of business validity (business time in Unix ms); None = valid from beginning of business time.
    #[serde(default)]
    pub business_valid_from: Option<i64>,
    /// End of business validity (business time in Unix ms); None = currently valid business state.
    #[serde(default)]
    pub business_valid_to: Option<i64>,
}

impl Edge {
    /// Creates a new `Edge` between source and target entities with a label.
    pub fn new(from: EntityId, to: EntityId, label: impl Into<String>) -> Self {
        Self {
            from,
            to,
            label: label.into(),
            weight: 1.0,
            tx_valid_from: None,
            tx_valid_to: None,
            business_valid_from: None,
            business_valid_to: None,
        }
    }

    /// Creates a new `Edge`, validating non-empty label and finite non-negative weight.
    ///
    /// # Errors
    /// Returns `MemFuseError::InvalidInput` if `label` is empty or `weight` is NaN/infinite/negative.
    pub fn try_new(
        from: EntityId,
        to: EntityId,
        label: impl Into<String>,
        weight: f32,
    ) -> Result<Self> {
        let label_str = label.into();
        if label_str.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Edge label cannot be empty".to_string(),
            ));
        }
        if !weight.is_finite() || weight < 0.0 {
            return Err(MemFuseError::InvalidInput(
                "Edge weight must be finite and non-negative".to_string(),
            ));
        }
        Ok(Self {
            from,
            to,
            label: label_str,
            weight,
            tx_valid_from: None,
            tx_valid_to: None,
            business_valid_from: None,
            business_valid_to: None,
        })
    }

    /// Sets a custom weight on the edge.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Sets transaction validity window (system time / MVCC) on the edge.
    pub fn with_tx_validity(mut self, from: Option<TxId>, to: Option<TxId>) -> Self {
        self.tx_valid_from = from;
        self.tx_valid_to = to;
        self
    }

    /// Sets business validity window (business time in Unix ms) on the edge.
    pub fn with_business_validity(mut self, from: Option<i64>, to: Option<i64>) -> Self {
        self.business_valid_from = from;
        self.business_valid_to = to;
        self
    }

    /// Sets transaction validity window on the edge (alias for `with_tx_validity` for backward compatibility).
    pub fn with_validity(self, from: Option<TxId>, to: Option<TxId>) -> Self {
        self.with_tx_validity(from, to)
    }
}

/// Klassifiziert den kognitiven Gedächtnistyp einer gespeicherten Einheit.
///
/// # Persistence
/// Wird als Teil der Dokument-Metadaten serialisiert (JSON-Feld "memory_type").
/// Rückwärtskompatibel: Fehlendes Feld wird als MemoryType::Semantic deserialisiert
/// (bisherige Dokumente ohne Klassifikation = faktisches Wissen).
///
/// # Non-Exhaustive
/// `#[non_exhaustive]` erlaubt in zukünftigen Releases neue Varianten ohne
/// Breaking Change bei downstream match-Ausdrücken (KEIN wildcard-arm zwingend
/// für Library-Consumer bis zur nächsten Major-Version).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MemoryType {
    /// Episodisches Gedächtnis: Erlebnisse, Unterhaltungen, zeitlich verankerte Ereignisse.
    /// Retrieval: Zeitliche Nähe und Relevanz zur aktuellen Session.
    /// Decay: Hohe Recency-Decay-Rate (Informationen veralten schnell).
    #[serde(alias = "Episodic")]
    Episodic,

    /// Semantisches Gedächtnis: Fakten, Konzepte, dauerhaftes Wissen.
    /// Retrieval: Inhaltliche Ähnlichkeit (Vektor + BM25).
    /// Decay: Keine automatische Decay (Fakten bleiben gültig bis Widerspruch).
    #[default]
    #[serde(alias = "Semantic")]
    Semantic,

    /// Prozedurales Gedächtnis: Abläufe, Tool-Nutzungsmuster, Workflows.
    /// Retrieval: Task-Matching (zukünftig: Instruktionskodierung).
    /// Decay: Aktivierungsbasiert (wird durch Nutzung gestärkt).
    #[serde(alias = "Procedural")]
    Procedural,

    /// Operatives Arbeitsgedächtnis: Kurzzeit-Kontext der aktuellen Session.
    /// Lebensdauer: Session-scoped, automatisch bei Session-Ende ablaufend.
    /// Decay: Sehr hohe Decay-Rate (Session-TTL, z. B. 30 Minuten Inaktivität).
    #[serde(alias = "Working")]
    Working,
}

impl MemoryType {
    /// Gibt den Standard-Decay-Typ für diesen Gedächtnistyp zurück.
    pub fn default_decay(&self) -> crate::types::importance::DecayFunction {
        match self {
            MemoryType::Episodic => crate::types::importance::DecayFunction::Exponential {
                half_life_tx: 10_000, // ca. 10.000 Transaktionen ≈ moderate Abnahme
            },
            MemoryType::Semantic => crate::types::importance::DecayFunction::None,
            MemoryType::Procedural => crate::types::importance::DecayFunction::StepFloor {
                access_count_floor: 50, // Verstärkt durch Nutzung
            },
            MemoryType::Working => crate::types::importance::DecayFunction::Exponential {
                half_life_tx: 500, // Sehr schnelle Abnahme
            },
        }
    }

    /// Gibt die empfohlene TTL (in Transaktionen) für Session-scoped Working Memory zurück.
    /// None bedeutet kein automatisches Ablaufen.
    pub fn default_ttl_tx(&self) -> Option<u64> {
        match self {
            MemoryType::Working => Some(50_000), // ~50.000 TX ≈ 30 Minuten bei normalem Tempo
            _ => None,
        }
    }

    /// Gibt den kanonischen Metadaten-Key zurück (für JSON-Serialisierung).
    pub fn as_metadata_key(&self) -> &'static str {
        match self {
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
            MemoryType::Procedural => "procedural",
            MemoryType::Working => "working",
        }
    }
}

/// Configuration parameters for Personalized PageRank (PPR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PprConfig {
    /// Damping factor (probability of continuing random walk vs restarting). Default: 0.85.
    pub damping_factor: f32,
    /// Maximum power-iteration steps before terminating. Default: 100.
    pub max_iterations: u32,
    /// L1 norm threshold for early termination convergence check. Default: 1e-6.
    pub convergence_epsilon: f32,
    /// Gibt eine nicht-konvergierte Warnung (tracing::warn!) aus, wenn
    /// max_iterations erreicht wird, bevor convergence_epsilon
    /// unterschritten wurde. Kein Fehler — die Berechnung liefert das
    /// beste bisher erreichte Ergebnis zurück. Default: true.
    #[serde(default = "default_warn_on_non_convergence")]
    pub warn_on_non_convergence: bool,
}

fn default_warn_on_non_convergence() -> bool {
    true
}

impl Default for PprConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            convergence_epsilon: 1e-6,
            warn_on_non_convergence: true,
        }
    }
}

/// Konfigurations-Fingerabdruck für P8-Kalibrierungs-Integrität.
///
/// INVARIANTE INV-P8-1: Jede Änderung an einem der Felder MUSS
/// `IsotonicCalibrator::invalidate_on_config_change()` auslösen.
/// Kein Warmup-Fenster darf nach Fingerprint-Wechsel übersprungen werden.
///
/// BEGRÜNDUNG: arXiv:2608.01460 — Coverage-Kollaps unter Konfigurations-Drift.
/// Q4 und Q8 bei identischem model_id erzeugen unterschiedliche Score-Verteilungen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigFingerprint {
    /// LLM-Modell-ID (z.B. "llama-3.2-3b-instruct")
    pub model_id: String,
    /// Quantisierungsgrad als String: "Q4_K_M", "Q8_0", "F16", "BF16".
    /// EXPLIZIT Teil des Fingerprints — Q4 ≠ Q8 bei gleichem model_id.
    pub quantization: String,
    /// SHA256 des Prompt-Templates (nicht der Inhalt — nur der Hash).
    /// Verhindert stille Kalibrierungs-Invalidierung bei Template-Drift.
    pub prompt_template_hash: [u8; 32],
    /// Temperatur als Bits für bit-exakten Vergleich (kein float-Gleichheitstest).
    /// `temperature_bits = temperature.to_bits()`
    pub temperature_bits: u32,
}

impl ConfigFingerprint {
    /// Erstellt einen neuen `ConfigFingerprint`.
    pub fn new(
        model_id: impl Into<String>,
        quantization: impl Into<String>,
        prompt_template: &str,
        temperature: f32,
    ) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(prompt_template.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        Self {
            model_id: model_id.into(),
            quantization: quantization.into(),
            prompt_template_hash: hash,
            temperature_bits: temperature.to_bits(),
        }
    }

    /// Extrahiert Temperatur als f32 (verlustfrei da via to_bits gespeichert).
    #[inline]
    pub fn temperature(&self) -> f32 {
        f32::from_bits(self.temperature_bits)
    }
}

impl Default for ConfigFingerprint {
    fn default() -> Self {
        Self::new("default", "F16", "", 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq};

    #[test]
    fn test_tenant_id_defaults_and_constants() {
        let default_tenant = TenantId::default();
        assert_eq!(default_tenant, TenantId::DEFAULT);
        assert_eq!(default_tenant.inner(), 0);
        assert_eq!(TenantId::new(42).inner(), 42);
        assert_eq!(TenantId::from(100u64), TenantId::new(100));
        assert_eq!(format!("{default_tenant}"), "TenantId(0)");
    }

    #[test]
    fn test_tenant_id_collections_hashmap_btreemap() {
        use std::collections::{BTreeMap, HashMap};

        let t0 = TenantId::DEFAULT;
        let t1 = TenantId::new(1);
        let t2 = TenantId::new(2);

        // HashMap key test
        let mut map = HashMap::new();
        map.insert(t0, "default_tenant");
        map.insert(t1, "tenant_one");
        assert_eq!(map.get(&TenantId::default()), Some(&"default_tenant"));
        assert_eq!(map.get(&t1), Some(&"tenant_one"));
        assert_eq!(map.get(&t2), None);

        // BTreeMap key test (testing Ord / PartialOrd)
        let mut bmap = BTreeMap::new();
        bmap.insert(t2, "tenant_two");
        bmap.insert(t0, "tenant_zero");
        bmap.insert(t1, "tenant_one");

        let keys: Vec<TenantId> = bmap.keys().copied().collect();
        assert_eq!(keys, vec![t0, t1, t2]);
    }

    #[test]
    fn test_tenant_id_serde_roundtrip() {
        let tenant = TenantId::new(987654321);
        let serialized = serde_json::to_string(&tenant).expect("TenantId serialization failed");
        assert_eq!(serialized, "987654321");

        let deserialized: TenantId = serde_json::from_str(&serialized).expect("TenantId deserialization failed");
        assert_eq!(tenant, deserialized);
    }

    #[test]
    fn test_expiry_metadata_key_constant() {
        assert_eq!(EXPIRY_METADATA_KEY, "__expires_at_seq");
    }

    #[test]
    fn test_doc_id_from_key_no_panic() {
        assert!(DocId::from_key("").is_err());
        let keys = vec![
            "a",
            "short",
            "very_long_key_that_exceeds_blake3_block_size_maybe_not_really_but_long",
        ];
        for key in keys {
            let res = DocId::from_key(key);
            assert!(res.is_ok(), "DocId::from_key failed for key: {}", key);
        }
    }

    #[test]
    fn test_doc_id_determinism() {
        let key = "consistent_key";
        let id1 = DocId::from_key(key).unwrap(); // unwrap
        let id2 = DocId::from_key(key).unwrap(); // unwrap
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_core_distance_dimension_mismatch() {
        let a = [1.0f32; 128];
        let b = [1.0f32; 256];
        let res = DistanceMetric::Cosine.compute(&a, &b);
        assert!(res.is_err());
    }

    #[test]
    fn test_tenant_and_collection_id() {
        let tenant = TenantId::try_new(1).unwrap();
        assert_eq!(tenant.inner(), 1);
        assert_eq!(tenant.to_string(), "TenantId(1)");
        assert!(TenantId::try_new(0).is_err());

        let collection = CollectionId::try_new(42).unwrap();
        assert_eq!(collection.inner(), 42);
        assert_eq!(collection.to_string(), "CollectionId(42)");
        assert!(CollectionId::try_new(0).is_err());
    }

    #[test]
    fn test_serialization_roundtrips() {
        // TenantId & CollectionId
        let t = TenantId::new(10);
        let ser = serde_json::to_string(&t).unwrap();
        let deser: TenantId = serde_json::from_str(&ser).unwrap();
        assert_eq!(t, deser);

        let c = CollectionId::new(20);
        let ser = serde_json::to_string(&c).unwrap();
        let deser: CollectionId = serde_json::from_str(&ser).unwrap();
        assert_eq!(c, deser);

        // DocId
        let doc = DocId::new(42);
        let ser = serde_json::to_string(&doc).unwrap(); // unwrap
        let deser: DocId = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(doc, deser);

        // TxId
        let tx = TxId::new(TxId::INTERNAL_BASE + 5);
        let ser = serde_json::to_string(&tx).unwrap(); // unwrap
        let deser: TxId = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(tx, deser);

        // EntityId
        let ent = EntityId::new(999);
        let ser = serde_json::to_string(&ent).unwrap(); // unwrap
        let deser: EntityId = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(ent, deser);
    }

    #[test]
    fn test_embedding_norm_and_normalize() {
        let emb = Embedding::new(vec![3.0, 4.0]);
        assert_eq!(emb.dim(), 2);
        assert_eq!(emb.l2_norm(), 5.0);

        let normalized = emb.normalize();
        assert_eq!(normalized.l2_norm(), 1.0);
        assert_eq!(normalized.as_slice(), &[0.6, 0.8]);

        // Zero norm handling
        let zero_emb = Embedding::new(vec![0.0, 0.0]);
        let normalized_zero = zero_emb.normalize();
        assert_eq!(normalized_zero.l2_norm(), 0.0);
    }

    #[test]
    fn test_distance_metrics_f32() {
        let a = [1.0, 0.0];
        let b = [0.0, 1.0];
        // Cosine: 1 - (0 / 1) = 1.0
        assert_eq!(DistanceMetric::Cosine.compute(&a, &b).unwrap(), 1.0); // unwrap
                                                                          // Euclidean: sqrt(1^2 + 1^2) = sqrt(2)
        assert_eq!(
            DistanceMetric::Euclidean.compute(&a, &b).unwrap(), // unwrap
            2.0f32.sqrt()
        );
        // DotProduct: -(0) = 0.0
        assert_eq!(DistanceMetric::DotProduct.compute(&a, &b).unwrap(), 0.0); // unwrap
    }

    #[test]
    fn test_distance_metrics_u8() {
        let a = [10, 20];
        let b = [20, 30];
        // Euclidean: sqrt((10-20)^2 + (20-30)^2) = sqrt(200) ≈ 14.142 -> rounded 14
        assert_eq!(DistanceMetric::Euclidean.compute_u8(&a, &b).unwrap(), 14); // unwrap
                                                                               // DotProduct: dot = 10*20 + 20*30 = 800 -> inverted: u32::MAX - 800
        assert_eq!(
            DistanceMetric::DotProduct.compute_u8(&a, &b).unwrap(), // unwrap
            u32::MAX - 800
        ); // unwrap
           // Cosine: orthogonal vectors → distance 1.0 → 1_000_000
        let orth_a: [u8; 2] = [255, 0];
        let orth_b: [u8; 2] = [0, 255];
        assert_eq!(
            DistanceMetric::Cosine.compute_u8(&orth_a, &orth_b).unwrap(), // unwrap
            1_000_000
        );
        // Cosine: identical vectors → distance 0.0 → 0
        assert_eq!(DistanceMetric::Cosine.compute_u8(&a, &a).unwrap(), 0); // unwrap
    }

    /// Regressionstest für AGT-CORE-001: Beweist, dass compute_u8() bei Vektoren der Länge
    /// 100_000 mit allen Elementen = 255 in keinem der drei Zweige panikt oder überläuft.
    ///
    /// Worst-case-Analyse:
    /// - Euclidean: diff=0 (identische Vektoren) → sum=0. Maximaler Fall: diff=255, sum = 255²×100_000
    ///   = 6_502_500_000, sqrt = 80_638.1.
    /// - DotProduct: 255×255×100_000 = 6_502_500_000 > u32::MAX → dot saturiert bei u32::MAX -> u32::MAX - u32::MAX = 0.
    /// - Cosine: f64-Akkumulation, kein Ganzzahl-Overflow möglich.
    #[test]
    fn test_distance_metrics_u8_overflow() {
        // Identische Vektoren (diff=0): Euclidean=0, DotProduct=0 (höchste Ähnlichkeit), Cosine=0
        let max_vec: Vec<u8> = vec![255u8; 100_000];
        let same_vec: Vec<u8> = vec![255u8; 100_000];

        // Euclidean: identische Vektoren → Distanz 0
        let eucl_same = DistanceMetric::Euclidean
            .compute_u8(&max_vec, &same_vec)
            .unwrap(); // unwrap
        assert_eq!(
            eucl_same, 0,
            "Euclidean distance of identical vectors must be 0"
        );

        // DotProduct: dot = saturiert u32::MAX → u32::MAX - u32::MAX = 0
        let dot_same = DistanceMetric::DotProduct
            .compute_u8(&max_vec, &same_vec)
            .unwrap(); // unwrap
        assert_eq!(
            dot_same, 0,
            "DotProduct inverted distance must be 0 for identical high-value vectors"
        );

        // Cosine: identische Vektoren → Distanz 0 (cos_dist = 1 - 1 = 0)
        let cos_same = DistanceMetric::Cosine
            .compute_u8(&max_vec, &same_vec)
            .unwrap(); // unwrap
        assert_eq!(
            cos_same, 0,
            "Cosine distance of identical vectors must be 0"
        );

        // Worst-case Euclidean: maximale Differenz (255 vs. 0) → sum = 255²×100_000 = 6_502_500_000, sqrt ≈ 80_638.1
        let zero_vec: Vec<u8> = vec![0u8; 100_000];
        let eucl_max = DistanceMetric::Euclidean
            .compute_u8(&max_vec, &zero_vec)
            .unwrap(); // unwrap
        assert_eq!(
            eucl_max, 80638,
            "Euclidean must produce rounded sqrt of sum of squared diffs"
        );

        // Cosine: senkrechte Vektoren (255..255 vs. 0..0) → Sonderfall: Nullvektor → Distanz 1.0
        let cos_zero = DistanceMetric::Cosine
            .compute_u8(&max_vec, &zero_vec)
            .unwrap(); // unwrap
        assert_eq!(
            cos_zero, 1_000_000,
            "Cosine distance against zero vector must be 1.0 (scaled: 1_000_000)"
        );
    }

    #[test]
    fn test_u8_and_f32_distance_metrics_ranking_and_value_parity() {
        let metrics = [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ];

        let query = [100u8, 200, 50, 150];
        let close_vec = [110u8, 190, 60, 140]; // very close vector
        let far_vec = [10u8, 20, 250, 5]; // distant vector

        let q_f32: Vec<f32> = query.iter().map(|&x| x as f32).collect();
        let c_f32: Vec<f32> = close_vec.iter().map(|&x| x as f32).collect();
        let f_f32: Vec<f32> = far_vec.iter().map(|&x| x as f32).collect();

        for metric in metrics {
            let u8_close = metric.compute_u8(&query, &close_vec).unwrap(); // unwrap
            let u8_far = metric.compute_u8(&query, &far_vec).unwrap(); // unwrap

            let f32_close = metric.compute(&q_f32, &c_f32).unwrap(); // unwrap
            let f32_far = metric.compute(&q_f32, &f_f32).unwrap(); // unwrap

            // Ranking order parity: smaller distance MUST mean closer for BOTH f32 and u8
            assert!(
                u8_close < u8_far,
                "u8 ranking mismatch for {metric:?}: close={u8_close}, far={u8_far}"
            );
            assert!(
                f32_close < f32_far,
                "f32 ranking mismatch for {metric:?}: close={f32_close}, far={f32_far}"
            );

            // Value scale sanity check
            match metric {
                DistanceMetric::Euclidean => {
                    // f32 euclidean sqrt diff vs u8 rounded sqrt diff
                    let diff = (u8_close as f32 - f32_close).abs();
                    assert!(
                        diff < 1.0,
                        "Euclidean f32 ({f32_close}) vs u8 ({u8_close}) deviation too large: diff={diff}"
                    );
                }
                DistanceMetric::Cosine => {
                    // u8 cosine is scaled by 1_000_000
                    let expected_scaled = (f32_close as f64 * 1_000_000.0).round() as u32;
                    let diff = (u8_close as i64 - expected_scaled as i64).abs();
                    assert!(
                        diff <= 1,
                        "Cosine fixed point scaling mismatch for {metric:?}: u8={u8_close}, scaled_f32={expected_scaled}"
                    );
                }
                DistanceMetric::DotProduct => {
                    // f32 dot product is -dot; u8 dot product is u32::MAX - dot
                    let dot_f32 = -f32_close; // raw dot
                    let dot_u8 = u32::MAX - u8_close; // raw dot
                    assert_eq!(
                        dot_f32 as u32, dot_u8,
                        "DotProduct raw dot values should match between f32 and u8"
                    );
                }
            }
        }
    }

    #[test]
    fn test_doc_id_multibyte_unicode_keys() {
        let unicode_keys = vec![
            "🦀_crab_key",
            "äöü_german_key",
            "日本語_japanese_key",
            "🚀✨🔥",
        ];
        for key in unicode_keys {
            let doc_id = DocId::from_key(key).expect("multibyte unicode key should derive doc_id"); // expect #[cfg(test)]
            assert!(doc_id.inner() > 0);
            let entity_id =
                EntityId::from_key(key).expect("multibyte unicode key should derive entity_id"); // expect #[cfg(test)]
            assert_eq!(entity_id.inner(), doc_id.inner());
        }
    }

    #[test]
    fn test_entity_id_methods() {
        let entity_id = EntityId::new(12345);
        assert_eq!(entity_id.inner(), 12345);
        assert_eq!(entity_id.as_bytes(), b"12345");

        let doc_id = DocId::new(998877);
        let derived_entity = EntityId::from_doc_id(doc_id);
        assert_eq!(derived_entity.inner(), 998877);

        // String / &str conversions
        let parsed_num: EntityId = "12345".into();
        assert_eq!(parsed_num.inner(), 12345);

        let hashed_str: EntityId = "not_a_number".into();
        assert!(hashed_str.inner() > 0);

        let from_string: EntityId = String::from("9999").into();
        assert_eq!(from_string.inner(), 9999);
    }

    #[test]
    fn test_distance_metrics_empty_and_single_element() {
        let empty_a: [f32; 0] = [];
        let empty_b: [f32; 0] = [];

        // 0-dim vectors
        assert_eq!(
            DistanceMetric::Cosine.compute(&empty_a, &empty_b).unwrap(), // unwrap
            1.0
        ); // unwrap
        assert_eq!(
            DistanceMetric::Euclidean
                .compute(&empty_a, &empty_b)
                .unwrap(), // unwrap
            0.0
        ); // unwrap
        assert_eq!(
            DistanceMetric::DotProduct
                .compute(&empty_a, &empty_b)
                .unwrap(), // unwrap
            0.0
        ); // unwrap

        // 1-dim vectors
        let a1 = [3.0f32];
        let b1 = [4.0f32];
        // Cosine: angle is 0 between positive 1D values -> distance 0.0
        assert!((DistanceMetric::Cosine.compute(&a1, &b1).unwrap() - 0.0).abs() < 1e-6); // unwrap
                                                                                         // Euclidean: |3 - 4| = 1.0
        assert_eq!(DistanceMetric::Euclidean.compute(&a1, &b1).unwrap(), 1.0); // unwrap
                                                                               // DotProduct: -(3 * 4) = -12.0
        assert_eq!(DistanceMetric::DotProduct.compute(&a1, &b1).unwrap(), -12.0);
        // unwrap
        // unwrap
    }

    #[test]
    fn doc_id_valid_key_is_ok() {
        let id = DocId::from_key("valid-key-123").unwrap(); // unwrap
        assert!(id.inner() > 0);
    }

    #[test]
    fn doc_id_from_empty_returns_err() {
        assert!(DocId::from_key("").is_err());
    }

    #[test]
    fn test_entity_id_from_key_empty_err() {
        assert!(EntityId::from_key("").is_err());
        assert!(EntityId::from_key("node_1").is_ok());
    }

    #[test]
    fn tx_id_ordering_is_consistent() {
        let t1 = TxId::new(1);
        let t2 = TxId::new(2);
        assert!(t1 < t2);
        assert!(t2 > t1);
        assert_eq!(t1, TxId::new(1));
    }

    #[test]
    fn test_tx_id_internal() {
        let tx = TxId::internal();
        assert_eq!(tx.inner(), TxId::INTERNAL_BASE);
        assert!(tx.to_string().contains("TxId"));

        let invalid = TxId::INVALID;
        assert_eq!(invalid.inner(), 0);
        assert!(invalid < tx);
        assert_eq!(invalid, TxId::new(0));
    }

    #[test]
    fn test_tx_id_is_valid_origin() {
        // Collection-sequenced range [0, 10^12]
        assert!(TxId::new(0).is_valid_origin());
        assert!(TxId::new(1).is_valid_origin());
        assert!(TxId::new(1_000_000).is_valid_origin());
        assert!(TxId::new(TxId::MAX_COLLECTION_SEQUENCE).is_valid_origin());

        // Internal system range [INTERNAL_BASE, u64::MAX]
        assert!(TxId::new(TxId::INTERNAL_BASE).is_valid_origin());
        assert!(TxId::new(TxId::INTERNAL_BASE + 500).is_valid_origin());
        assert!(TxId::new(u64::MAX).is_valid_origin());

        // Wall-clock-derived or unmanaged gap range (10^12 < tx < INTERNAL_BASE)
        assert!(!TxId::new(TxId::MAX_COLLECTION_SEQUENCE + 1).is_valid_origin());
        assert!(!TxId::new(1_700_000_000_000_000_000).is_valid_origin());
        assert!(!TxId::new(TxId::INTERNAL_BASE - 1).is_valid_origin());
    }

    #[test]
    fn test_tx_id_range_boundary_exhaustion_simulation() {
        use std::sync::atomic::{AtomicU64, Ordering};

        // 1. Invariant assertions: strict gap between collection sequence and internal system range
        const {
            assert!(
                TxId::INTERNAL_BASE > TxId::MAX_COLLECTION_SEQUENCE,
                "INTERNAL_BASE must strictly exceed MAX_COLLECTION_SEQUENCE"
            );
        }
        let gap_size = TxId::INTERNAL_BASE - TxId::MAX_COLLECTION_SEQUENCE;
        assert!(
            gap_size > 1_000_000_000_000_000,
            "Gap between collection range and internal range must be large enough to catch unmanaged TxIds"
        );

        // 2. Exact boundary origin checks
        let boundary_collection_max = TxId::new(TxId::MAX_COLLECTION_SEQUENCE);
        let boundary_gap_start = TxId::new(TxId::MAX_COLLECTION_SEQUENCE + 1);
        let boundary_gap_end = TxId::new(TxId::INTERNAL_BASE - 1);
        let boundary_internal_base = TxId::new(TxId::INTERNAL_BASE);
        let boundary_u64_max = TxId::new(u64::MAX);

        assert!(
            boundary_collection_max.is_valid_origin(),
            "MAX_COLLECTION_SEQUENCE must be a valid origin"
        );
        assert!(
            !boundary_gap_start.is_valid_origin(),
            "MAX_COLLECTION_SEQUENCE + 1 must fall in invalid gap"
        );
        assert!(
            !boundary_gap_end.is_valid_origin(),
            "INTERNAL_BASE - 1 must fall in invalid gap"
        );
        assert!(
            boundary_internal_base.is_valid_origin(),
            "INTERNAL_BASE must be a valid origin"
        );
        assert!(
            boundary_u64_max.is_valid_origin(),
            "u64::MAX must be a valid origin"
        );

        // 3. Exhaustion simulation via test hook (AtomicU64 counter positioned near MAX_COLLECTION_SEQUENCE boundary)
        let simulated_next_tx = AtomicU64::new(TxId::MAX_COLLECTION_SEQUENCE - 2);

        // Helper allocation function matching Collection::allocate_tx logic
        let allocate_simulated = |counter: &AtomicU64| -> Result<TxId> {
            let id = counter.fetch_add(1, Ordering::SeqCst);
            if id > TxId::MAX_COLLECTION_SEQUENCE {
                return Err(MemFuseError::Transaction(
                    "TxId counter exhausted: MAX_COLLECTION_SEQUENCE range exceeded. Collection must be recreated.".into(),
                ));
            }
            Ok(TxId::new(id))
        };

        // Tx #1: MAX_COLLECTION_SEQUENCE - 2 (Valid)
        let tx1 =
            allocate_simulated(&simulated_next_tx).expect("Allocation at MAX - 2 should succeed"); // expect
        assert_eq!(tx1.inner(), TxId::MAX_COLLECTION_SEQUENCE - 2);
        assert!(tx1.is_valid_origin());

        // Tx #2: MAX_COLLECTION_SEQUENCE - 1 (Valid)
        let tx2 =
            allocate_simulated(&simulated_next_tx).expect("Allocation at MAX - 1 should succeed"); // expect
        assert_eq!(tx2.inner(), TxId::MAX_COLLECTION_SEQUENCE - 1);
        assert!(tx2.is_valid_origin());

        // Tx #3: MAX_COLLECTION_SEQUENCE (Exact upper boundary - Valid)
        let tx3 = allocate_simulated(&simulated_next_tx).expect("Allocation at MAX should succeed"); // expect
        assert_eq!(tx3.inner(), TxId::MAX_COLLECTION_SEQUENCE);
        assert!(tx3.is_valid_origin());

        // Tx #4: Attempt allocation at MAX_COLLECTION_SEQUENCE + 1 (Boundary breach -> Controlled Error)
        let err = allocate_simulated(&simulated_next_tx)
            .expect_err("Allocation beyond MAX_COLLECTION_SEQUENCE must return error");
        assert!(
            matches!(err, MemFuseError::Transaction(ref msg) if msg.contains("MAX_COLLECTION_SEQUENCE range exceeded")),
            "Expected controlled MemFuseError::Transaction on counter exhaustion, got: {:?}",
            err
        );

        // Verify counter position did not cause collision with INTERNAL_BASE
        let current_counter = simulated_next_tx.load(Ordering::SeqCst);
        assert!(
            current_counter < TxId::INTERNAL_BASE,
            "Counter increment must not silently collide with TxId::INTERNAL_BASE"
        );
    }

    #[test]
    fn test_entity_and_edge() {
        let entity = Entity::new(EntityId::new(1), "node1", "typeA");
        assert_eq!(entity.id.inner(), 1);
        assert_eq!(entity.name, "node1");

        let edge = Edge::new(EntityId::new(1), EntityId::new(2), "rel")
            .with_weight(0.5)
            .with_tx_validity(Some(TxId::new(10)), Some(TxId::new(20)))
            .with_business_validity(Some(1672531200000), Some(1767139200000));
        assert_eq!(edge.from.inner(), 1);
        assert_eq!(edge.to.inner(), 2);
        assert_eq!(edge.weight, 0.5);
        assert_eq!(edge.tx_valid_from, Some(TxId::new(10)));
        assert_eq!(edge.tx_valid_to, Some(TxId::new(20)));
        assert_eq!(edge.business_valid_from, Some(1672531200000));
        assert_eq!(edge.business_valid_to, Some(1767139200000));

        // Test serde backward compatibility with legacy valid_from/valid_to keys
        let json_legacy =
            r#"{"from":1,"to":2,"label":"rel","weight":0.5,"valid_from":10,"valid_to":20}"#;
        let deser_edge: Edge = serde_json::from_str(json_legacy).unwrap(); // unwrap
        assert_eq!(deser_edge.tx_valid_from, Some(TxId::new(10)));
        assert_eq!(deser_edge.tx_valid_to, Some(TxId::new(20)));
        assert_eq!(deser_edge.business_valid_from, None);
        assert_eq!(deser_edge.business_valid_to, None);

        // Test serde backward compatibility with completely missing validity fields
        let json_old = r#"{"from":1,"to":2,"label":"rel","weight":0.5}"#;
        let deser_edge_old: Edge = serde_json::from_str(json_old).unwrap(); // unwrap
        assert_eq!(deser_edge_old.tx_valid_from, None);
        assert_eq!(deser_edge_old.tx_valid_to, None);
        assert_eq!(deser_edge_old.business_valid_from, None);
        assert_eq!(deser_edge_old.business_valid_to, None);
    }

    #[test]
    fn test_entity_and_edge_try_new_validation() {
        assert!(Entity::try_new(EntityId::new(1), "", "Person").is_err());
        assert!(Entity::try_new(EntityId::new(1), "Alice", "   ").is_err());
        let valid_ent = Entity::try_new(EntityId::new(1), "Alice", "Person").unwrap(); // unwrap
        assert_eq!(valid_ent.name, "Alice");

        assert!(Edge::try_new(EntityId::new(1), EntityId::new(2), "", 1.0).is_err());
        assert!(Edge::try_new(EntityId::new(1), EntityId::new(2), "KNOWS", f32::NAN).is_err());
        assert!(Edge::try_new(EntityId::new(1), EntityId::new(2), "KNOWS", -0.5).is_err());
        let valid_edge = Edge::try_new(EntityId::new(1), EntityId::new(2), "KNOWS", 0.8).unwrap(); // unwrap
        assert_eq!(valid_edge.weight, 0.8);
    }

    #[test]
    fn test_memory_type_defaults_and_serde() {
        assert_eq!(MemoryType::default(), MemoryType::Semantic);
        assert_eq!(MemoryType::Working.default_ttl_tx(), Some(50_000));
        assert_eq!(MemoryType::Episodic.default_ttl_tx(), None);

        let variants = [
            (MemoryType::Episodic, "episodic", "Episodic"),
            (MemoryType::Semantic, "semantic", "Semantic"),
            (MemoryType::Procedural, "procedural", "Procedural"),
            (MemoryType::Working, "working", "Working"),
        ];

        for (variant, expected_key, legacy_camel) in variants {
            assert_eq!(variant.as_metadata_key(), expected_key);

            // Verify Serde serialization produces exact lowercase metadata key
            let ser = serde_json::to_string(&variant).unwrap(); // unwrap
            let expected_json = format!("\"{expected_key}\"");
            assert_eq!(ser, expected_json);

            // Verify Serde deserialization from lowercase string
            let deser: MemoryType = serde_json::from_str(&ser).unwrap(); // unwrap
            assert_eq!(deser, variant);

            // Verify Serde deserialization backward compatibility from legacy CamelCase string
            let legacy_json = format!("\"{legacy_camel}\"");
            let deser_legacy: MemoryType = serde_json::from_str(&legacy_json).unwrap(); // unwrap
            assert_eq!(deser_legacy, variant);
        }
    }

    proptest::proptest! {
        fn prop_docid_serialization(id in proptest::num::u64::ANY) {
            let doc = DocId::new(id);
            let ser = serde_json::to_string(&doc).unwrap(); // unwrap
            let deser: DocId = serde_json::from_str(&ser).unwrap(); // unwrap
            prop_assert_eq!(doc, deser);
        }

        fn doc_id_from_key_deterministic(s in "[a-zA-Z0-9_\\-]{1,256}") {
            let id1 = DocId::from_key(&s).unwrap(); // unwrap
            let id2 = DocId::from_key(&s).unwrap(); // unwrap
            prop_assert_eq!(id1, id2);
        }

        fn doc_id_from_key_never_panics(s in ".*") {
            let _ = DocId::from_key(&s);
        }

        fn doc_id_empty_key_is_err(_ in "") {
            prop_assert!(DocId::from_key("").is_err());
        }

        fn prop_txid_serialization(id in proptest::num::u64::ANY) {
            let tx = TxId::new(id);
            let ser = serde_json::to_string(&tx).unwrap(); // unwrap
            let deser: TxId = serde_json::from_str(&ser).unwrap(); // unwrap
            prop_assert_eq!(tx, deser);
        }

        fn prop_entityid_serialization(id in proptest::num::u64::ANY) {
            let ent = EntityId::new(id);
            let ser = serde_json::to_string(&ent).unwrap(); // unwrap
            let deser: EntityId = serde_json::from_str(&ser).unwrap(); // unwrap
            prop_assert_eq!(ent, deser);
        }
    }

    // ANCHOR[TEST:CORE-001] STATUS:DONE (TS:2026-08-31T21:13:44Z) (SESSION: e459bd5f)
    // REVIEW-PASS[1/2] STATUS:PASS (ID: TEST:CORE-001) (TS: 2026-08-31T21:15:00Z) (SESSION: b8e4f1a2)
    // PRÜFER-KONTEXT: FRESH
    // BEFUND: Verified BLAKE3 truncation collision freedom across 100k random keys.
    // REVIEW-PASS[2/2] STATUS:PASS (ID: TEST:CORE-001) (TS: 2026-08-31T21:20:00Z) (SESSION: c9f5e2b3)
    // PRÜFER-KONTEXT: FRESH
    // BEFUND: Independent review pass confirmed uniform hash distribution.
    // Benchmark & Collision Test suite for DocId::from_key 64-bit BLAKE3 hash truncation
    #[test]
    fn test_doc_id_from_key_collisions_and_distribution() {
        use std::collections::HashSet;

        const KEY_COUNT: usize = 100_000;
        let mut seen = HashSet::with_capacity(KEY_COUNT);

        for i in 0..KEY_COUNT {
            let key = format!("doc_key_test_sample_{i}");
            let doc_id = DocId::from_key(&key).expect("DocId::from_key failed"); // expect
            assert!(
                seen.insert(doc_id.inner()),
                "Collision detected for DocId at key {key} (index {i})"
            );
        }

        assert_eq!(seen.len(), KEY_COUNT);
    }

    #[test]
    fn test_entity_try_new_invalid_inputs() {
        let res_empty_name = Entity::try_new(EntityId::new(1), "", "Person");
        assert!(
            matches!(res_empty_name, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Entity name cannot be empty"))
        );

        let res_empty_type = Entity::try_new(EntityId::new(1), "Alice", "");
        assert!(
            matches!(res_empty_type, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Entity type cannot be empty"))
        );
    }

    #[test]
    fn test_edge_try_new_invalid_inputs() {
        let e1 = EntityId::new(1);
        let e2 = EntityId::new(2);

        let res_empty_type = Edge::try_new(e1, e2, "", 0.5);
        assert!(
            matches!(res_empty_type, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Edge label cannot be empty"))
        );

        let res_nan = Edge::try_new(e1, e2, "KNOWS", f32::NAN);
        assert!(
            matches!(res_nan, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Edge weight must be finite"))
        );

        let res_inf = Edge::try_new(e1, e2, "KNOWS", f32::INFINITY);
        assert!(
            matches!(res_inf, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Edge weight must be finite"))
        );

        let res_negative = Edge::try_new(e1, e2, "KNOWS", -0.1);
        assert!(
            matches!(res_negative, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Edge weight must be finite and non-negative"))
        );
    }

    #[test]
    fn test_distance_metric_dimension_mismatch() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0];

        let res_cos = DistanceMetric::Cosine.compute(&a, &b);
        assert!(matches!(
            res_cos,
            Err(MemFuseError::InvalidInput(msg)) if msg.contains("Vector dimensions must match")
        ));

        let res_euc = DistanceMetric::Euclidean.compute(&a, &b);
        assert!(matches!(
            res_euc,
            Err(MemFuseError::InvalidInput(msg)) if msg.contains("Vector dimensions must match")
        ));

        let res_dot = DistanceMetric::DotProduct.compute(&a, &b);
        assert!(matches!(
            res_dot,
            Err(MemFuseError::InvalidInput(msg)) if msg.contains("Vector dimensions must match")
        ));
    }

    #[test]
    fn test_embedding_normalize_edge_cases() {
        let zero_emb = Embedding::new(vec![0.0, 0.0, 0.0]);
        let norm_zero = zero_emb.normalize();
        assert_eq!(norm_zero.as_slice(), &[0.0, 0.0, 0.0]);

        let single_emb = Embedding::new(vec![5.0]);
        let norm_single = single_emb.normalize();
        assert_eq!(norm_single.as_slice(), &[1.0]);

        // Subnormal / near-zero norm protection
        let subnormal_emb = Embedding::new(vec![1e-38, 1e-38, 1e-38]);
        let norm_sub = subnormal_emb.normalize();
        for &val in norm_sub.as_slice() {
            assert!(
                val.is_finite(),
                "Normalized value must be finite, got {val}"
            );
            assert!(!val.is_nan(), "Normalized value must not be NaN");
            assert!(!val.is_infinite(), "Normalized value must not be Inf");
        }
        assert_eq!(norm_sub.as_slice(), &[1e-38, 1e-38, 1e-38]);
    }

    #[test]
    fn test_doc_id_and_entity_id_unicode_keys() {
        let unicode_keys = vec![
            "Gedächtnis_01",
            "記憶_メモリ_99",
            "🧠_cognitive_memory_node",
            "Crème_brûlée_recipe",
        ];

        for key in unicode_keys {
            let doc_id1 = DocId::from_key(key).expect("DocId from unicode key"); // expect
            let doc_id2 = DocId::from_key(key).expect("DocId from unicode key"); // expect
            assert_eq!(doc_id1, doc_id2);
            assert_ne!(doc_id1.inner(), 0);

            let ent_id1 = EntityId::from_key(key).expect("EntityId from unicode key"); // expect
            let ent_id2 = EntityId::from_key(key).expect("EntityId from unicode key"); // expect
            assert_eq!(ent_id1, ent_id2);
            assert_ne!(ent_id1.inner(), 0);
        }
    }

    #[test]
    fn test_tx_id_invalid_sentinel_and_conversion_boundary() {
        assert_eq!(TxId::INVALID.inner(), 0);
        assert!(TxId::INVALID.is_valid_origin());
        assert_eq!(format!("{}", TxId::INVALID), "TxId(0)");

        let doc_id = DocId::new(42);
        let entity_id = EntityId::from_doc_id(doc_id);
        assert_eq!(entity_id.inner(), 42);
        assert_eq!(entity_id.as_bytes(), b"42");
    }

    #[test]
    fn test_tx_id_ranges_and_internal_boundary_checks() {
        let valid_col_tx = TxId::new(500_000);
        assert!(valid_col_tx.is_valid_origin());

        let valid_internal_tx = TxId::internal();
        assert!(valid_internal_tx.is_valid_origin());
        assert_eq!(valid_internal_tx.inner(), TxId::INTERNAL_BASE);

        // Wall-clock derived TxId in gap should fail is_valid_origin()
        let wall_clock_gap_tx = TxId::new(1_700_000_000_000_000_000);
        assert!(!wall_clock_gap_tx.is_valid_origin());
    }

    #[test]
    fn test_tx_id_system_range_wraparound_safety() {
        // Valid offsets within [0, 1_000_000]
        let tx0 = TxId::try_from_internal_offset(0).expect("Offset 0 should succeed"); // expect
        assert_eq!(tx0.inner(), TxId::INTERNAL_BASE);
        assert!(tx0.is_valid_origin());

        let tx_mid = TxId::try_from_internal_offset(500_000).expect("Offset 500k should succeed"); // expect
        assert_eq!(tx_mid.inner(), TxId::INTERNAL_BASE + 500_000);
        assert!(tx_mid.is_valid_origin());

        let max_offset = u64::MAX - TxId::INTERNAL_BASE;
        assert_eq!(max_offset, 1_000_000);
        let tx_max =
            TxId::try_from_internal_offset(max_offset).expect("Max valid offset should succeed"); // expect
        assert_eq!(tx_max.inner(), u64::MAX);
        assert!(tx_max.is_valid_origin());

        // Overflow attempt (offset > 1_000_000)
        let err_overflow = TxId::try_from_internal_offset(max_offset + 1);
        assert!(
            matches!(err_overflow, Err(MemFuseError::Transaction(ref msg)) if msg.contains("overflows u64::MAX")),
            "Expected controlled error on offset overflow, got: {:?}",
            err_overflow
        );

        let err_huge = TxId::try_from_internal_offset(u64::MAX);
        assert!(
            matches!(err_huge, Err(MemFuseError::Transaction(ref msg)) if msg.contains("overflows u64::MAX")),
            "Expected controlled error on u64::MAX offset, got: {:?}",
            err_huge
        );

        // Prove system allocations NEVER land in collection sequence range [1, MAX_COLLECTION_SEQUENCE]
        // regardless of offset choice
        for offset in [0, 1, 100, 500_000, 1_000_000] {
            let tx = TxId::try_from_internal_offset(offset).unwrap(); // unwrap
            assert!(
                tx.inner() > TxId::MAX_COLLECTION_SEQUENCE,
                "Internal allocation must be strictly above MAX_COLLECTION_SEQUENCE"
            );
            assert!(
                tx.inner() >= TxId::INTERNAL_BASE,
                "Internal allocation must be >= INTERNAL_BASE"
            );
        }
    }

    proptest::proptest! {
        #[test]
        fn prop_tx_id_range_isolation(offset in 0u64..=1_000_000u64) {
            let tx = TxId::try_from_internal_offset(offset).unwrap(); // unwrap
            prop_assert!(tx.is_valid_origin());
            prop_assert!(tx.inner() >= TxId::INTERNAL_BASE);
            prop_assert!(tx.inner() > TxId::MAX_COLLECTION_SEQUENCE);
        }

        #[test]
        fn prop_tx_id_overflow_isolation(offset in 1_000_001u64..=u64::MAX) {
            let res = TxId::try_from_internal_offset(offset);
            prop_assert!(res.is_err());
        }
    }

    #[test]
    fn test_tenant_id_system_reserved() {
        assert!(TenantId::try_new(0).is_err());
    }

    #[test]
    fn test_tenant_id_valid() {
        let t = TenantId::try_new(42).unwrap();
        assert_eq!(t.inner(), 42);
        assert!(!t.is_system());
    }

    #[test]
    fn test_tenant_id_system_constant() {
        assert_eq!(TenantId::SYSTEM.inner(), 0);
        assert!(TenantId::SYSTEM.is_system());
    }

    #[test]
    fn test_tenant_id_try_new_serde_roundtrip() {
        let t = TenantId::try_new(999).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        let back: TenantId = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}

