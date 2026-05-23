//! # Database Transactions
//!
//! This module provides `DbTransaction`, an orchestrator for atomic multi-index commits
//! between the LSM-Tree storage engine (`memfuse-store`) and the HNSW vector index (`memfuse-index`).
//! It implements a 2-phase commit protocol and provides compensating transactions for rollbacks.
//!
//! # Safety & Reliability Invariants
//! - **[INV-DB-3] Strict Error Visibility in Rollbacks**: Compensating transactions during
//!   rollback must never silently drop errors. Discovered during Forensic Audit (HARD-004),
//!   any rollback failure must log explicitly to `tracing::error!` mapping out a potential Split-Brain.

use crate::Collection;
use memfuse_core::{DocId, MemFuseError, Result, StorageEngine, TxId, VectorIndex};
use std::sync::Mutex;

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
            staged_forward_keys: Mutex::new(Vec::with_capacity(16)),
            staged_reverse_keys: Mutex::new(Vec::with_capacity(16)),
            staged_doc_ids: Mutex::new(Vec::with_capacity(16)),
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
    ///
    /// Follows a 3-step sequence:
    /// 1. Write Intent WAL entry to LSM.
    /// 2. Commit Storage (LSM).
    /// 3. Commit Index (HNSW).
    ///
    /// If 3 fails, it performs a compensating transaction on the LSM store.
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
            if let Err(e) = self.collection.index.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback index during transaction abort. \
                     Index DB split-brain possible! Error: {}",
                    e
                );
            }
            // Also explicitly rollback storage in-memory state
            if let Err(e) = self.collection.storage.rollback(self.tx_id).await {
                tracing::error!(
                    "[INV-DB-3] CRITICAL: Failed to rollback storage in-memory state after commit failure. \
                     Error: {}",
                    e
                );
            }
            return Err(MemFuseError::Transaction(storage_err.to_string()));
        }

        // 3. Commit Index (HNSW)
        if let Err(index_err) = self.collection.index.commit(self.tx_id).await {
            // Compensating transaction to rollback the LSM Storage since it's already committed
            // Implemented Durable Retry to prevent Split-Brain (HARD-004)
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

                if let Err(e) = self
                    .collection
                    .storage
                    .put(rollback_tx, &intent_key, b"aborted")
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
                    tracing::warn!("[INV-DB-3] Compensating transaction attempt {} failed. Retrying in 100ms...", attempts);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }

            if !success {
                tracing::error!(
                    "[INV-DB-3] FATAL: Compensating transaction failed after {} attempts. \
                     Index DB potential split-brain detected! Repair-on-Open required.",
                    max_attempts
                );
            }

            return Err(MemFuseError::Transaction(format!(
                "Index commit failed, storage rolled back via compensating tx. Error: {}",
                index_err
            )));
        }

        // 4. Finalize / Cleanup
        let cleanup_tx = TxId::new(
            self.collection
                .next_tx
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        if let Err(e) = self
            .collection
            .storage
            .put(cleanup_tx, &intent_key, b"committed")
            .await
        {
            tracing::warn!("Failed to write committed intent marker: {}", e);
        }

        if let Err(e) = self.collection.storage.commit(cleanup_tx).await {
            tracing::warn!("Failed to commit cleanup transaction: {}", e);
        }

        Ok(())
    }

    /// Rolls back any changes applied to the sub-systems in-memory.
    pub async fn rollback(self) -> Result<()> {
        let storage_res = self.collection.storage.rollback(self.tx_id).await;
        let index_res = self.collection.index.rollback(self.tx_id).await;

        if let Err(ref e) = storage_res {
            tracing::error!("[INV-DB-3] Storage rollback failed: {}", e);
        }
        if let Err(ref e) = index_res {
            tracing::error!("[INV-DB-3] Index rollback failed: {}", e);
        }

        storage_res?;
        index_res?;
        Ok(())
    }
}
