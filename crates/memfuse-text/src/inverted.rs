//! Inverted Index backend mapping terms to Roaring Bitmaps.

use roaring::RoaringBitmap;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct InvertedIndex {
    /// Maps a term (String) to a RoaringBitmap of document IDs.
    pub postings: HashMap<String, RoaringBitmap>,

    /// Metadata mapping doc_id -> token_count for BM25
    pub doc_lengths: HashMap<u32, u32>,
    pub total_docs: u32,
    pub sum_doc_lengths: u64,
}

impl InvertedIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn index_document(&mut self, doc_id: u32, tokens: &[String]) {
        self.total_docs += 1;
        self.doc_lengths.insert(doc_id, tokens.len() as u32);
        self.sum_doc_lengths += tokens.len() as u64;

        for token in tokens {
            self.postings
                .entry(token.clone())
                .or_default()
                .insert(doc_id);
        }
    }

    pub fn avg_doc_len(&self) -> f32 {
        if self.total_docs == 0 {
            return 0.0;
        }
        (self.sum_doc_lengths as f32) / (self.total_docs as f32)
    }

    pub fn get_posting(&self, term: &str) -> Option<&RoaringBitmap> {
        self.postings.get(term)
    }
}
