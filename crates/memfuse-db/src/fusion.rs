// ANCHOR:ARCH:FUSION-001 — Reciprocal Rank Fusion (RRF).
// FORMEL: score(doc) = Σ_{r ∈ result_sets} 1 / (k + rank_r(doc))
//! Reciprocal Rank Fusion for combining search result sets.

use memfuse_core::DocId;
use std::collections::HashMap;

/// Performs Reciprocal Rank Fusion on multiple result sets.
/// `k` is a smoothing constant, typically 60.
pub fn reciprocal_rank_fusion(result_sets: &[Vec<DocId>], k: f32) -> Vec<(DocId, f32)> {
    let mut combined_scores: HashMap<DocId, f32> = HashMap::new();

    for results in result_sets {
        for (rank, doc_id) in results.iter().enumerate() {
            let score = 1.0 / (k + (rank as f32) + 1.0);
            *combined_scores.entry(*doc_id).or_insert(0.0) += score;
        }
    }

    let mut final_results: Vec<(DocId, f32)> = combined_scores.into_iter().collect();
    final_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    final_results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let set1 = vec![DocId(1), DocId(2), DocId(3)];
        let set2 = vec![DocId(2), DocId(1), DocId(4)];

        let results = reciprocal_rank_fusion(&[set1, set2], 60.0);

        // Doc 1 and 2 should be at the top
        assert!(results[0].0 == DocId(1) || results[0].0 == DocId(2));
        assert!(results[0].1 > results[2].1);
    }
}
