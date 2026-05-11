// ANCHOR:TEST:LAYER-001 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:07 DATE:2026-05-09 STATUS:DONE
//! Verifies that memfuse-checkpoint can function with any StorageEngine implementation
//! without a direct dependency on memfuse-store in production code.

use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, Result, StorageStats, TxId};
use std::sync::Arc;

struct MockStorage;

#[async_trait::async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> { Ok(None) }
    async fn put(&self, _tx_id: TxId, _key: &[u8], _value: &[u8]) -> Result<()> { Ok(()) }
    async fn delete(&self, _tx_id: TxId, _key: &[u8]) -> Result<()> { Ok(()) }
    async fn commit(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
    async fn flush(&self) -> Result<()> { Ok(()) }
    async fn stats(&self) -> Result<StorageStats> {
        Ok(StorageStats {
            num_segments: 42,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn scan_prefix(&self, _prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> { Ok(Vec::new()) }
    fn last_seq_no(&self) -> u64 { 100 }
}

#[tokio::test]
async fn test_checkpoint_manager_with_mock_storage() {
    let storage = Arc::new(MockStorage);
    let manager = CheckpointManager::new(storage);

    let cp = manager.create_checkpoint("test").await.unwrap();
    assert_eq!(cp.name, "test");
    assert_eq!(cp.seq_no, 100);
}
