//! Core trait definitions for MemFuse subsystems.
//!
//! These traits define the abstract interfaces that concrete implementations
//! must fulfill, enabling modularity and testability.

// FILE-CONTEXT
// STAND: 2026-08-30T21:51:46Z (SESSION: a43b7682)
// ZWECK: Kern-Trait-Hierarchien (StorageEngine, VectorIndex, TextIndex, GraphIndex) für Layer 0.
// INVARIANTEN: Downward-only Trait interfaces; neue Trait-Methoden brauchen Default-Impls (Abwärtskompatibilität).
// HOTSPOTS: 30-500
// NICHT-OFFENSICHTLICH: Default-Impls für nicht unterstützte Subsystem-Features werfen standardisiertes CapabilityUnsupported.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md (ADR-024)

// INVARIANT: Trait-Contracts sind das API-Rückgrat des Workspace.
// REGEL: Neue Methoden MÜSSEN Default-Impl haben (backward compat).

use crate::types::*;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Trait and mock definitions for text embedding providers and LLMs.
pub mod embedding;
pub use embedding::*;

/// Abstract contract for generating consistent checkpoints.
#[async_trait]
pub trait Checkpoint: Send + Sync + 'static {
    /// Takes a deterministic snapshot of the current state.
    async fn take_snapshot(&self, tx: TxId) -> Result<WorkflowState>;

    /// Rolls the state back to the specified checkpoint.
    async fn restore(&self, state: &WorkflowState) -> Result<()>;
}

/// Unified Checkpoint Coordinator Trait combining named, TxId+seq_no-scoped, persistent checkpoints.
///
/// # Dyn-Safety Note
/// This trait is intentionally NOT dyn-compatible without specifying the associated type `Meta`
/// (e.g. `Arc<dyn CheckpointCoordinator<Meta = ...>>`) or using a concrete type, due to the
/// associated type `type Meta: Send + Sync`.
///
/// # DECISION-REF
/// ADR-011 — Consolidated Checkpoint Subsystem Architecture (resolving AGT-STORE-002).
#[async_trait]
pub trait CheckpointCoordinator: Send + Sync + 'static {
    /// Type representing checkpoint metadata.
    type Meta: Send + Sync;

    /// Creates and persists a new named checkpoint.
    async fn create_named_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<Self::Meta>;

    /// Restores database state to a named checkpoint.
    async fn restore_named_checkpoint(&self, name: &str) -> Result<Self::Meta>;

    /// Deletes a checkpoint by name.
    async fn drop_named_checkpoint(&self, name: &str) -> Result<()>;

    /// Lists all active checkpoints.
    async fn list_named_checkpoints(&self) -> Result<Vec<Self::Meta>>;
}

/// Represents a point-in-time view of the database.
pub trait Snapshot: Send + Sync {
    /// Returns the sequence number for this snapshot.
    fn seq_no(&self) -> u64;
}

/// Statistics for a vector index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VectorIndexStats {
    /// Number of active (non-deleted) vectors.
    pub num_vectors: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
    /// Number of HNSW layers.
    pub num_layers: usize,
    /// Fraction of deleted vectors (0.0 to 1.0).
    #[serde(default)]
    pub deleted_ratio: f64,
    /// Number of full index rebuilds completed.
    #[serde(default)]
    pub rebuild_count: u64,
}

/// Statistics for the storage engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageStats {
    /// Number of SSTable segments.
    pub num_segments: usize,
    /// Total size of all SSTables in bytes.
    pub total_size_bytes: u64,
    /// Total size of memtables in bytes.
    pub memtable_size_bytes: u64,
}

// INVARIANT: Implementor: LsmStorage (memfuse-store/src/lsm.rs)
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

    /// Deletes multiple keys as a single logical batch operation.
    ///
    /// # Performance
    /// Default implementation delegates to sequential `delete()` calls.
    /// Implementors handling large batches (e.g. from `delete_prefix()`)
    /// SHOULD override this with a true batch operation (single lock
    /// acquisition) to avoid per-key lock contention.
    async fn delete_many(&self, tx_id: TxId, keys: Vec<Vec<u8>>) -> Result<u64> {
        let mut deleted = 0u64;
        for key in keys {
            self.delete(tx_id, &key).await?;
            deleted += 1;
        }
        Ok(deleted)
    }

    /// Deletes all key-value pairs whose key starts with `prefix` as part of a transaction.
    ///
    /// Returns the number of keys staged for deletion.
    ///
    /// Default implementation scans all matching keys, then delegates to [`delete_many`][Self::delete_many].
    /// Concrete implementors handling batch mutations should override `delete_many()` or `delete_prefix()`
    /// with a true batch operation to avoid per-key lock overhead.
    async fn delete_prefix(&self, tx_id: TxId, prefix: &[u8]) -> Result<u64> {
        let matching_keys: Vec<Vec<u8>> = self
            .scan_prefix(prefix)
            .await?
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        self.delete_many(tx_id, matching_keys).await
    }

    /// Commits a transaction — makes writes visible.
    async fn commit(&self, tx_id: TxId) -> Result<()>;

    /// Rolls back a transaction — discards staged uncommitted writes for the given ID.
    ///
    /// **Note**: `rollback()` only discards entries currently in the staging buffer.
    /// Once `commit()` has completed, `rollback()` on that `tx_id` is a no-op.
    /// Undoing a physically committed transaction requires a compensating transaction or `rollback_to_tx()`.
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

    /// Scans keys with a prefix, returning only entries visible at or before `seq_no`.
    ///
    /// # Contract
    /// Must respect MVCC snapshot isolation.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"snapshot_read_at"` if snapshot-isolated prefix scan is not implemented.
    /// Tested via `capability_coverage` test module.
    async fn scan_prefix_at(
        &self,
        _prefix: &[u8],
        _seq_no: u64,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "snapshot_read_at",
            "Storage-level snapshot-isolated prefix scan (scan_prefix_at) is not supported by default — implementors must override this method to guarantee MVCC snapshot isolation.",
        ))
    }

    /// Scans a range of keys between `start` and `end` bounds.
    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

// INVARIANT: Implementor: HnswIndex (memfuse-index/src/hnsw.rs)
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

    /// Searches for the k nearest neighbors to a query vector at a specific sequence number.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"snapshot_read_at"` if snapshot-isolated vector search is not implemented.
    /// Tested via `capability_coverage` test module.
    async fn search_at(
        &self,
        _query: &[f32],
        _k: usize,
        _seq_no: u64,
    ) -> Result<Vec<ScoredDocument>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "snapshot_read_at",
            "Vector search snapshot isolation (search_at) is not supported by default — tracked in ADR-024",
        ))
    }

    /// Searches with an optional filter predicate.
    ///
    /// # Default Behaviour
    /// Returns an error if a filter is provided. Implementors **MUST** override
    /// this method if filtered search is supported by their vector engine.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"vector_filtered_search"` if a filter predicate is passed to an engine without filter support.
    /// Tested via `capability_coverage` test module.
    ///
    /// # Note
    /// This default exists solely for backward compatibility.
    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        if filter.is_some() {
            return Err(crate::error::MemFuseError::capability_unsupported(
                "vector_filtered_search",
                "Filtered vector search is not supported by default for this vector engine",
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

    /// Returns true if the index requires a background rebuild (e.g. due to tombstone accumulation).
    fn is_rebuild_required(&self) -> bool {
        false
    }

    /// Triggers an asynchronous background rebuild of the index if supported.
    fn trigger_rebuild_async(&self) {}
}

/// Text embedding engine trait.
#[async_trait]
pub trait TextEmbeddingEngine: Send + Sync + 'static {
    /// Generates an embedding for the given text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generates embeddings for multiple texts.
    /// Default implementation executes sequential calls.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
}

/// Abstract contract for LLM text generation (summarization, importance evaluation, query expansion).
#[async_trait]
pub trait LlmTextGenerator: Send + Sync + 'static {
    /// Generates text for a given prompt using an LLM.
    async fn generate(&self, prompt: &str) -> Result<String>;
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

    /// Searches for documents matching the query at a specific sequence number.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"snapshot_read_at"` if snapshot-isolated text search is not implemented.
    /// Tested via `capability_coverage` test module.
    async fn search_at(
        &self,
        _query: &str,
        _k: usize,
        _seq_no: u64,
    ) -> Result<Vec<ScoredDocument>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "snapshot_read_at",
            "Text search snapshot isolation (search_at) is not supported by default — tracked in ADR-024",
        ))
    }

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

    /// Returns the last transaction ID processed by the index.
    async fn last_tx_id(&self) -> Result<u64>;

    /// Returns the number of documents in the index.
    async fn len(&self) -> usize;

    /// Returns true if the index is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns index statistics.
    async fn stats(&self) -> Result<TextIndexStats>;
}

// INVARIANT: Graph Engine Trait (Signal 3)

/// Graph-Index Trait — CSR-basierte Entity-Relation-Traversal.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
///
/// # TxId-Origin-Invariant (AGT-GRAPH-001)
///
/// **Aufrufer MÜSSEN tx entweder aus der Collection-eigenen next_tx-Sequenz oder aus TxId::INTERNAL_BASE-Offset-Bereich beziehen.**
///
/// **Aufrufer MÜSSEN sicherstellen, dass `tx`-Argumente für [`add_entity`],
/// [`add_edge`] und [`commit`] ausschließlich aus einer der folgenden beiden
/// kanonischen Quellen stammen:**
///
/// 1. **Collection-eigene Sequenz**: Der `next_tx: Arc<AtomicU64>` Zähler in
///    `memfuse-db/src/collection.rs`, der kollisionsfrei aufsteigend inkrementiert
///    wird. Solche TxIds liegen typischerweise im Bereich `[1, ~10^12]`.
///
/// 2. **Interner Systembereich**: `TxId::INTERNAL_BASE` (`u64::MAX - 1_000_000`)
///    aufwärts — reserviert für Checkpoint, WAL-Replay und andere
///    System-Transaktionen (Muster: `memfuse-checkpoint/src/lib.rs:76-79`).
///
/// **Verbotene Quellen:**
/// - Wall-Clock-abgeleitete TxIds (z.B. `SystemTime::now().as_nanos() as u64`
///   ≈ `1.7×10¹⁸`). Diese liegen zufällig zwischen den beiden erlaubten
///   Bereichen und korrumpieren die `rollback_to_tx()`-Kausalordnung: Der Graph
///   "vergisst" nie committed Daten, aber Time-Travel-Wiederherstellung kann
///   die falsche Transaktionsgrenze wählen.
/// - Beliebige fremde IDs ohne Korrelation zur Collection-eigenen Sequenz.
///
/// Implementierungen DÜRFEN bei Verletzung dieses Vertrags eine Warnung loggen
/// (mittels `tracing::warn!`), aber MÜSSEN die Operation nicht hart ablehnen,
/// da der `next_tx`-Höchststand der aufrufenden Collection dem Graph nicht
/// bekannt ist.
///
/// [`add_entity`]: GraphIndex::add_entity
/// [`add_edge`]: GraphIndex::add_edge
/// [`commit`]: GraphIndex::commit
#[async_trait]
pub trait GraphIndex: Send + Sync + 'static {
    /// Traverses the entity graph using BFS up to a maximum number of hops.
    /// Distributes traversing decay weights across related entities.
    async fn traverse(
        &self,
        start_node: crate::types::EntityId,
        max_hops: usize,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>>;

    /// Returns direct (1-hop) neighbor EntityIds for the given entity.
    async fn neighbors(
        &self,
        start_node: crate::types::EntityId,
    ) -> crate::Result<Vec<crate::types::EntityId>> {
        let results = self.traverse(start_node, 1).await?;
        Ok(results.into_iter().map(|(id, _)| id).collect())
    }

    /// Removes an edge between two entities.
    async fn remove_edge(
        &self,
        tx: crate::types::TxId,
        from: crate::types::EntityId,
        to: crate::types::EntityId,
    ) -> crate::Result<()> {
        let _ = (tx, from, to);
        Ok(())
    }

    /// Adds a bidirectional edge between two entities.
    async fn add_bidirectional(
        &self,
        tx: crate::types::TxId,
        from: crate::types::EntityId,
        to: crate::types::EntityId,
        label: &str,
    ) -> crate::Result<()> {
        self.add_edge(tx, crate::types::Edge::new(from, to, label))
            .await?;
        self.add_edge(tx, crate::types::Edge::new(to, from, label))
            .await?;
        Ok(())
    }

    /// Traverses the entity graph starting from multiple anchor entities up to max_hops.
    /// Aggregates decay weights (keeping max score per entity) across anchors.
    async fn multi_traverse(
        &self,
        start_nodes: &[crate::types::EntityId],
        max_hops: usize,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
        let mut combined: std::collections::HashMap<crate::types::EntityId, f32> =
            std::collections::HashMap::new();
        for &start in start_nodes {
            let results = self.traverse(start, max_hops).await?;
            for (entity_id, score) in results {
                combined
                    .entry(entity_id)
                    .and_modify(|s| *s = s.max(score))
                    .or_insert(score);
            }
        }
        let mut results: Vec<(crate::types::EntityId, f32)> = combined.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    /// Traverses the entity graph using BFS up to a maximum number of hops at a specific sequence number.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"graph_traverse_at"` if snapshot-isolated graph traversal is not implemented.
    /// Tested via `capability_coverage` test module.
    async fn traverse_at(
        &self,
        _start_node: crate::types::EntityId,
        _max_hops: usize,
        _seq_no: u64,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "graph_traverse_at",
            "Graph traversal snapshot isolation (traverse_at) is not supported by default — tracked in ADR-024",
        ))
    }

    /// Traverses the entity graph using BFS at a specific point in time (bi-temporal edge filtering).
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"graph_traverse_at_time"` if bi-temporal graph traversal is not implemented.
    /// Tested via `capability_coverage` test module.
    async fn traverse_at_time(
        &self,
        _start_node: crate::types::EntityId,
        _max_hops: usize,
        _as_of: crate::types::TxId,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "graph_traverse_at_time",
            "Bi-temporal graph traversal (traverse_at_time) is not supported by default",
        ))
    }

    /// Traverses the entity graph using BFS with independent system time and business time constraints.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"graph_traverse_at_bitemporal"` if bitemporal graph traversal is not implemented.
    async fn traverse_at_bitemporal(
        &self,
        _start_node: crate::types::EntityId,
        _max_hops: usize,
        _as_of_tx: crate::types::TxId,
        _as_of_business: Option<i64>,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "graph_traverse_at_bitemporal",
            "Bi-temporal graph traversal (traverse_at_bitemporal) is not supported by default",
        ))
    }

    /// Calculates Personalized PageRank (PPR) starting from seed nodes.
    ///
    /// # Convergence Behavior
    /// Power iteration terminates when the L1 norm difference between iterations drops below `config.convergence_epsilon`,
    /// or when `config.max_iterations` is reached. If `config.max_iterations` is reached without full convergence,
    /// the function returns the best-effort intermediate ranking state (no `Err`) and emits a `tracing::warn!` log entry.
    ///
    /// # Errors
    /// Returns [`MemFuseError::CapabilityUnsupported`][crate::MemFuseError::CapabilityUnsupported]
    /// with capability `"graph_ppr"` if Personalized PageRank is not supported by this implementation.
    /// Tested via `capability_coverage` test module.
    async fn personalized_page_rank(
        &self,
        _seed_nodes: &[crate::types::EntityId],
        _config: &crate::types::PprConfig,
    ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
        Err(crate::error::MemFuseError::capability_unsupported(
            "graph_ppr",
            "Personalized PageRank (personalized_page_rank) is not supported by default for this GraphIndex implementation",
        ))
    }

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

    /// Returns the last transaction ID processed by the index.
    async fn last_tx_id(&self) -> crate::Result<u64>;

    /// Returns the number of entities in the index.
    async fn len(&self) -> usize;

    /// Returns true if the index is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

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

/// Report summarizing statistics of a memory lifecycle sweep operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleSweepReport {
    /// Total number of entries evaluated during the sweep.
    pub swept_count: u64,
    /// Number of entries deleted due to time-to-live (TTL) expiration.
    pub deleted_by_ttl: u64,
    /// Number of entries deleted due to importance score recency decay.
    pub deleted_by_decay: u64,
    /// Number of entries skipped because they are pinned or exempt.
    pub skipped_pinned: u64,
}

/// Actions planned during memory consolidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConsolidationAction {
    /// Keep document as is.
    Keep {
        /// ID of the document to keep.
        doc_id: DocId,
    },
    /// Merge two or more documents into a new consolidated entry.
    Merge {
        /// Source document IDs to merge.
        source_ids: Vec<DocId>,
        /// Hint or context summary for the consolidated entry.
        summary_hint: String,
    },
    /// Replace an old document with a new updated entry.
    Supersede {
        /// Old document ID to supersede.
        old_id: DocId,
        /// Replacement document ID.
        new_id: DocId,
    },
    /// Drop a document due to obsolete or low-relevance memory state.
    Drop {
        /// ID of the document to drop.
        doc_id: DocId,
    },
}

/// Trait controlling active Memory Lifecycle management: Decay sweep and Consolidation planning.
///
/// Decouples decision planning (`plan_consolidation`) from execution (`sweep`) for auditability.
#[async_trait]
pub trait MemoryLifecycleManager: Send + Sync {
    /// Performs a decay and TTL sweep.
    /// Returns a report summarizing deleted, retained, and skipped entries.
    async fn sweep(&self, now_tx: TxId) -> Result<LifecycleSweepReport>;

    /// Plans consolidation of similar entries (Mem0 ADD/UPDATE/NOOP pattern).
    /// Returns an action plan without performing automatic execution.
    async fn plan_consolidation(&self, candidates: &[DocId]) -> Result<Vec<ConsolidationAction>>;
}

#[cfg(test)]
mod dyn_safety {
    use super::*;

    fn _assert_dyn_storage(_: Option<&dyn StorageEngine>) {}
    fn _assert_dyn_vector(_: Option<&dyn VectorIndex>) {}
    fn _assert_dyn_text(_: Option<&dyn TextIndex>) {}
    fn _assert_dyn_graph(_: Option<&dyn GraphIndex>) {}
    fn _assert_dyn_embedding(_: Option<&dyn TextEmbeddingEngine>) {}
    fn _assert_dyn_lifecycle(_: Option<&dyn MemoryLifecycleManager>) {}

    #[test]
    fn test_dyn_safety_compiles() {
        _assert_dyn_storage(None);
        _assert_dyn_vector(None);
        _assert_dyn_text(None);
        _assert_dyn_graph(None);
        _assert_dyn_embedding(None);
        _assert_dyn_lifecycle(None);
    }
}

#[cfg(test)]
mod capability_coverage {
    use super::*;

    /// Verifies that calling search_at on a productive VectorIndex instance
    /// does NOT return CapabilityUnsupported.
    #[tokio::test]
    async fn test_hnsw_search_at_capability() {
        struct VectorIndexPlaceholder;
        #[async_trait]
        impl VectorIndex for VectorIndexPlaceholder {
            async fn insert(&self, _: TxId, _: DocId, _: &[f32]) -> Result<()> {
                Ok(())
            }
            async fn search(&self, _: &[f32], _: usize) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn search_at(&self, _: &[f32], _: usize, _: u64) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn delete(&self, _: TxId, _: DocId) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                0
            }
            async fn stats(&self) -> Result<VectorIndexStats> {
                Ok(VectorIndexStats {
                    num_vectors: 0,
                    memory_usage_bytes: 0,
                    num_layers: 0,
                    deleted_ratio: 0.0,
                    rebuild_count: 0,
                })
            }
        }
        let index = VectorIndexPlaceholder;
        let res = index.search_at(&[1.0, 0.0], 5, 1).await;
        assert!(
            !matches!(res, Err(crate::MemFuseError::CapabilityUnsupported { .. })),
            "search_at returned CapabilityUnsupported"
        );
        assert!(!index.is_rebuild_required());
        index.trigger_rebuild_async();
    }

    /// Verifies that calling traverse_at, traverse_at_time, and traverse_at_bitemporal
    /// on a GraphIndex implementation does NOT return CapabilityUnsupported.
    #[tokio::test]
    async fn test_csr_graph_capability() {
        struct GraphIndexPlaceholder;
        #[async_trait]
        impl GraphIndex for GraphIndexPlaceholder {
            async fn traverse(&self, _: EntityId, _: usize) -> Result<Vec<(EntityId, f32)>> {
                Ok(vec![])
            }
            async fn traverse_at(
                &self,
                _: EntityId,
                _: usize,
                _: u64,
            ) -> Result<Vec<(EntityId, f32)>> {
                Ok(vec![])
            }
            async fn traverse_at_time(
                &self,
                _: EntityId,
                _: usize,
                _: TxId,
            ) -> Result<Vec<(EntityId, f32)>> {
                Ok(vec![])
            }
            async fn traverse_at_bitemporal(
                &self,
                _: EntityId,
                _: usize,
                _: TxId,
                _: Option<i64>,
            ) -> Result<Vec<(EntityId, f32)>> {
                Ok(vec![])
            }
            async fn add_entity(&self, _: TxId, _: Entity) -> Result<()> {
                Ok(())
            }
            async fn add_edge(&self, _: TxId, _: Edge) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                0
            }
            async fn stats(&self) -> Result<GraphIndexStats> {
                Ok(GraphIndexStats {
                    num_entities: 0,
                    num_edges: 0,
                    memory_usage_bytes: 0,
                })
            }
        }
        let graph = GraphIndexPlaceholder;
        let res_traverse_at = graph.traverse_at(EntityId::new(1), 2, 1).await;
        assert!(
            !matches!(
                res_traverse_at,
                Err(crate::MemFuseError::CapabilityUnsupported { .. })
            ),
            "traverse_at returned CapabilityUnsupported"
        );

        let res_traverse_at_time = graph
            .traverse_at_time(EntityId::new(1), 2, TxId::new(1))
            .await;
        assert!(
            !matches!(
                res_traverse_at_time,
                Err(crate::MemFuseError::CapabilityUnsupported { .. })
            ),
            "traverse_at_time returned CapabilityUnsupported"
        );

        let res_traverse_at_bitemporal = graph
            .traverse_at_bitemporal(EntityId::new(1), 2, TxId::new(1), Some(1000))
            .await;
        assert!(
            !matches!(
                res_traverse_at_bitemporal,
                Err(crate::MemFuseError::CapabilityUnsupported { .. })
            ),
            "traverse_at_bitemporal returned CapabilityUnsupported"
        );
    }

    /// Verifies that calling search_at on a TextIndex implementation does NOT return CapabilityUnsupported.
    #[tokio::test]
    async fn test_text_index_search_at_capability() {
        struct TextIndexPlaceholder;
        #[async_trait]
        impl TextIndex for TextIndexPlaceholder {
            async fn search(&self, _: &str, _: usize) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn search_at(&self, _: &str, _: usize, _: u64) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn insert(&self, _: TxId, _: DocId, _: &str) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _: TxId, _: DocId) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                0
            }
            async fn stats(&self) -> Result<TextIndexStats> {
                Ok(TextIndexStats {
                    num_documents: 0,
                    num_tokens: 0,
                    memory_usage_bytes: 0,
                })
            }
        }
        let text_index = TextIndexPlaceholder;
        let res = text_index.search_at("test", 5, 1).await;
        assert!(
            !matches!(res, Err(crate::MemFuseError::CapabilityUnsupported { .. })),
            "search_at returned CapabilityUnsupported"
        );
    }

    /// Verifies that calling default scan_prefix_at on a StorageEngine implementation
    /// returns CapabilityUnsupported with capability "snapshot_read_at".
    #[tokio::test]
    async fn test_storage_scan_prefix_at_capability() {
        struct StorageEnginePlaceholder;
        #[async_trait]
        impl StorageEngine for StorageEnginePlaceholder {
            async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put(&self, _: TxId, _: &[u8], _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn flush(&self) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<StorageStats> {
                Ok(StorageStats {
                    num_segments: 0,
                    total_size_bytes: 0,
                    memtable_size_bytes: 0,
                })
            }
            async fn last_seq_no(&self) -> Result<u64> {
                Ok(0)
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                Ok(TxId(0))
            }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
            async fn scan(
                &self,
                _: std::ops::Bound<&[u8]>,
                _: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
        }

        let placeholder = StorageEnginePlaceholder;
        let result = placeholder.scan_prefix_at(b"prefix", 0).await;
        assert!(matches!(
            result,
            Err(crate::MemFuseError::CapabilityUnsupported { ref capability, .. }) if capability == "snapshot_read_at"
        ));
    }
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
            deleted_ratio: 0.1,
            rebuild_count: 1,
        };
        let ser = serde_json::to_string(&v_stats).unwrap(); // unwrap
        let deser: VectorIndexStats = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(v_stats.num_vectors, deser.num_vectors);

        let s_stats = StorageStats {
            num_segments: 2,
            total_size_bytes: 2048,
            memtable_size_bytes: 512,
        };
        let ser = serde_json::to_string(&s_stats).unwrap(); // unwrap
        let deser: StorageStats = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(s_stats.total_size_bytes, deser.total_size_bytes);

        let t_stats = TextIndexStats {
            num_documents: 10,
            num_tokens: 1000,
            memory_usage_bytes: 256,
        };
        let ser = serde_json::to_string(&t_stats).unwrap(); // unwrap
        let deser: TextIndexStats = serde_json::from_str(&ser).unwrap(); // unwrap
        assert_eq!(t_stats.num_documents, deser.num_documents);
    }

    #[tokio::test]
    async fn test_storage_engine_default_put_batch() {
        type KVPair = (Vec<u8>, Vec<u8>);
        type Log = std::sync::Arc<std::sync::Mutex<Vec<KVPair>>>;
        struct MockStorage(Log);
        #[async_trait::async_trait]
        impl StorageEngine for MockStorage {
            async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put(&self, _: TxId, key: &[u8], value: &[u8]) -> Result<()> {
                self.0.lock().unwrap().push((key.to_vec(), value.to_vec())); // unwrap
                Ok(())
            }
            async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn flush(&self) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<StorageStats> {
                Ok(StorageStats {
                    num_segments: 0,
                    total_size_bytes: 0,
                    memtable_size_bytes: 0,
                })
            }
            async fn last_seq_no(&self) -> Result<u64> {
                Ok(0)
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                Ok(TxId(0))
            }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
            async fn scan(
                &self,
                _: std::ops::Bound<&[u8]>,
                _: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
        }

        let store = MockStorage(std::sync::Arc::new(std::sync::Mutex::new(vec![])));
        let entries = vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ];
        store.put_batch(TxId(1), &entries).await.unwrap(); // unwrap
        assert_eq!(store.0.lock().unwrap().len(), 2); // unwrap

        // Test scan_prefix_at default error
        let res = store.scan_prefix_at(b"pre", 1).await;
        match res {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, .. }) => {
                assert_eq!(capability, "snapshot_read_at");
            }
            _ => panic!("Expected CapabilityUnsupported for scan_prefix_at"),
        }
    }

    #[tokio::test]
    async fn test_delete_many_default_impl_deletes_all_keys() {
        struct MockStorage {
            data: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>>,
            delete_call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl StorageEngine for MockStorage {
            async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(self.data.lock().unwrap().get(key).cloned()) // unwrap
            }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put(&self, _: TxId, key: &[u8], value: &[u8]) -> Result<()> {
                self.data
                    .lock()
                    .unwrap() // unwrap
                    .insert(key.to_vec(), value.to_vec());
                Ok(())
            }
            async fn delete(&self, _: TxId, key: &[u8]) -> Result<()> {
                self.delete_call_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.data.lock().unwrap().remove(key); // unwrap
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn flush(&self) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<StorageStats> {
                Ok(StorageStats {
                    num_segments: 0,
                    total_size_bytes: 0,
                    memtable_size_bytes: 0,
                })
            }
            async fn last_seq_no(&self) -> Result<u64> {
                Ok(0)
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                Ok(TxId(0))
            }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                let map = self.data.lock().unwrap(); // unwrap
                let mut res = Vec::new();
                for (k, v) in map.iter() {
                    if k.starts_with(prefix) {
                        res.push((k.clone(), v.clone()));
                    }
                }
                Ok(res)
            }
            async fn scan(
                &self,
                _: std::ops::Bound<&[u8]>,
                _: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
        }

        let map = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let store = MockStorage {
            data: map.clone(),
            delete_call_count: count.clone(),
        };

        store.put(TxId(1), b"pref:1", b"v1").await.unwrap(); // unwrap
        store.put(TxId(1), b"pref:2", b"v2").await.unwrap(); // unwrap
        store.put(TxId(1), b"other:1", b"v3").await.unwrap(); // unwrap

        let deleted = store.delete_prefix(TxId(2), b"pref:").await.unwrap(); // unwrap
        assert_eq!(deleted, 2);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(store.get(b"pref:1").await.unwrap().is_none()); // unwrap
        assert!(store.get(b"pref:2").await.unwrap().is_none()); // unwrap
        assert_eq!(store.get(b"other:1").await.unwrap().unwrap(), b"v3"); // unwrap
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
            async fn search(&self, _: &[f32], _: usize) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn delete(&self, _: TxId, _: DocId) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                self.0.load(std::sync::atomic::Ordering::SeqCst)
            }
            async fn stats(&self) -> Result<VectorIndexStats> {
                Ok(VectorIndexStats {
                    num_vectors: 0,
                    memory_usage_bytes: 0,
                    num_layers: 0,
                    deleted_ratio: 0.0,
                    rebuild_count: 0,
                })
            }
        }

        let index = MockIndex(std::sync::atomic::AtomicUsize::new(0));
        assert!(index.is_empty().await);

        let vectors = vec![
            (DocId(1), [1.0, 2.0].as_slice()),
            (DocId(2), [3.0, 4.0].as_slice()),
        ];
        index.insert_batch(TxId(1), &vectors).await.unwrap(); // unwrap
        assert_eq!(index.len().await, 2);
        assert!(!index.is_empty().await);

        // Test search_filtered default error
        let res = index.search_filtered(&[1.0], 1, Some(&|_| true)).await;
        match res {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, .. }) => {
                assert_eq!(capability, "vector_filtered_search");
            }
            _ => panic!("Expected CapabilityUnsupported for search_filtered"),
        }

        // Test search_at default error
        let res2 = index.search_at(&[1.0], 1, 42).await;
        match res2 {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, reason }) => {
                assert_eq!(capability, "snapshot_read_at");
                assert!(reason.contains("ADR-024"), "Unexpected reason: {reason}");
            }
            _ => panic!("Expected CapabilityUnsupported with ADR-024 for search_at"),
        }
    }

    #[tokio::test]
    async fn test_text_index_defaults() {
        struct MockTextIndex;
        #[async_trait::async_trait]
        impl TextIndex for MockTextIndex {
            async fn search(&self, _: &str, _: usize) -> Result<Vec<ScoredDocument>> {
                Ok(vec![])
            }
            async fn insert(&self, _: TxId, _: DocId, _: &str) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _: TxId, _: DocId) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                0
            }
            async fn stats(&self) -> Result<TextIndexStats> {
                Ok(TextIndexStats {
                    num_documents: 0,
                    num_tokens: 0,
                    memory_usage_bytes: 0,
                })
            }
        }

        let index = MockTextIndex;
        let res = index.search_at("query", 10, 42).await;
        match res {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, reason }) => {
                assert_eq!(capability, "snapshot_read_at");
                assert!(reason.contains("ADR-024"), "Unexpected reason: {reason}");
            }
            _ => panic!("Expected CapabilityUnsupported for search_at"),
        }
    }

    #[tokio::test]
    async fn test_graph_index_defaults() {
        struct MockGraphIndex;
        #[async_trait::async_trait]
        impl GraphIndex for MockGraphIndex {
            async fn traverse(
                &self,
                _: crate::types::EntityId,
                _: usize,
            ) -> crate::Result<Vec<(crate::types::EntityId, f32)>> {
                Ok(vec![])
            }
            async fn add_entity(
                &self,
                _: crate::types::TxId,
                _: crate::types::Entity,
            ) -> crate::Result<()> {
                Ok(())
            }
            async fn add_edge(
                &self,
                _: crate::types::TxId,
                _: crate::types::Edge,
            ) -> crate::Result<()> {
                Ok(())
            }
            async fn commit(&self, _: crate::types::TxId) -> crate::Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: crate::types::TxId) -> crate::Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: crate::types::TxId) -> crate::Result<()> {
                Ok(())
            }
            async fn last_tx_id(&self) -> crate::Result<u64> {
                Ok(0)
            }
            async fn len(&self) -> usize {
                0
            }
            async fn stats(&self) -> crate::Result<GraphIndexStats> {
                Ok(GraphIndexStats {
                    num_entities: 0,
                    num_edges: 0,
                    memory_usage_bytes: 0,
                })
            }
        }

        let index = MockGraphIndex;
        let res = index
            .traverse_at(crate::types::EntityId::new(1), 2, 42)
            .await;
        match res {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, reason }) => {
                assert_eq!(capability, "graph_traverse_at");
                assert!(reason.contains("ADR-024"), "Unexpected reason: {reason}");
            }
            _ => panic!("Expected CapabilityUnsupported with ADR-024"),
        }

        let res_time = index
            .traverse_at_time(
                crate::types::EntityId::new(1),
                2,
                crate::types::TxId::new(10),
            )
            .await;
        match res_time {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, .. }) => {
                assert_eq!(capability, "graph_traverse_at_time");
            }
            _ => panic!("Expected CapabilityUnsupported for traverse_at_time"),
        }

        let res_ppr = index
            .personalized_page_rank(
                &[crate::types::EntityId::new(1)],
                &crate::types::PprConfig::default(),
            )
            .await;
        match res_ppr {
            Err(crate::error::MemFuseError::CapabilityUnsupported { capability, .. }) => {
                assert_eq!(capability, "graph_ppr");
            }
            _ => panic!("Expected CapabilityUnsupported for personalized_page_rank"),
        }
    }
}
