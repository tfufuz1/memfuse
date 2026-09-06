// FILE-CONTEXT: Layer 1 text search integration facade (memfuse-text).
// ZWECK: Exportiert Bm25Scorer, InvertedIndex, Morphologie-Tools und BM25-Modelle für DB-Hybrid-Suche.
// INVARIANTEN: #![forbid(unsafe_code)], TextIndex-Trait Implementierung ist fully async & transaction-aware.
// NICHT-OFFENSICHTLICH: Scorer delegiert direkt an InvertedIndex; MVCC & Lock-Free Storage durch StorageEngine.
// HOTSPOTS: Bm25Scorer::search, Bm25Scorer::insert
// STAND: TS:2026-08-30T22:01:55Z (SESSION: cf1f75c6)

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

    async fn last_tx_id(&self) -> Result<TxId> {
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
    use memfuse_core::BoxFuture;
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


    impl StorageEngine for MockStorage {
        fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
            Box::pin(async move {
            Ok(self.data.lock().unwrap().get(key).cloned()) // unwrap allowed
            })
        }

        fn put<'a>(&'a self, _tx_id: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            self.data
                .lock()
                .unwrap() // unwrap allowed
                .insert(key.to_vec(), value.to_vec());
            Ok(())
            })
        }

        fn delete<'a>(&'a self, _tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            self.data.lock().unwrap().remove(key); // unwrap allowed
            Ok(())
            })
        }

        fn commit<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn rollback<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn get_at_seq<'a>(&'a self, key: &'a [u8], _seq: u64) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
            Box::pin(async move {
            self.get(key).await
            })
        }

        fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
            Box::pin(async move {
            Ok(1)
            })
        }

        fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
            Box::pin(async move {
            Ok(TxId::new(1))
            })
        }

        fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn stats<'a>(&'a self) -> BoxFuture<'a, Result<StorageStats>> {
            Box::pin(async move {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
            })
        }

        fn pin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn unpin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
            Ok(())
            })
        }

        fn scan<'a>(
            &'a self,
            _start: std::ops::Bound<&'a [u8]>,
            _end: std::ops::Bound<&'a [u8]>,
        ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
            Box::pin(async move {
            Ok(Vec::new())
            })
        }

        fn scan_prefix<'a>(&'a self, prefix: &'a [u8]) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
            Box::pin(async move {
            let data = self.data.lock().unwrap(); // unwrap allowed
            let mut res = Vec::new();
            for (k, v) in data.iter() {
                if k.starts_with(prefix) {
                    res.push((k.clone(), v.clone()));
                }
            }
            Ok(res)
            })
        }

        fn scan_prefix_at<'a>(
            &'a self,
            prefix: &'a [u8],
            _seq_no: u64,
        ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
            Box::pin(async move {
            self.scan_prefix(prefix).await
            })
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
        assert_eq!(scorer.last_tx_id().await?, TxId(1));

        scorer.delete(tx, doc_id).await?;
        scorer.commit(tx).await?;
        assert_eq!(scorer.len().await, 0);

        scorer.rollback(tx).await?;
        scorer.rollback_to_tx(tx).await?;

        Ok(())
    }
}
