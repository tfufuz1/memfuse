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

/// Internal document identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct DocId(pub u64);

impl DocId {
    pub const MAX: Self = Self(u64::MAX);
    pub const MIN: Self = Self(0);

    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    pub fn from_key(key: &str) -> Result<Self> {
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
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.to_string().into_bytes()
    }

    pub fn from_doc_id(doc_id: DocId) -> Self {
        Self(doc_id.inner())
    }

    pub fn from_key(key: &str) -> Self {
        DocId::from_key(key)
            .map(|d| Self(d.inner()))
            .unwrap_or_else(|_| Self::from(key))
    }
}

impl From<u64> for EntityId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<&str> for EntityId {
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
    /// Base for internal/system transaction IDs. Internal TxIds count upward
    /// from this value to avoid collision with user-facing TxIds (which count
    /// upward from 1). This reserves the top ~1M of the u64 space for system use.
    pub const INTERNAL_BASE: u64 = u64::MAX - 1_000_000;

    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }

    /// Returns a new internal/system transaction ID.
    #[inline]
    pub const fn internal() -> Self {
        Self(Self::INTERNAL_BASE)
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
    #[default]
    Cosine,
    Euclidean,
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
    // TODO[STABILIZE][memfuse-core][CRITICAL][PANIC-SAFETY]
    // PROBLEM: Euclidean and DotProduct distance computation on u8 vectors can cause integer overflow.
    // BEWEIS: If vectors have length > 66050 and diff is maximum (255), sum/dot accumulates to > u32::MAX, causing panic in debug mode and wrapping in release mode.
    // URSACHE: Sum/dot is accumulated in a u32 variable which is not guarded against overflow.
    // LÖSUNG: Use saturating addition or check/cast to u64 during accumulation and return error/saturate, or validate max dimension at the start of the function.
    // VERIFIKATION: Add a test `test_distance_metrics_u8_overflow` with vector length 100_000 populated with 255.
    // ABHÄNGIGKEIT: None
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
    pub data: Vec<f32>,
}

impl Embedding {
    pub fn new(data: Vec<f32>) -> Self {
        Self { data }
    }

    #[inline]
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    pub fn l2_norm(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

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
    pub doc_id: DocId,
    pub score: f32,
}

impl ScoredDocument {
    pub fn new(doc_id: DocId, score: f32) -> Self {
        Self { doc_id, score }
    }
}

/// Graph entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    #[serde(default)]
    pub name: String,
    pub entity_type: String,
    #[serde(default)]
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
}

impl Entity {
    pub fn new(id: EntityId, name: impl Into<String>, entity_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            entity_type: entity_type.into(),
            attributes: Default::default(),
        }
    }
}

/// Graph edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: EntityId,
    pub to: EntityId,
    pub label: String,
    pub weight: f32,
}

impl Edge {
    pub fn new(from: EntityId, to: EntityId, label: impl Into<String>) -> Self {
        Self {
            from,
            to,
            label: label.into(),
            weight: 1.0,
        }
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prop_assert_eq;

    #[test]
    fn test_doc_id_from_key_no_panic() {
        let keys = vec![
            "",
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
        let id1 = DocId::from_key(key).unwrap();
        let id2 = DocId::from_key(key).unwrap();
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
        let ser = serde_json::to_string(&doc).unwrap();
        let deser: DocId = serde_json::from_str(&ser).unwrap();
        assert_eq!(doc, deser);

        // TxId
        let tx = TxId::new(TxId::INTERNAL_BASE + 5);
        let ser = serde_json::to_string(&tx).unwrap();
        let deser: TxId = serde_json::from_str(&ser).unwrap();
        assert_eq!(tx, deser);

        // EntityId
        let ent = EntityId::new(999);
        let ser = serde_json::to_string(&ent).unwrap();
        let deser: EntityId = serde_json::from_str(&ser).unwrap();
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
        assert_eq!(DistanceMetric::Cosine.compute(&a, &b).unwrap(), 1.0);
        // Euclidean: sqrt(1^2 + 1^2) = sqrt(2)
        assert_eq!(
            DistanceMetric::Euclidean.compute(&a, &b).unwrap(),
            2.0f32.sqrt()
        );
        // DotProduct: -(0) = 0.0
        assert_eq!(DistanceMetric::DotProduct.compute(&a, &b).unwrap(), 0.0);
    }

    #[test]
    fn test_distance_metrics_u8() {
        let a = [10, 20];
        let b = [20, 30];
        // Euclidean: (10-20)^2 + (20-30)^2 = 100 + 100 = 200
        assert_eq!(DistanceMetric::Euclidean.compute_u8(&a, &b).unwrap(), 200);
        // DotProduct: 10*20 + 20*30 = 200 + 600 = 800
        assert_eq!(DistanceMetric::DotProduct.compute_u8(&a, &b).unwrap(), 800);
        // Cosine: orthogonal vectors → distance 1.0 → 1_000_000
        let orth_a: [u8; 2] = [255, 0];
        let orth_b: [u8; 2] = [0, 255];
        assert_eq!(
            DistanceMetric::Cosine.compute_u8(&orth_a, &orth_b).unwrap(),
            1_000_000
        );
        // Cosine: identical vectors → distance 0.0 → 0
        assert_eq!(DistanceMetric::Cosine.compute_u8(&a, &a).unwrap(), 0);
    }

    #[test]
    fn test_cosine_u8_ranking_matches_f32() {
        // FIND-COR-002: Verify that u8 cosine ranking matches f32 cosine ranking
        let query = [100u8, 200, 50, 150];
        let close_vec = [110u8, 190, 60, 140]; // similar direction
        let far_vec = [10u8, 20, 250, 5]; // different direction

        let dist_close = DistanceMetric::Cosine
            .compute_u8(&query, &close_vec)
            .unwrap();
        let dist_far = DistanceMetric::Cosine.compute_u8(&query, &far_vec).unwrap();
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
        let f32_close = DistanceMetric::Cosine.compute(&q_f32, &c_f32).unwrap();
        let f32_far = DistanceMetric::Cosine.compute(&q_f32, &f_f32).unwrap();
        assert!(f32_close < f32_far, "f32 ranking mismatch");
    }

    #[test]
    fn test_tx_id_internal() {
        let tx = TxId::internal();
        assert_eq!(tx.inner(), TxId::INTERNAL_BASE);
        assert!(tx.to_string().contains("TxId"));
    }

    #[test]
    fn test_entity_and_edge() {
        let entity = Entity::new(EntityId::new(1), "node1", "typeA");
        assert_eq!(entity.id.inner(), 1);
        assert_eq!(entity.name, "node1");

        let edge = Edge::new(EntityId::new(1), EntityId::new(2), "rel").with_weight(0.5);
        assert_eq!(edge.from.inner(), 1);
        assert_eq!(edge.to.inner(), 2);
        assert_eq!(edge.weight, 0.5);
    }

    proptest::proptest! {
        #[test]
        fn prop_docid_serialization(id in proptest::num::u64::ANY) {
            let doc = DocId::new(id);
            let ser = serde_json::to_string(&doc).unwrap();
            let deser: DocId = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(doc, deser);
        }

        #[test]
        fn prop_txid_serialization(id in proptest::num::u64::ANY) {
            let tx = TxId::new(id);
            let ser = serde_json::to_string(&tx).unwrap();
            let deser: TxId = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(tx, deser);
        }

        #[test]
        fn prop_entityid_serialization(id in proptest::num::u64::ANY) {
            let ent = EntityId::new(id);
            let ser = serde_json::to_string(&ent).unwrap();
            let deser: EntityId = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(ent, deser);
        }
    }
}
