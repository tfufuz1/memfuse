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

// ANCHOR:ARCH:TOMBSTONE-001 — Bit 63 der SeqNo markiert Tombstones.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Verwendet in: lsm.rs, compaction.rs. Raw-SeqNo = seq & !TOMBSTONE_BIT.
/// Bit mask for identifying tombstones in sequence numbers.
pub const TOMBSTONE_BIT: u64 = 1 << 63;

/// Internal document identifier (u64, not exposed to users).
///
/// `DocId` is derived from a user-provided string key using a Blake3 hash.
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
    pub fn from_key(key: &str) -> Self {
        Self::try_from_key(key).expect("Blake3 hash must be 32 bytes")
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

        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
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

/// Transaction identifier used to group atomic operations.
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
        let current = self
            .memory_used
            .fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);
        if current + bytes > self.budget.memory_limit {
            self.memory_used
                .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
            return Err(MemFuseError::MemoryBudgetExceeded {
                used_mb: (current + bytes) / (1024 * 1024),
                limit_mb: self.budget.memory_limit / (1024 * 1024),
            });
        }
        Ok(())
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
