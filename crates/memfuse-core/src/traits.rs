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

/// Abstract contract for generating consistent checkpoints.
#[async_trait]
pub trait Checkpoint: Send + Sync + 'static {
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

/// Storage Engine trait — abstrahiert die LSM-Tree-Persistenz.
///
/// # Dyn-Kompatibilität
/// Dieser Trait ist durch `#[async_trait]` vtable-kompatibel (dyn-safe).
/// Alle `async fn`-Methoden werden zu `Pin<Box<dyn Future<...>>>` desugared.
///
/// # Invarianten
/// - Implementierungen DÜRFEN NICHT paniken (Zero-Panic Doctrine)
/// - Alle Fehler werden über `crate::Result<T>` propagiert
#[async_trait]
pub trait StorageEngine: Send + Sync + 'static {
    /// Retrieves a value by key.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Retrieves a value by key at a specific sequence number (MVCC).
    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>>;

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

    /// Rolls back a transaction — discards stagged writes for the given ID.
    async fn rollback(&self, tx_id: TxId) -> Result<()>;

    /// Rolls back the entire storage state to a specific transaction ID.
    ///
    /// **Implementor contract**: MUST physically revert all state beyond `tx_id`.
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;

    /// Flushes the memtable to disk.
    async fn flush(&self) -> Result<()>;

    /// Returns storage statistics.
    async fn stats(&self) -> Result<StorageStats>;

    /// Returns the last sequence number committed to storage.
    async fn last_seq_no(&self) -> Result<u64>;

    /// Returns the last transaction ID committed to storage.
    async fn last_tx_id(&self) -> Result<TxId>;

    /// Pins a checkpoint for the given sequence number.
    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()>;

    /// Unpins a checkpoint for the given sequence number.
    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()>;

    /// Scans a range of keys with the given prefix.
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Scans a range of keys between `start` and `end` bounds.
    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

// ANCHOR:ARCH:CONTRACT-INDEX-001 — Implementor: HnswIndex (memfuse-index/src/hnsw.rs)
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Rebuild: Automatisch bei >20% gelöschten Nodes.

/// Vector Index Trait — abstrahiert die HNSW-Vektorsuche.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
#[async_trait]
pub trait VectorIndex: Send + Sync + 'static {
    /// Inserts a vector with an associated document ID.
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;
    
    /// Returns all active (non-deleted) document IDs in the index.
    async fn all_doc_ids(&self) -> Result<Vec<DocId>> {
        Ok(Vec::new())
    }

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
        if filter.is_some() {
            return Err(crate::error::MemFuseError::Index(
                "Filter support not implemented for this vector engine".into(),
            ));
        }
        self.search(query, k).await
    }

    /// Deletes a vector by its document ID.
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;

    /// Commits a transaction.
    async fn commit(&self, tx: TxId) -> Result<()>;

    /// Rolls back a transaction.
    async fn rollback(&self, tx: TxId) -> Result<()>;

    /// Rolls back the entire index state to a specific transaction ID.
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;

    /// Returns the last transaction ID processed by the index.
    async fn last_tx_id(&self) -> Result<u64>;

    /// Returns the number of vectors in the index.
    async fn len(&self) -> usize;

    /// Returns true if the index is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns index statistics.
    async fn stats(&self) -> Result<VectorIndexStats>;
}

/// Text embedding engine trait.
#[async_trait]
pub trait TextEmbeddingEngine: Send + Sync + 'static {
    /// Generates an embedding for the given text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Statistics for a text index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextIndexStats {
    /// Total number of documents indexed.
    pub num_documents: usize,
    /// Total number of tokens across all documents.
    pub num_tokens: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
}

/// Text-Index Trait — abstrahiert BM25/Inverted-Index-Operationen.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
#[async_trait]
pub trait TextIndex: Send + Sync + 'static {
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

    /// Rolls back the entire index state to a specific transaction ID.
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;

    /// Returns index statistics.
    async fn stats(&self) -> Result<TextIndexStats>;
}

// ANCHOR:ARCH:TRAIT-003 — Graph Engine Trait (Signal 3)
// WP:WP-6.x PRIO:4 NEEDS:WP-2.1
// STATUS:SCAFFOLD DATE:2026-05-17

/// Graph-Index Trait — CSR-basierte Entity-Relation-Traversal.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
#[async_trait]
pub trait GraphIndex: Send + Sync + 'static {
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

    /// Rolls back the entire graph state to a specific transaction ID.
    async fn rollback_to_tx(&self, tx_id: crate::types::TxId) -> crate::Result<()>;

    /// Collects statistics for the Graph.
    async fn stats(&self) -> crate::Result<GraphIndexStats>;
}

/// Statistics for the GraphIndex layer.
#[derive(Debug, Clone)]
pub struct GraphIndexStats {
    /// Number of active nodes (Entities).
    pub num_entities: usize,
    /// Number of active edges.
    pub num_edges: usize,
    /// Total bytes allocated by CSR representation.
    pub memory_usage_bytes: usize,
}

/// Distance calculator trait for vector comparison.
pub trait DistanceCalculator: Send + Sync {
    /// Computes the distance between two f32 vectors.
    fn compute_f32(&self, a: &[f32], b: &[f32]) -> Result<f32>;

    /// Computes the distance between two u8 vectors.
    fn compute_u8(&self, a: &[u8], b: &[u8]) -> Result<u32>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_serialization() {
        let v_stats = VectorIndexStats {
            num_vectors: 100,
            memory_usage_bytes: 1024,
            num_layers: 5,
        };
        let ser = serde_json::to_string(&v_stats).unwrap();
        let deser: VectorIndexStats = serde_json::from_str(&ser).unwrap();
        assert_eq!(v_stats.num_vectors, deser.num_vectors);

        let s_stats = StorageStats {
            num_segments: 2,
            total_size_bytes: 2048,
            memtable_size_bytes: 512,
        };
        let ser = serde_json::to_string(&s_stats).unwrap();
        let deser: StorageStats = serde_json::from_str(&ser).unwrap();
        assert_eq!(s_stats.total_size_bytes, deser.total_size_bytes);

        let t_stats = TextIndexStats {
            num_documents: 10,
            num_tokens: 1000,
            memory_usage_bytes: 256,
        };
        let ser = serde_json::to_string(&t_stats).unwrap();
        let deser: TextIndexStats = serde_json::from_str(&ser).unwrap();
        assert_eq!(t_stats.num_documents, deser.num_documents);
    }

    #[tokio::test]
    async fn test_storage_engine_default_put_batch() {
        type KVPair = (Vec<u8>, Vec<u8>);
        type Log = std::sync::Arc<std::sync::Mutex<Vec<KVPair>>>;
        struct MockStorage(Log);
        #[async_trait::async_trait]
        impl StorageEngine for MockStorage {
            async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> { Ok(None) }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> { Ok(None) }
            async fn put(&self, _: TxId, key: &[u8], value: &[u8]) -> Result<()> {
                self.0.lock().unwrap().push((key.to_vec(), value.to_vec()));
                Ok(())
            }
            async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> { Ok(()) }
            async fn commit(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn rollback(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn flush(&self) -> Result<()> { Ok(()) }
            async fn stats(&self) -> Result<StorageStats> { 
                Ok(StorageStats { num_segments: 0, total_size_bytes: 0, memtable_size_bytes: 0 }) 
            }
            async fn last_seq_no(&self) -> Result<u64> { Ok(0) }
            async fn last_tx_id(&self) -> Result<TxId> { Ok(TxId(0)) }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> { Ok(()) }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> { Ok(()) }
            async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> { Ok(vec![]) }
            async fn scan(&self, _: std::ops::Bound<&[u8]>, _: std::ops::Bound<&[u8]>) -> Result<Vec<(Vec<u8>, Vec<u8>)>> { Ok(vec![]) }
        }

        let store = MockStorage(std::sync::Arc::new(std::sync::Mutex::new(vec![])));
        let entries = vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ];
        store.put_batch(TxId(1), &entries).await.unwrap();
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_vector_index_defaults() {
        struct MockIndex(std::sync::atomic::AtomicUsize);
        #[async_trait::async_trait]
        impl VectorIndex for MockIndex {
            async fn insert(&self, _: TxId, _: DocId, _: &[f32]) -> Result<()> { 
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(()) 
            }
            async fn search(&self, _: &[f32], _: usize) -> Result<Vec<ScoredDocument>> { Ok(vec![]) }
            async fn delete(&self, _: TxId, _: DocId) -> Result<()> { Ok(()) }
            async fn commit(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn rollback(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> { Ok(()) }
            async fn last_tx_id(&self) -> Result<u64> { Ok(0) }
            async fn len(&self) -> usize { self.0.load(std::sync::atomic::Ordering::SeqCst) }
            async fn stats(&self) -> Result<VectorIndexStats> {
                Ok(VectorIndexStats { num_vectors: 0, memory_usage_bytes: 0, num_layers: 0 })
            }
        }

        let index = MockIndex(std::sync::atomic::AtomicUsize::new(0));
        assert!(index.is_empty().await);

        let vectors = vec![
            (DocId(1), [1.0, 2.0].as_slice()),
            (DocId(2), [3.0, 4.0].as_slice()),
        ];
        index.insert_batch(TxId(1), &vectors).await.unwrap();
        assert_eq!(index.len().await, 2);
        assert!(!index.is_empty().await);

        // Test search_filtered default error
        let res = index.search_filtered(&[1.0], 1, Some(&|_| true)).await;
        assert!(res.is_err());
    }
}
