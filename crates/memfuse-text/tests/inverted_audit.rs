// FILE-CONTEXT: InvertedIndex CRUD, MVCC Snapshot Isolation & Concurrency Audit Suite.
// ZWECK: Verifiziert CRUD-Semantik, MVCC Snapshot Isolation und Nebenläufigkeits-Konsistenz.

use async_trait::async_trait;
use memfuse_core::{
    DocId, Result, StorageEngine, StorageStats, TextIndex, TxId, TOMBSTONE_BIT,
};
use memfuse_text::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct AuditMockStorage {
    store: RwLock<HashMap<Vec<u8>, Vec<(Vec<u8>, u64)>>>,
    staged: RwLock<HashMap<TxId, Vec<Vec<u8>>>>,
    next_seq: AtomicU64,
}

impl AuditMockStorage {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            next_seq: AtomicU64::new(1),
        }
    }
}

#[async_trait]
impl StorageEngine for AuditMockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_at_seq(key, u64::MAX).await
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        self.store
            .write()
            .entry(key.to_vec())
            .or_default()
            .push((value.to_vec(), seq));
        self.staged
            .write()
            .entry(tx_id)
            .or_default()
            .push(key.to_vec());
        Ok(())
    }

    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let mut w = self.store.write();
        w.entry(key.to_vec())
            .or_default()
            .push((Vec::new(), seq | TOMBSTONE_BIT));
        self.staged
            .write()
            .entry(tx_id)
            .or_default()
            .push(key.to_vec());
        Ok(())
    }

    async fn commit(&self, tx_id: TxId) -> Result<()> {
        self.staged.write().remove(&tx_id);
        Ok(())
    }

    async fn rollback(&self, tx_id: TxId) -> Result<()> {
        let keys = self.staged.write().remove(&tx_id).unwrap_or_default();
        let mut store = self.store.write();
        for k in keys {
            if let Some(versions) = store.get_mut(&k) {
                versions.pop();
                if versions.is_empty() {
                    store.remove(&k);
                }
            }
        }
        Ok(())
    }

    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }

    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
        let store = self.store.read();
        if let Some(versions) = store.get(key) {
            for (val, v_seq) in versions.iter().rev() {
                let raw_seq = v_seq & !TOMBSTONE_BIT;
                if raw_seq <= seq {
                    if (v_seq & TOMBSTONE_BIT) != 0 {
                        return Ok(None);
                    }
                    return Ok(Some(val.clone()));
                }
            }
        }
        Ok(None)
    }

    async fn last_seq_no(&self) -> Result<u64> {
        Ok(self.next_seq.load(Ordering::SeqCst).saturating_sub(1))
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
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

    async fn pin_checkpoint(&self, _id: u64) -> Result<()> {
        Ok(())
    }

    async fn unpin_checkpoint(&self, _id: u64) -> Result<()> {
        Ok(())
    }

    async fn scan(
        &self,
        _start: std::ops::Bound<&[u8]>,
        _end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(Vec::new())
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix_at(prefix, u64::MAX).await
    }

    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let store = self.store.read();
        let mut results = Vec::new();
        for (k, versions) in store.iter() {
            if k.starts_with(prefix) {
                for (val, v_seq) in versions.iter().rev() {
                    let raw_seq = v_seq & !TOMBSTONE_BIT;
                    if raw_seq <= seq_no {
                        if (v_seq & TOMBSTONE_BIT) == 0 {
                            results.push((k.clone(), val.clone()));
                        }
                        break;
                    }
                }
            }
        }
        Ok(results)
    }
}

#[tokio::test]
async fn test_inverted_index_crud_lifecycle() -> Result<()> {
    let storage = Arc::new(AuditMockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "crud_audit");

    // 1. INSERT Document 1 & 2
    let tx1 = TxId::new(1);
    let d1 = DocId::new(101);
    let d2 = DocId::new(102);

    index.insert(tx1, d1, "quantum computing breakthroughs").await?;
    index.insert(tx1, d2, "quantum algorithms distributed memory").await?;
    index.commit(tx1).await?;

    assert_eq!(index.len().await, 2);
    let quantum_hits = index.search("quantum", 10).await?;
    assert_eq!(quantum_hits.len(), 2);

    // 2. UPDATE Document 1 (Replace text: remove "computing breakthroughs", add "error correction")
    let tx2 = TxId::new(2);
    index.insert(tx2, d1, "quantum error correction").await?;
    index.commit(tx2).await?;

    // Ghost term resolution check
    let tx_res = TxId::new(3);
    index.resolve_tombstones(tx_res).await?;

    let old_term_hits = index.search("breakthroughs", 10).await?;
    assert_eq!(
        old_term_hits.len(),
        0,
        "Ghost term 'breakthroughs' must be removed after update & tombstone resolution"
    );

    let new_term_hits = index.search("correction", 10).await?;
    assert_eq!(new_term_hits.len(), 1);
    assert_eq!(new_term_hits[0].doc_id, d1);

    // 3. DELETE Document 2
    let tx4 = TxId::new(4);
    index.delete(tx4, d2).await?;
    index.commit(tx4).await?;

    assert_eq!(index.len().await, 1);
    let quantum_hits_after_del = index.search("quantum", 10).await?;
    assert_eq!(quantum_hits_after_del.len(), 1);
    assert_eq!(quantum_hits_after_del[0].doc_id, d1);

    Ok(())
}

#[tokio::test]
async fn test_inverted_index_mvcc_snapshot_isolation() -> Result<()> {
    let storage = Arc::new(AuditMockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "mvcc_audit");

    // Snapshot seq_0: index is empty
    let seq_0 = storage.last_seq_no().await?;

    // Tx 1: Insert doc 1
    let tx1 = TxId::new(1);
    let d1 = DocId::new(1);
    index.insert(tx1, d1, "sovereign memory engine").await?;
    index.commit(tx1).await?;
    let seq_1 = storage.last_seq_no().await?;

    // Tx 2: Insert doc 2
    let tx2 = TxId::new(2);
    let d2 = DocId::new(2);
    index.insert(tx2, d2, "sovereign memory architecture").await?;
    index.commit(tx2).await?;

    // Verify isolation:
    // At seq_0: 0 results
    let hits_seq0 = index.search_at("memory", 10, seq_0).await?;
    assert_eq!(hits_seq0.len(), 0);

    // At seq_1: 1 result (d1)
    let hits_seq1 = index.search_at("memory", 10, seq_1).await?;
    assert_eq!(hits_seq1.len(), 1);
    assert_eq!(hits_seq1[0].doc_id, d1);

    // At latest: 2 results (d1, d2)
    let hits_latest = index.search("memory", 10).await?;
    assert_eq!(hits_latest.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_inverted_index_concurrency_stress() -> Result<()> {
    let storage = Arc::new(AuditMockStorage::new());
    let index = Arc::new(InvertedIndex::new(storage.clone(), "stress_audit"));

    let num_tasks = 8;
    let docs_per_task = 25;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let idx = index.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..docs_per_task {
                let doc_raw = (t * docs_per_task + i + 1) as u64;
                let tx_raw = doc_raw;
                let doc_id = DocId::new(doc_raw);
                let tx = TxId::new(tx_raw);
                let text = format!("concurrent document {} worker {}", i, t);

                idx.insert(tx, doc_id, &text)
                    .await
                    .expect("insert must succeed");
                idx.commit(tx).await.expect("commit must succeed");

                // Interleaved search
                let _ = idx.search("concurrent", 5).await;
            }
        }));
    }

    for h in handles {
        h.await.expect("worker task completed");
    }

    let expected_total_docs = (num_tasks * docs_per_task) as u64;
    assert_eq!(
        index.len().await,
        expected_total_docs as usize,
        "Total document count must match concurrent inserts"
    );

    let search_res = index.search("concurrent", 200).await?;
    assert_eq!(
        search_res.len(),
        expected_total_docs as usize,
        "Search hits must equal total inserted documents"
    );

    Ok(())
}
