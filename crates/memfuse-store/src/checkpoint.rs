//! Checkpoint management for LSM storage (WP-5.1).
//!
//! Provides transactional rollbacks and durable snapshots.

use crate::lsm::LsmStorage;
use memfuse_core::{Result, StorageEngine, TxId};
use std::sync::Arc;

/// Handles snapshot and rollback operations.
pub struct Checkpointer {
    storage: Arc<LsmStorage>,
}

impl Checkpointer {
    /// Creates a new checkpointer for the given storage.
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        Self { storage }
    }

    /// Creates a persistent snapshot and returns the sequence number.
    pub async fn create_checkpoint(&self) -> Result<u64> {
        let seq = self.storage.last_seq_no().await?;
        self.storage.pin_checkpoint(seq).await?;
        Ok(seq)
    }

    /// Rolls back the storage state to a specific sequence number.
    pub async fn rollback_to(&self, seq_no: u64) -> Result<()> {
        self.storage.rollback_to_tx(TxId::new(seq_no)).await
    }

    /// Removes a persistent snapshot.
    pub async fn drop_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.storage.unpin_checkpoint(seq_no).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsm::LsmConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_to_checkpoint() {
        let tmp = TempDir::new().unwrap(); // unwrap
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.unwrap()); // unwrap
        let checkpointer = Checkpointer::new(storage.clone());

        // 1. Initial state
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let cp1 = checkpointer.create_checkpoint().await.unwrap(); // unwrap

        // 2. Add more data
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec())); // unwrap

        // 3. Rollback
        checkpointer.rollback_to(cp1).await.expect("rollback");

        // 4. Verify state
        checkpointer.rollback_to(cp1).await.expect("rollback");
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // unwrap

        // 5. Verify we can still write and seq_no is correct
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap
        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));
        // unwrap
    }
}
