//! Hybrid Search Engine & BM25 Scoring (WP-2.1)
//!
//! Evaluates Inverse Document Frequencies integrating natively into the
//! `fusion.rs` layer in `memfuse-db`.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod morphology;
pub mod tokenizer;

pub use bm25::BM25;
pub use inverted::{BM25MorphIndex, InvertedIndex, Language};
pub use morphology::{normalize_umlauts, GermanCompoundSplitter, MorphologicalTokenizer};
pub use tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};

use memfuse_core::{DocId, Result, ScoredDocument, TextIndex, TextIndexStats, TxId};

/// Evaluates keyword weights and applies standard BM25 logic.
pub struct Bm25Scorer<S: memfuse_core::StorageEngine> {
    index: InvertedIndex<S>,
}

impl<S: memfuse_core::StorageEngine> Bm25Scorer<S> {
    pub fn new(storage: std::sync::Arc<S>, namespace: &str) -> Self {
        Self {
            index: InvertedIndex::new(storage, namespace),
        }
    }
}

#[async_trait::async_trait]
impl<S: memfuse_core::StorageEngine> TextIndex for Bm25Scorer<S> {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>> {
        self.index.search(query, k).await
    }

    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()> {
        self.index.insert(tx, id, text).await
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.index.delete(tx, id).await
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        self.index.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.index.rollback(tx).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.index.rollback_to_tx(tx_id).await
    }

    async fn last_tx_id(&self) -> Result<u64> {
        self.index.last_tx_id().await
    }

    async fn len(&self) -> usize {
        self.index.len().await
    }

    async fn stats(&self) -> Result<TextIndexStats> {
        self.index.stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::{StorageEngine, StorageStats};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockStorage {
        data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }

        async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            self.data
                .lock()
                .unwrap()
                .insert(key.to_vec(), value.to_vec());
            Ok(())
        }

        async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
            self.data.lock().unwrap().remove(key);
            Ok(())
        }

        async fn commit(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }

        async fn rollback(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }

        async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }

        async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
            self.get(key).await
        }

        async fn last_seq_no(&self) -> Result<u64> {
            Ok(1)
        }

        async fn last_tx_id(&self) -> Result<TxId> {
            Ok(TxId::new(1))
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
            let data = self.data.lock().unwrap();
            let mut res = Vec::new();
            for (k, v) in data.iter() {
                if k.starts_with(prefix) {
                    res.push((k.clone(), v.clone()));
                }
            }
            Ok(res)
        }

        async fn scan_prefix_at(
            &self,
            prefix: &[u8],
            _seq_no: u64,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.scan_prefix(prefix).await
        }
    }

    #[tokio::test]
    async fn bm25_scorer_case_full_lifecycle() -> Result<()> {
        let storage = std::sync::Arc::new(MockStorage::new());
        let scorer = Bm25Scorer::new(storage, "test_ns");

        let tx = TxId::new(1);
        let doc_id = DocId::from(100u64);

        scorer.insert(tx, doc_id, "rust text search engine").await?;
        scorer.commit(tx).await?;

        assert_eq!(scorer.len().await, 1);

        let results = scorer.search("rust engine", 10).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, doc_id);

        let stats = scorer.stats().await?;
        assert_eq!(stats.num_documents, 1);
        assert_eq!(scorer.last_tx_id().await?, 1);

        scorer.delete(tx, doc_id).await?;
        scorer.commit(tx).await?;
        assert_eq!(scorer.len().await, 0);

        scorer.rollback(tx).await?;
        scorer.rollback_to_tx(tx).await?;

        Ok(())
    }
}
