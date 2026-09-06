use super::Collection;
use memfuse_core::{
    Result, StorageEngine, TxId, VectorIndex};
use std::sync::atomic::Ordering;

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Generates and returns the next sequential transaction ID for this collection.
    #[deprecated(
        since = "0.1.0",
        note = "Use `allocate_tx()` instead — both methods are functionally identical, `allocate_tx()` is the canonical public API."
    )]
    pub fn next_tx(&self) -> Result<TxId> {
        self.allocate_tx()
    }

    /// Allokiert eine eindeutige, atomar inkrementierte Transaction-ID.
    /// Externe Crates verwenden diese Methode statt eigener TxId-Generierung.
    /// Verhindert TxId-Kollisionen bei paralleler Ingestion (EMBED_CONCURRENCY > 1).
    pub fn allocate_tx(&self) -> Result<TxId> {
        let id = self.next_tx.fetch_add(1, Ordering::SeqCst);
        if id > TxId::MAX_COLLECTION_SEQUENCE {
            return Err(memfuse_core::MemFuseError::Transaction(
                "TxId counter exhausted: MAX_COLLECTION_SEQUENCE range exceeded. Collection must be recreated.".into(),
            ));
        }
        Ok(TxId::new(id))
    }

    /// Begins a new atomic transaction for this collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn begin_transaction(&self) -> Result<crate::transaction::DbTransaction<S, V>> {
        let tx = self.allocate_tx()?;
        Ok(crate::transaction::DbTransaction::new(self.clone(), tx))
    }
}
