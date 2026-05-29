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
use tracing::instrument;

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
    #[instrument(skip(self, query))]
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>> {
        self.index.search(query, k).await
    }

    #[instrument(skip(self, text))]
    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()> {
        self.index.insert(tx, id, text).await
    }

    #[instrument(skip(self))]
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.index.delete(tx, id).await
    }

    #[instrument(skip(self))]
    async fn commit(&self, tx: TxId) -> Result<()> {
        self.index.commit(tx).await
    }

    #[instrument(skip(self))]
    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.index.rollback(tx).await
    }

    #[instrument(skip(self))]
    async fn stats(&self) -> Result<TextIndexStats> {
        self.index.stats().await
    }
}
