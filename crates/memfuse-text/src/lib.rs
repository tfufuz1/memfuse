//! Hybrid Search Engine & BM25 Scoring (WP-2.1)
//!
//! Evaluates Inverse Document Frequencies integrating natively into the
//! `fusion.rs` layer in `memfuse-db`.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod morphology;
pub mod tokenizer;

pub use inverted::{BM25MorphIndex, InvertedIndex};
pub use morphology::{GermanCompoundSplitter, MorphologicalTokenizer};
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

    async fn stats(&self) -> Result<TextIndexStats> {
        self.index.stats().await
    }
}
