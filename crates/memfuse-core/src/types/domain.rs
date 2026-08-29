//! Domain types for MemFuse.
//!
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

impl From<&str> for EntityId {
    /// Infallible conversion: parses as `u64` first, then falls back to BLAKE3 hash.
    /// For consistent error handling at API boundaries, prefer `EntityId::from_key`.
    fn from(s: &str) -> Self {
        if let Ok(val) = s.parse::<u64>() {
            Self(val)
        } else {
            let hash = blake3::hash(s.as_bytes());
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&hash.as_bytes()[..8]);
            Self(u64::from_le_bytes(buf))
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
                    1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))
                }
            }
            Self::Euclidean => {
                let mut sum = 0.0;
                for (x, y) in a.iter().zip(b.iter()) {
                    let diff = x - y;
                    sum += diff * diff;
                }
                sum.sqrt()
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
    /// Returns a `u32` distance value. For metrics that naturally produce floats
    /// (Cosine), the result is scaled by `1_000_000` for fixed-point ranking.
    ///
    /// # DotProduct semantics
    /// Unlike the f32 path (`compute()`), the u32 result is **not negated** since
    /// `u32` cannot represent negative values. The caller (e.g., HNSW quantized search)
    /// is responsible for inverting the ranking order when using DotProduct.
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
                Ok(sum.min(u32::MAX as u64) as u32)
            }
            Self::DotProduct => {
                // Raw dot product (unsigned). Caller handles ranking inversion.
                let mut dot = 0u64;
                for (&x, &y) in a.iter().zip(b.iter()) {
                    dot += (x as u64) * (y as u64);
                }
                Ok(dot.min(u32::MAX as u64) as u32)
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

/// Graph entity node representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique entity identifier.
    pub id: EntityId,
    /// Human-readable entity name.
    #[serde(default)]
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
}

/// Graph directed edge representation.
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
    /// Start of business validity; None = valid from the beginning of time.
    #[serde(default)]
    pub valid_from: Option<TxId>,
    /// End of business validity; None = currently valid.
    #[serde(default)]
    pub valid_to: Option<TxId>,
}

impl Edge {
    /// Creates a new `Edge` between source and target entities with a label.
    pub fn new(from: EntityId, to: EntityId, label: impl Into<String>) -> Self {
        Self {
            from,
            to,
            label: label.into(),
            weight: 1.0,
            valid_from: None,
            valid_to: None,
        }
    }

    /// Sets a custom weight on the edge.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Sets business validity window on the edge.
    pub fn with_validity(mut self, from: Option<TxId>, to: Option<TxId>) -> Self {
        self.valid_from = from;
        self.valid_to = to;
        self
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
}

impl Default for PprConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            convergence_epsilon: 1e-6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq};

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
    fn test_serialization_roundtrips() {
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
        // Euclidean: (10-20)^2 + (20-30)^2 = 100 + 100 = 200
        assert_eq!(DistanceMetric::Euclidean.compute_u8(&a, &b).unwrap(), 200); // unwrap
                                                                                // DotProduct: 10*20 + 20*30 = 200 + 600 = 800
        assert_eq!(DistanceMetric::DotProduct.compute_u8(&a, &b).unwrap(), 800); // unwrap
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
    ///   = 6_502_500_000 < u64::MAX. Gesättigter Rückgabewert: 4_294_967_295 (u32::MAX).
    /// - DotProduct: 255×255×100_000 = 6_502_500_000 < u64::MAX. Gesättigter Rückgabewert: u32::MAX.
    /// - Cosine: f64-Akkumulation, kein Ganzzahl-Overflow möglich.
    #[test]
    fn test_distance_metrics_u8_overflow() {
        // Identische Vektoren (diff=0): Euclidean=0, DotProduct=saturiert, Cosine=0
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

        // DotProduct: 255*255*100_000 = 6_502_500_000 > u32::MAX → muss auf u32::MAX sättigen
        let dot_same = DistanceMetric::DotProduct
            .compute_u8(&max_vec, &same_vec)
            .unwrap(); // unwrap
        assert_eq!(
            dot_same,
            u32::MAX,
            "DotProduct must saturate to u32::MAX for 100_000-element all-255 vectors"
        );

        // Cosine: identische Vektoren → Distanz 0 (cos_dist = 1 - 1 = 0)
        let cos_same = DistanceMetric::Cosine
            .compute_u8(&max_vec, &same_vec)
            .unwrap(); // unwrap
        assert_eq!(
            cos_same, 0,
            "Cosine distance of identical vectors must be 0"
        );

        // Worst-case Euclidean: maximale Differenz (255 vs. 0) → sum = 255²×100_000 = 6_502_500_000
        // Muss auf u32::MAX sättigen
        let zero_vec: Vec<u8> = vec![0u8; 100_000];
        let eucl_max = DistanceMetric::Euclidean
            .compute_u8(&max_vec, &zero_vec)
            .unwrap(); // unwrap
        assert_eq!(
            eucl_max,
            u32::MAX,
            "Euclidean must saturate to u32::MAX for max-diff 100_000-element vectors"
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
    fn test_cosine_u8_ranking_matches_f32() {
        // FIND-COR-002: Verify that u8 cosine ranking matches f32 cosine ranking
        let query = [100u8, 200, 50, 150];
        let close_vec = [110u8, 190, 60, 140]; // similar direction
        let far_vec = [10u8, 20, 250, 5]; // different direction

        let dist_close = DistanceMetric::Cosine
            .compute_u8(&query, &close_vec)
            .unwrap(); // unwrap
        let dist_far = DistanceMetric::Cosine.compute_u8(&query, &far_vec).unwrap(); // unwrap
                                                                                     // Close vector should have smaller cosine distance
        assert!(
            dist_close < dist_far,
            "Ranking mismatch: close={} far={}",
            dist_close,
            dist_far
        );

        // Cross-check with f32 ranking
        let q_f32: Vec<f32> = query.iter().map(|&x| x as f32).collect();
        let c_f32: Vec<f32> = close_vec.iter().map(|&x| x as f32).collect();
        let f_f32: Vec<f32> = far_vec.iter().map(|&x| x as f32).collect();
        let f32_close = DistanceMetric::Cosine.compute(&q_f32, &c_f32).unwrap(); // unwrap
        let f32_far = DistanceMetric::Cosine.compute(&q_f32, &f_f32).unwrap(); // unwrap
        assert!(f32_close < f32_far, "f32 ranking mismatch");
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
    fn test_entity_and_edge() {
        let entity = Entity::new(EntityId::new(1), "node1", "typeA");
        assert_eq!(entity.id.inner(), 1);
        assert_eq!(entity.name, "node1");

        let edge = Edge::new(EntityId::new(1), EntityId::new(2), "rel")
            .with_weight(0.5)
            .with_validity(Some(TxId::new(10)), Some(TxId::new(20)));
        assert_eq!(edge.from.inner(), 1);
        assert_eq!(edge.to.inner(), 2);
        assert_eq!(edge.weight, 0.5);
        assert_eq!(edge.valid_from, Some(TxId::new(10)));
        assert_eq!(edge.valid_to, Some(TxId::new(20)));

        // Test serde backward compatibility with missing valid_from/valid_to
        let json_old = r#"{"from":1,"to":2,"label":"rel","weight":0.5}"#;
        let deser_edge: Edge = serde_json::from_str(json_old).unwrap(); // unwrap
        assert_eq!(deser_edge.valid_from, None);
        assert_eq!(deser_edge.valid_to, None);
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
}
