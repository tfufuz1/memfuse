#![forbid(unsafe_code)]

//! Reciprocal Rank Fusion implementation.

use crate::SearchResult;
use std::collections::HashMap;

/// Fuses multiple sets of ranked search results into a single ranked list using Reciprocal Rank Fusion (RRF).
/// RRF score = sum(1 / (k + rank)) for each result set, where k = 60 by default.
pub fn reciprocal_rank_fusion(
    result_sets: Vec<Vec<SearchResult>>,
    max_results: usize,
) -> Vec<SearchResult> {
    let k = 60;

    // id -> (total_score, metadata)
    let mut fused_scores: HashMap<String, (f32, Option<serde_json::Value>)> = HashMap::new();

    for cur_set in result_sets {
        for (rank, cur_doc) in cur_set.into_iter().enumerate() {
            // Rank is 1-indexed for the formula usually, so rank + 1
            let score = 1.0 / ((k + rank + 1) as f32);
            let entry = fused_scores
                .entry(cur_doc.id)
                .or_insert((0.0, cur_doc.metadata));
            entry.0 += score;
        }
    }

    let mut final_results: Vec<SearchResult> = fused_scores
        .into_iter()
        .map(|(id, (score, metadata))| SearchResult {
            id,
            score,
            metadata,
        })
        .collect();

    // Sort descending by score
    final_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    final_results.truncate(max_results);

    final_results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_combines_result_sets() {
        let vectors = vec![
            SearchResult {
                id: "doc_a".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                id: "doc_b".to_string(),
                score: 0.8,
                metadata: None,
            },
            SearchResult {
                id: "doc_c".to_string(),
                score: 0.7,
                metadata: None,
            },
        ];

        let keywords = vec![
            SearchResult {
                id: "doc_b".to_string(),
                score: 2.1,
                metadata: None,
            },
            SearchResult {
                id: "doc_d".to_string(),
                score: 1.5,
                metadata: None,
            },
        ];

        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 5);

        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        // doc_b appears in both sets, rank 1 and rank 0 respectively.
        // score for doc_b: 1/(60+2) + 1/(60+1) = 1/62 + 1/61 = ~0.0325
        // score for doc_a: 1/(60+1) = 1/61 = ~0.0163
        // score for doc_d: 1/(60+2) = 1/62 = ~0.0161
        // score for doc_c: 1/(60+3) = 1/63 = ~0.0158
        assert_eq!(ids, vec!["doc_b", "doc_a", "doc_d", "doc_c"]);
    }
}
