//! Core trait definitions for MemFuse subsystems.
//!
//! These traits define the abstract interfaces that concrete implementations
//! must fulfill, enabling modularity and testability.

#![allow(async_fn_in_trait)]

// ANCHOR:ARCH:TRAITS-001 — Trait-Contracts sind das API-Rückgrat des Workspace.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// REGEL: Neue Methoden MÜSSEN Default-Impl haben (backward compat).

use crate::types::*;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Abstract contract for generating consistent checkpoints.
pub trait Checkpoint: Send + Sync {
    /// Takes a deterministic snapshot of the current state.
    async fn take_snapshot(&self, tx: TxId) -> Result<WorkflowState>;

    /// Rolls the state back to the specified checkpoint.
    async fn restore(&self, state: &WorkflowState) -> Result<()>;
}

/// Represents a point-in-time view of the database.
pub trait Snapshot: Send + Sync {
    /// Returns the sequence number for this snapshot.
    fn seq_no(&self) -> u64;
}

/// Statistics for a vector index implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndexStats {
    /// Number of active (non-deleted) vectors.
    pub num_vectors: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
    /// Number of HNSW layers.
    pub num_layers: usize,
}

/// Statistics for a storage engine implementation.
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
// TODO(FIND-COR-001): Trait requires #[async_trait] macro.
// Remove default implementations to prevent silent E0038/E0195 dyn-compatibility issues.
pub trait StorageEngine: Send + Sync {
    /// Retrieves a value by key.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Retrieves a value by key at a specific sequence number (MVCC).
    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
        let _ = seq;
        self.get(key).await
    }

    /// Stores a key-value pair as part of a transaction.
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;

    /// Stores multiple key-value pairs as part of a transaction.
    async fn put_batch(&self, tx_id: TxId, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<()> {
        for (key, value) in entries {
            self.put(tx_id, key, value).await?;
        }
        Ok(())
    }

    /// Deletes a key as part of a transaction.
    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()>;

    /// Commits a transaction — makes writes visible.
    async fn commit(&self, tx_id: TxId) -> Result<()>;

    /// Rolls back a transaction — discards writes.
    ///
    /// **Implementor contract**: MUST discard all staged writes for `tx_id`.
    /// The default no-op is only valid for test mocks. Production implementations
    /// MUST override this. See `LsmStorage::rollback` for reference.
    async fn rollback(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }

    /// Rolls back the entire storage state to a specific transaction ID.
    ///
    /// **Implementor contract**: MUST physically revert all state beyond `tx_id`,
    /// including WAL truncation and memtable reconstruction. The default no-op
    /// is only valid for test mocks. See `LsmStorage::rollback_to_tx` for reference.
    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }

    /// Flushes the memtable to disk.
    async fn flush(&self) -> Result<()>;

    /// Returns storage statistics.
    async fn stats(&self) -> Result<StorageStats>;

    /// Returns the last sequence number committed to storage.
    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }

    /// Returns the last transaction ID committed to storage.
    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
    }

    /// Pins a checkpoint for the given sequence number.
    async fn pin_checkpoint(&self, _seq_no: u64) -> Result<()> {
        Ok(())
    }

    /// Unpins a checkpoint for the given sequence number.
    async fn unpin_checkpoint(&self, _seq_no: u64) -> Result<()> {
        Ok(())
    }

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
pub trait VectorIndex: Send + Sync {
    /// Inserts a vector with an associated document ID.
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;

    /// Inserts multiple vectors with associated document IDs.
    async fn insert_batch(&self, tx: TxId, vectors: &[(DocId, &[f32])]) -> Result<()> {
        for (id, embedding) in vectors {
            self.insert(tx, *id, embedding).await?;
        }
        Ok(())
    }

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

    /// Returns the last transaction ID processed by the index.
    async fn last_tx_id(&self) -> Result<u64> {
        Ok(0)
    }

    /// Returns the number of vectors in the index.
    async fn len(&self) -> usize;

    /// Returns true if the index is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns index statistics.
    async fn stats(&self) -> Result<VectorIndexStats>;
}

/// Statistics for a text (inverted) index implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextIndexStats {
    /// Total number of documents indexed.
    pub num_documents: usize,
    /// Total number of tokens across all documents.
    pub num_tokens: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
}

/// Text index trait — abstracts over the inverted index and BM25 search.
pub trait TextIndex: Send + Sync {
    /// Searches for documents matching the query.
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;

    /// Inserts or updates a document in the index.
    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()>;

    /// Deletes a document from the index.
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;

    /// Commits a transaction.
    async fn commit(&self, tx: TxId) -> Result<()>;

    /// Rolls back a transaction.
    async fn rollback(&self, tx: TxId) -> Result<()>;

    /// Returns index statistics.
    async fn stats(&self) -> Result<TextIndexStats>;
}

// ANCHOR:ARCH:TRAIT-003 — Graph Engine Trait (Signal 3)
// WP:WP-6.x PRIO:4 NEEDS:WP-2.1
// STATUS:SCAFFOLD DATE:2026-05-17

/// Defines the contract for the CSR Graph traverse capabilities (Signal 3).
pub trait GraphIndex: Send + Sync {
    /// Traverses the entity graph using BFS up to a maximum number of hops.
    /// Distributes traversing decay weights across related entities.
    async fn traverse(
        &self,
        start_node: crate::types::EntityId,
        max_hops: usize,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>>;

    /// Inserts or updates a node entity.
    async fn add_entity(
        &self,
        tx: crate::types::TxId,
        entity: crate::types::Entity,
    ) -> crate::Result<()>;

    /// Inserts or updates an edge between two entities.
    async fn add_edge(&self, tx: crate::types::TxId, edge: crate::types::Edge)
        -> crate::Result<()>;

    /// Commits a transaction.
    async fn commit(&self, tx: crate::types::TxId) -> crate::Result<()>;

    /// Rolls back a transaction.
    async fn rollback(&self, tx: crate::types::TxId) -> crate::Result<()>;

    /// Collects statistics for the Graph.
    async fn stats(&self) -> crate::Result<GraphIndexStats>;
}

/// Statistics for a graph index implementation.
#[derive(Debug, Clone)]
pub struct GraphIndexStats {
    /// Number of active nodes (Entities).
    pub num_entities: usize,
    /// Number of active edges.
    pub num_edges: usize,
    /// Total bytes allocated by CSR representation.
    pub memory_usage_bytes: usize,
}
