// FILE-CONTEXT
// ZWECK: Orchestrierung atomarer 4-Index 2-Phase-Commits und kompensierender Transaktionen.
// INVARIANTEN: [INV-DB-3] Keine verschluckten Fehler bei Rollbacks; Kompensierende Transaktionen bei HNSW/BM25/Graph Ausfällen.
// NICHT-OFFENSICHTLICH: Multi-Attempt LSM-Kompensation mit Split-Brain Tracing-Warnungen bei anhaltenden Fehlern.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

//! # Database Transactions
//!
//! This module provides `DbTransaction`, an orchestrator for atomic multi-index commits
//! between LSM-Tree storage engine (`memfuse-store`), HNSW vector index (`memfuse-index`),
//! BM25 inverted text index (`memfuse-text`), and CSR graph index (`memfuse-graph`).
//! It implements a 4-index 2-phase commit protocol and provides compensating transactions for rollbacks.
//!
//! # Safety & Reliability Invariants
//! - **[INV-DB-3] Strict Error Visibility in Rollbacks**: Compensating transactions during
//!   rollback must never silently drop errors. Discovered during Forensic Audit (HARD-004),
//!   any rollback failure must log explicitly to `tracing::error!` mapping out a potential Split-Brain.
//!
//! # Lock Hierarchy & Poison Recovery
//! `DbTransaction` uses fine-grained `std::sync::Mutex` instances for staging index changes.
//! Lock ordering between staged locks is not strict because staged fields are modified sequentially per operation.
//! All Mutex lock acquisitions explicitly handle `PoisonError` via `match` with `p.into_inner()`
//! to ensure fail-safe operation without panics.

use crate::Collection;
use memfuse_core::{
    DocId, Edge, Entity, EntityId, GraphIndex, MemFuseError, Result, StorageEngine, TextIndex,
    TxId, VectorIndex,
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Status of a multi-index transaction during the 2-phase commit.
#[derive(Debug, Serialize, Deserialize)]
pub enum CommitIntent {
    /// Transaction is in the "Prepared" state. Stored DocIds assist recovery.
    Pending {
        doc_ids: Vec<DocId>,
        #[serde(default)]
        has_text: bool,
        #[serde(default)]
        has_graph: bool,
    },
    /// Transaction is committed across all indices.
    Committed,
    /// Transaction was aborted and compensated.
    Aborted,
}

/// A transaction wrapper that ensures atomic multi-index commits across LSM-Store, HNSW-Index, Text-Index, and Graph-Index.
pub struct DbTransaction<S: StorageEngine, V: VectorIndex = memfuse_index::HnswIndex> {
    pub tx_id: TxId,
    collection: Collection<S, V>,
    staged_forward_keys: Mutex<Vec<Vec<u8>>>,
    staged_reverse_keys: Mutex<Vec<Vec<u8>>>,
    staged_doc_ids: Mutex<Vec<DocId>>,
    staged_text_ops: Mutex<Vec<(DocId, String)>>,
    staged_text_deletes: Mutex<Vec<DocId>>,
    staged_graph_entities: Mutex<Vec<Entity>>,
    staged_graph_edges: Mutex<Vec<Edge>>,
    staged_graph_entity_deletes: Mutex<Vec<EntityId>>,
    staged_graph_edge_deletes: Mutex<Vec<(EntityId, EntityId)>>,
}

impl<S: StorageEngine, V: VectorIndex> DbTransaction<S, V> {
    pub fn new(collection: Collection<S, V>, tx_id: TxId) -> Self {
        Self {
            tx_id,
            collection,
            staged_forward_keys: Mutex::new(Vec::with_capacity(16)),
            staged_reverse_keys: Mutex::new(Vec::with_capacity(16)),
            staged_doc_ids: Mutex::new(Vec::with_capacity(16)),
            staged_text_ops: Mutex::new(Vec::with_capacity(16)),
            staged_text_deletes: Mutex::new(Vec::with_capacity(16)),
            staged_graph_entities: Mutex::new(Vec::with_capacity(16)),
            staged_graph_edges: Mutex::new(Vec::with_capacity(16)),
            staged_graph_entity_deletes: Mutex::new(Vec::with_capacity(16)),
            staged_graph_edge_deletes: Mutex::new(Vec::with_capacity(16)),
        }
    }

    /// Records keys and IDs that have been operated on for potential compensating rollback.
    pub fn record_keys(&self, forward: Vec<u8>, reverse: Vec<u8>, doc_id: DocId) {
        let mut fw = match self.staged_forward_keys.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        fw.push(forward);

        let mut rev = match self.staged_reverse_keys.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        rev.push(reverse);

        let mut ids = match self.staged_doc_ids.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        ids.push(doc_id);
    }

    pub fn stage_text_insert(&self, doc_id: DocId, text: String) {
        let mut guard = match self.staged_text_ops.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push((doc_id, text));
    }

    pub fn stage_text_delete(&self, doc_id: DocId) {
        let mut guard = match self.staged_text_deletes.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push(doc_id);
    }

    pub fn stage_graph_entity(&self, entity: Entity) {
        let mut guard = match self.staged_graph_entities.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push(entity);
    }

    pub fn stage_graph_edge(&self, edge: Edge) {
        let mut guard = match self.staged_graph_edges.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push(edge);
    }

    pub fn stage_graph_entity_delete(&self, entity_id: EntityId) {
        let mut guard = match self.staged_graph_entity_deletes.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push(entity_id);
    }

    pub fn stage_graph_edge_delete(&self, from: EntityId, to: EntityId) {
        let mut guard = match self.staged_graph_edge_deletes.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push((from, to));
    }

    async fn commit_text_staged(&self) -> Result<()> {
        let text_deletes = {
            let mut guard = match self.staged_text_deletes.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };
        let text_ops = {
            let mut guard = match self.staged_text_ops.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };

        for doc_id in text_deletes {
            self.collection
                .text_index
                .delete_document(self.tx_id, doc_id)
                .await?;
        }

        for (doc_id, text) in text_ops {
            self.collection
                .text_index
                .upsert_document(self.tx_id, doc_id, &text)
                .await?;
        }

        Ok(())
    }

    async fn commit_graph_staged(&self) -> Result<()> {
        let entities = {
            let mut guard = match self.staged_graph_entities.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };
        let edges = {
            let mut guard = match self.staged_graph_edges.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };

        for entity in entities {
            self.collection
                .graph_index
                .add_entity(self.tx_id, entity)
                .await?;
        }

        for edge in edges {
            self.collection
                .graph_index
                .add_edge(self.tx_id, edge)
                .await?;
        }

        let edge_deletes = {
            let mut guard = match self.staged_graph_edge_deletes.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };

        for (from, to) in edge_deletes {
            self.collection
                .graph_index
                .remove_edge(self.tx_id, from, to)
                .await?;
        }

        Ok(())
    }

    /// Commits the transaction atomically across all 4 indices (LSM, HNSW, BM25, CSR).
    ///
    /// Sequence:
    /// PHASE 1 - PREPARE:
    ///   a) Write CommitIntent (Pending) mit allen staged_doc_ids, has_text, has_graph → LSM
    ///
    /// PHASE 2 - COMMIT (reihenfolge ist kritisch!):
    ///   b) storage.commit(tx_id)                     ← LSM (WAL + MemTable)
    ///   c) index.commit(tx_id)                       ← HNSW
    ///   d) text_index.commit(tx_id)                  ← BM25
    ///   e) graph_index.commit(tx_id)                 ← CSR
    ///
    /// PHASE 3 - CLEANUP:
    ///   f) CommitIntent (Committed) löschen / schreiben → LSM
    pub async fn commit(self) -> Result<()> {
        let intent_key = self
            .collection
            .namespaced_key(&self.tx_id.inner().to_le_bytes(), 3);

        let doc_ids = {
            let guard = match self.staged_doc_ids.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.clone()
        };

        let has_text = {
            let ops = match self.staged_text_ops.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let dels = match self.staged_text_deletes.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            !ops.is_empty() || !dels.is_empty()
        };

        let has_graph = {
            let ents = match self.staged_graph_entities.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let edgs = match self.staged_graph_edges.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let e_dels = match self.staged_graph_edge_deletes.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            !ents.is_empty() || !edgs.is_empty() || !e_dels.is_empty()
        };

        // Execute staged text and graph staging before prepare/commit
        if let Err(e) = self.commit_text_staged().await {
            self.rollback_internal().await;
            return Err(e);
        }

        if let Err(e) = self.commit_graph_staged().await {
            self.rollback_internal().await;
            return Err(e);
        }

        // 1. Prepare phase: Write intent marker with staged IDs
        let intent = CommitIntent::Pending {
            doc_ids: doc_ids.clone(),
            has_text,
            has_graph,
        };
        let intent_bytes = serde_json::to_vec(&intent).map_err(|e| {
            MemFuseError::Transaction(format!("Failed to serialize commit intent: {}", e))
        })?;

        self.collection
            .storage
            .put(self.tx_id, &intent_key, &intent_bytes)
            .await?;

        // 2. Commit Storage (LSM)
        if let Err(storage_err) = self.collection.storage.commit(self.tx_id).await {
            self.rollback_internal().await;
            return Err(MemFuseError::Transaction(storage_err.to_string()));
        }

        // 3. Commit Index (HNSW)
        if let Err(index_err) = self.collection.index.commit(self.tx_id).await {
            if let Err(e) = self.collection.graph_index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback graph_index after HNSW commit failure: {}",
                    e
                );
            }
            if let Err(e) = self.collection.text_index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback text_index after HNSW commit failure: {}",
                    e
                );
            }
            self.compensate_lsm(&intent_key, &doc_ids).await;
            return Err(MemFuseError::Transaction(format!(
                "HNSW index commit failed, storage rolled back via compensating tx. Error: {}",
                index_err
            )));
        }

        // 4. Commit Text Index (BM25)
        if let Err(text_err) = self.collection.text_index.commit(self.tx_id).await {
            if let Err(e) = self.collection.graph_index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback graph_index after text commit failure: {}",
                    e
                );
            }
            if let Err(e) = self.collection.text_index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback text_index after text commit failure: {}",
                    e
                );
            }
            // HNSW is already committed -> compensate HNSW vector deletions
            self.compensate_hnsw(&doc_ids).await;
            self.compensate_lsm(&intent_key, &doc_ids).await;
            return Err(MemFuseError::Transaction(format!(
                "Text index commit failed, storage & HNSW rolled back via compensating tx. Error: {}",
                text_err
            )));
        }

        // 5. Commit Graph Index (CSR)
        if let Err(graph_err) = self.collection.graph_index.commit(self.tx_id).await {
            if let Err(e) = self.collection.graph_index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback graph_index after graph commit failure: {}",
                    e
                );
            }
            self.compensate_text(&doc_ids).await;
            self.compensate_hnsw(&doc_ids).await;
            self.compensate_lsm(&intent_key, &doc_ids).await;
            return Err(MemFuseError::Transaction(format!(
                "Graph index commit failed, storage, HNSW & text index rolled back via compensating tx. Error: {}",
                graph_err
            )));
        }

        // 6. Finalize / Cleanup
        let cleanup_tx = TxId::new(
            self.collection
                .next_tx
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        let commit_bytes = match serde_json::to_vec(&CommitIntent::Committed) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to serialize CommitIntent::Committed: {}", e);
                b"{}".to_vec()
            }
        };
        if let Err(e) = self
            .collection
            .storage
            .put(cleanup_tx, &intent_key, &commit_bytes)
            .await
        {
            tracing::warn!("Failed to write committed intent marker: {}", e);
        }

        if let Err(e) = self.collection.storage.commit(cleanup_tx).await {
            tracing::warn!("Failed to commit cleanup transaction: {}", e);
        }

        Ok(())
    }

    async fn compensate_hnsw(&self, doc_ids: &[DocId]) {
        let comp_tx = TxId::new(
            self.collection
                .next_tx
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        for &doc_id in doc_ids {
            if let Err(e) = self.collection.index.delete(comp_tx, doc_id).await {
                tracing::error!(
                    "[INV-DB-3] Compensating HNSW delete failed for doc_id {:?}: {}",
                    doc_id,
                    e
                );
            }
        }
        if let Err(e) = self.collection.index.commit(comp_tx).await {
            tracing::error!("[INV-DB-3] Compensating HNSW commit failed: {}", e);
        }
    }

    async fn compensate_text(&self, doc_ids: &[DocId]) {
        let comp_tx = TxId::new(
            self.collection
                .next_tx
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        for &doc_id in doc_ids {
            if let Err(e) = self
                .collection
                .text_index
                .delete_document(comp_tx, doc_id)
                .await
            {
                tracing::error!(
                    "[INV-DB-3] Compensating text delete failed for doc_id {:?}: {}",
                    doc_id,
                    e
                );
            }
        }
        if let Err(e) = self.collection.text_index.commit(comp_tx).await {
            tracing::error!("[INV-DB-3] Compensating text commit failed: {}", e);
        }
    }

    async fn compensate_lsm(&self, intent_key: &[u8], doc_ids: &[DocId]) {
        let f_keys = {
            let mut guard = match self.staged_forward_keys.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };
        let r_keys = {
            let mut guard = match self.staged_reverse_keys.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            std::mem::take(&mut *guard)
        };

        let mut success = false;
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts && !success {
            attempts += 1;
            let rollback_tx = TxId::new(
                self.collection
                    .next_tx
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            );

            let mut comp_failed = false;

            for f_key in &f_keys {
                if let Err(e) = self.collection.storage.delete(rollback_tx, f_key).await {
                    tracing::error!("[INV-DB-3] Compensating delete failed (forward): {}", e);
                    comp_failed = true;
                }
            }

            for r_key in &r_keys {
                if let Err(e) = self.collection.storage.delete(rollback_tx, r_key).await {
                    tracing::error!("[INV-DB-3] Compensating delete failed (reverse): {}", e);
                    comp_failed = true;
                }
            }

            let abort_bytes = serde_json::to_vec(&CommitIntent::Aborted).unwrap_or_default();
            if let Err(e) = self
                .collection
                .storage
                .put(rollback_tx, intent_key, &abort_bytes)
                .await
            {
                tracing::error!("[INV-DB-3] Failed to write aborted intent marker: {}", e);
                comp_failed = true;
            }

            if let Err(e) = self.collection.storage.commit(rollback_tx).await {
                tracing::error!("[INV-DB-3] Compensating commit failed: {}", e);
                comp_failed = true;
            }

            if !comp_failed {
                success = true;
                tracing::info!(
                    "[INV-DB-3] Compensating transaction succeeded on attempt {}",
                    attempts
                );
            } else if attempts < max_attempts {
                tracing::warn!(
                    "[INV-DB-3] Compensating transaction attempt {} failed. Retrying in 100ms...",
                    attempts
                );
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        if !success {
            tracing::error!(
                target: "memfuse.invariant",
                event = "split_brain",
                doc_ids = ?doc_ids,
                "[INV-DB-3] FATAL: Compensating transaction failed after {} attempts. \
                 Index DB potential split-brain detected! Repair-on-Open required.",
                max_attempts
            );
        }
    }

    async fn rollback_internal(&self) {
        if let Err(e) = self.collection.graph_index.rollback(self.tx_id).await {
            tracing::error!("[INV-DB-3] Graph index rollback failed: {}", e);
        }
        if let Err(e) = self.collection.text_index.rollback(self.tx_id).await {
            tracing::error!("[INV-DB-3] Text index rollback failed: {}", e);
        }
        if let Err(e) = self.collection.index.rollback(self.tx_id).await {
            tracing::error!("[INV-DB-3] Vector index rollback failed: {}", e);
        }
        if let Err(e) = self.collection.storage.rollback(self.tx_id).await {
            tracing::error!("[INV-DB-3] Storage rollback failed: {}", e);
        }
    }

    /// Rolls back any uncommitted changes applied to all 4 sub-systems in reverse commit order.
    pub async fn rollback(self) -> Result<()> {
        let graph_res = self.collection.graph_index.rollback(self.tx_id).await;
        let text_res = self.collection.text_index.rollback(self.tx_id).await;
        let index_res = self.collection.index.rollback(self.tx_id).await;
        let storage_res = self.collection.storage.rollback(self.tx_id).await;

        if let Err(ref e) = graph_res {
            tracing::error!("[INV-DB-3] Graph index rollback failed: {}", e);
        }
        if let Err(ref e) = text_res {
            tracing::error!("[INV-DB-3] Text index rollback failed: {}", e);
        }
        if let Err(ref e) = index_res {
            tracing::error!("[INV-DB-3] Vector index rollback failed: {}", e);
        }
        if let Err(ref e) = storage_res {
            tracing::error!("[INV-DB-3] Storage rollback failed: {}", e);
        }

        graph_res?;
        text_res?;
        index_res?;
        storage_res?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn create_test_collection() -> Collection<LsmStorage, HnswIndex> {
        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        Collection::new(
            "tx_test".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        )
    }

    #[tokio::test]
    async fn test_db_transaction_staging_and_commit() {
        use memfuse_core::{Edge, Entity, EntityId};

        let col = create_test_collection().await;
        let tx_id = col.allocate_tx().unwrap();
        let tx = DbTransaction::new(col.clone(), tx_id);

        let doc_id = DocId::new(100);
        tx.record_keys(vec![1, 2, 3], vec![3, 2, 1], doc_id);
        tx.stage_text_insert(doc_id, "hello transaction world".to_string());
        tx.stage_graph_entity(Entity::new(EntityId(1), "NodeA", "Concept"));
        tx.stage_graph_edge(Edge::new(EntityId(1), EntityId(2), "relates_to"));

        // Commit should succeed without errors
        let commit_res = tx.commit().await;
        assert!(commit_res.is_ok());
    }

    #[tokio::test]
    async fn test_db_transaction_staging_and_rollback() {
        use memfuse_core::EntityId;

        let col = create_test_collection().await;
        let tx_id = col.allocate_tx().unwrap();
        let tx = DbTransaction::new(col.clone(), tx_id);

        let doc_id = DocId::new(200);
        tx.stage_text_insert(doc_id, "staged text for rollback".to_string());
        tx.stage_text_delete(doc_id);
        tx.stage_graph_entity_delete(EntityId(10));
        tx.stage_graph_edge_delete(EntityId(10), EntityId(20));

        let rollback_res = tx.rollback().await;
        assert!(rollback_res.is_ok());
    }
}
