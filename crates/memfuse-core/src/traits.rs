//! Core trait definitions for MemFuse subsystems.
//!
//! These traits define the abstract interfaces that concrete implementations
//! must fulfill, enabling modularity and testability.

// ANCHOR:ARCH:TRAITS-001 — Trait-Contracts sind das API-Rückgrat des Workspace.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// REGEL: Neue Methoden MÜSSEN Default-Impl haben (backward compat).

use crate::types::*;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Statistics for a text index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextIndexStats {
    /// Number of indexed documents.
    pub num_documents: usize,
    /// Number of unique terms in the vocabulary.
    pub vocabulary_size: usize,
    /// Total number of tokens across all documents.
    pub total_tokens: u64,
}

/// Statistics for a vector index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndexStats {
    /// Number of active (non-deleted) vectors.
    pub num_vectors: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
    /// Number of HNSW layers.
    pub num_layers: usize,
}

/// Statistics for the storage engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Number of SSTable segments.
    pub num_segments: usize,
    /// Total size of all SSTables in bytes.
    pub total_size_bytes: u64,
    /// Total size of memtables in bytes.
    pub memtable_size_bytes: u64,
}

// ANCHOR:ARCH:CONTRACT-STORAGE-001 — Implementor: LsmStorage (memfuse-store/src/lsm.rs)
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Lifecycle: put/delete → commit/rollback → flush(background).
/// Storage engine trait — abstracts over the LSM-Tree implementation.
#[async_trait]
pub trait StorageEngine: Send + Sync {
    /// Retrieves a value by key.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Stores a key-value pair as part of a transaction.
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;

    /// Deletes a key as part of a transaction.
    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()>;

    /// Commits a transaction — makes writes visible.
    async fn commit(&self, tx_id: TxId) -> Result<()>;

    /// Rolls back a transaction — discards writes.
    async fn rollback(&self, tx_id: TxId) -> Result<()>;

    /// Flushes the memtable to disk.
    async fn flush(&self) -> Result<()>;

    /// Returns storage statistics.
    async fn stats(&self) -> Result<StorageStats>;

    /// Scans a range of keys with the given prefix.
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Scans a range of keys between `start` and `end` bounds.
    ///
    /// Returns all non-tombstoned entries within the range, deduplicated
    /// by key with the newest sequence number winning.
    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let _ = (start, end);
        Ok(Vec::new())
    }
}

// ANCHOR:ARCH:CONTRACT-INDEX-001 — Implementor: HnswIndex (memfuse-index/src/hnsw.rs)
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Rebuild: Automatisch bei >20% gelöschten Nodes.
/// Vector index trait — abstracts over the HNSW implementation.
#[async_trait]
pub trait VectorIndex: Send + Sync {
    /// Inserts a vector with an associated document ID.
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;

    /// Searches for the k nearest neighbors to a query vector.
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;

    /// Searches with an optional filter predicate.
    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        // Default: ignore filter, delegate to basic search.
        let _ = filter;
        self.search(query, k).await
    }

    /// Deletes a vector by its document ID.
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;

    /// Commits a transaction.
    async fn commit(&self, tx: TxId) -> Result<()>;

    /// Rolls back a transaction.
    async fn rollback(&self, tx: TxId) -> Result<()>;

    /// Returns the number of vectors in the index.
    async fn len(&self) -> usize;

    /// Returns true if the index is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns index statistics.
    async fn stats(&self) -> Result<VectorIndexStats>;
}
