//! Transactional orchestration for atomic multi-index commits.
// ANCHOR:DOC:DOC-TRANSACTION-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
use crate::Collection;
use memfuse_core::{DocId, MemFuseError, Result, StorageEngine, TxId, VectorIndex};
use std::sync::Mutex;

// ANCHOR:ARCH:DB-TX-001 — Atomic Multi-Index Commit Orchestrierung.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// MECHANISMUS: 2-Phase Commit zwischen LSM-Store und HNSW-Index.
// ROLLBACK: Führt Compensating Transactions für den LSM-Store aus, falls HNSW failt.
//
// ANCHOR:GREEN:WP-1.2-TX-001 — Isolation-Tests für DbTransaction::rollback unter Contention.
// WP:WP-1.2 PRIO:2 NEEDS:NONE
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
/// A transaction wrapper that ensures atomic multi-index commits across LSM-Store and HNSW-Index.
pub struct DbTransaction<'a> {
    pub tx_id: TxId,
    collection: &'a Collection,
    staged_forward_keys: Mutex<Vec<Vec<u8>>>,
    staged_reverse_keys: Mutex<Vec<Vec<u8>>>,
    staged_doc_ids: Mutex<Vec<DocId>>,
}

impl<'a> DbTransaction<'a> {
    pub fn new(collection: &'a Collection, tx_id: TxId) -> Self {
        Self {
            tx_id,
            collection,
            staged_forward_keys: Mutex::new(Vec::new()),
            staged_reverse_keys: Mutex::new(Vec::new()),
            staged_doc_ids: Mutex::new(Vec::new()),
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

    /// Commits the transaction atomically across both LSM and HNSW.
    pub async fn commit(self) -> Result<()> {
        let intent_key = self
            .collection
            .namespaced_key(&self.tx_id.inner().to_le_bytes(), 3);

        // 1. Prepare phase: Write intent marker
        self.collection
            .storage
            .put(self.tx_id, &intent_key, b"pending")
            .await?;

        // 2. Commit Storage (LSM)
        if let Err(storage_err) = self.collection.storage.commit(self.tx_id).await {
            // Roll back the index since storage failed
            let _ = self.collection.index.rollback(self.tx_id).await;
            return Err(MemFuseError::Transaction(storage_err.to_string()));
        }

        // 3. Commit Index (HNSW)
        if let Err(index_err) = self.collection.index.commit(self.tx_id).await {
            // Compensating transaction to rollback the LSM Storage
            let rollback_tx = TxId::new(
                self.collection
                    .next_tx
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            );

            let f_keys = {
                let mut guard = match self.staged_forward_keys.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                std::mem::take(&mut *guard)
            };
            for f_key in f_keys {
                let _ = self.collection.storage.delete(rollback_tx, &f_key).await;
            }

            let r_keys = {
                let mut guard = match self.staged_reverse_keys.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                std::mem::take(&mut *guard)
            };
            for r_key in r_keys {
                let _ = self.collection.storage.delete(rollback_tx, &r_key).await;
            }

            let _ = self
                .collection
                .storage
                .put(rollback_tx, &intent_key, b"aborted")
                .await;
            let _ = self.collection.storage.commit(rollback_tx).await;

            return Err(MemFuseError::Transaction(index_err.to_string()));
        }

        // 4. Finalize / Cleanup
        let cleanup_tx = TxId::new(
            self.collection
                .next_tx
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        let _ = self
            .collection
            .storage
            .put(cleanup_tx, &intent_key, b"committed")
            .await;
        let _ = self.collection.storage.commit(cleanup_tx).await;

        Ok(())
    }

    /// Rolls back any changes entirely from memory before commit is called.
    pub async fn rollback(self) -> Result<()> {
        self.collection.storage.rollback(self.tx_id).await?;
        self.collection.index.rollback(self.tx_id).await?;
        Ok(())
    }
}
