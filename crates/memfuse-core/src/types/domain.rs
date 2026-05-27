//! Domain-specific types for MemFuse (DocId, TxId, etc.).

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
        Self::try_from_key(key)
    }

    /// Convenience method to create a DocId from a string key.
    /// Panics if the key cannot be hashed (should never happen with Blake3).
    pub fn from_string(key: &str) -> Self {
        Self::from_key(key).expect("Blake3 hashing should not fail")
    }

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
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

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

/// Transaction identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct TxId(pub u64);

impl TxId {
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

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
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
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
    pub name: String,
    pub entity_type: String,
}

impl Entity {
    pub fn new(id: EntityId, name: impl Into<String>, entity_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            entity_type: entity_type.into(),
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
