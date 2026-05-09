// ANCHOR:ARCH:BM25-001 — BM25 Scoring-Engine (Standard Okapi BM25).
// INVARIANTE: Default-Parameter: k1=1.5, b=0.75.
//! BM25 Scoring utility.

use std::collections::HashMap;

/// Provides BM25 scoring for an inverted index query.
#[derive(Debug)]
pub struct Bm25Scorer {
    k1: f32,
    b: f32,
    avg_doc_len: f32,
    total_docs: f32,
    /// Store df for terms
    doc_freqs: HashMap<String, u32>,
}

impl Default for Bm25Scorer {
    fn default() -> Self {
        Self::new(1.5, 0.75, 0.0, 0.0)
    }
}

impl Bm25Scorer {
    pub fn new(k1: f32, b: f32, avg_doc_len: f32, total_docs: f32) -> Self {
        Self {
            k1,
            b,
            avg_doc_len,
            total_docs,
            doc_freqs: HashMap::new(),
        }
    }

    pub fn add_doc_freq(&mut self, term: &str, df: u32) {
        self.doc_freqs.insert(term.to_string(), df);
    }

    /// Computes BM25 score for a specific term and document.
    pub fn score_term(&self, term: &str, tf: u32, doc_len: u32) -> f32 {
        let df = *self.doc_freqs.get(term).unwrap_or(&0) as f32;
        if df == 0.0 || self.total_docs == 0.0 {
            return 0.0;
        }

        // IDF variant used in BM25: log(1 + (N - n + 0.5) / (n + 0.5))
        let idf = ((self.total_docs - df + 0.5) / (df + 0.5) + 1.0).ln();
        let idf = idf.max(0.0); // avoid negative weights

        let tf_f32 = tf as f32;
        let doc_len_f32 = doc_len as f32;

        let numerator = tf_f32 * (self.k1 + 1.0);
        let denominator =
            tf_f32 + self.k1 * (1.0 - self.b + self.b * (doc_len_f32 / self.avg_doc_len));

        idf * (numerator / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic_scoring() {
        let mut scorer = Bm25Scorer::new(1.5, 0.75, 10.0, 100.0);
        scorer.add_doc_freq("rust", 10);

        let score1 = scorer.score_term("rust", 1, 10);
        let score2 = scorer.score_term("rust", 2, 10);

        assert!(score2 > score1);
        assert!(score1 > 0.0);
    }

    #[test]
    fn test_bm25_length_normalization() {
        let mut scorer = Bm25Scorer::new(1.5, 0.75, 10.0, 100.0);
        scorer.add_doc_freq("rust", 10);

        let score_short = scorer.score_term("rust", 1, 5);
        let score_long = scorer.score_term("rust", 1, 20);

        assert!(score_short > score_long);
    }
}
