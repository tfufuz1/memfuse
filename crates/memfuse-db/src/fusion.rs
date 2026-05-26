//! Reciprocal Rank Fusion implementation.

use crate::SearchResult;
use std::collections::HashMap;

/// Fuses multiple sets of ranked search results into a single ranked list using Reciprocal Rank Fusion (RRF).
/// RRF score = sum(1 / (k + rank)) for each result set, where k = 60 by default.
pub fn reciprocal_rank_fusion(
    result_sets: Vec<Vec<SearchResult>>,
    max_results: usize,
) -> Vec<SearchResult> {
    // ANCHOR:PERF:ALLOC-001 AGENT:09 STATUS:DONE
    // OPTIMIERUNG: HashMap::with_capacity(estimated_capacity)
    // VORHER: 83.4µs → NACHHER: 83.5µs (Innerhalb der Messungenauigkeit)
    let k = 60;

    let estimated_capacity = result_sets.iter().map(|s| s.len()).sum::<usize>();

    // id -> (total_score, metadata)
    let mut fused_scores: HashMap<String, (f32, Option<serde_json::Value>)> =
        HashMap::with_capacity(estimated_capacity);

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
        assert_eq!(ids, vec!["doc_b", "doc_a", "doc_d", "doc_c"]);
    }

    #[test]
    fn test_rrf_empty_inputs_return_empty() {
        let fused = reciprocal_rank_fusion(vec![], 5);
        assert!(
            fused.is_empty(),
            "Empty input sets should return empty results"
        );

        let fused2 = reciprocal_rank_fusion(vec![vec![], vec![]], 5);
        assert!(
            fused2.is_empty(),
            "Inputs with empty inner sets should return empty results"
        );
    }

    #[test]
    fn test_rrf_truncates_max_results() {
        let vectors: Vec<SearchResult> = (0..10)
            .map(|i| SearchResult {
                id: format!("doc_{}", i),
                score: 0.99,
                metadata: None,
            })
            .collect();

        let keywords: Vec<SearchResult> = (5..15)
            .map(|i| SearchResult {
                id: format!("doc_{}", i),
                score: 0.88,
                metadata: None,
            })
            .collect();

        // Pass 10 + 10 elements. The limit is exclusively 3.
        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 3);
        assert_eq!(
            fused.len(),
            3,
            "Result must be strictly truncated to max_results"
        );
    }

    #[test]
    fn test_rrf_identical_ranks() {
        let vectors = vec![SearchResult {
            id: "X".to_string(),
            score: 0.9,
            metadata: None,
        }];
        let keywords = vec![SearchResult {
            id: "Y".to_string(),
            score: 0.9,
            metadata: None,
        }];
        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 2);

        assert_eq!(fused.len(), 2);
        // Both hit rank 0. Score = 1 / (60 + 0 + 1) = 1/61 = ~0.01639
        assert!(
            (fused[0].score - (1.0 / 61.0)).abs() < f32::EPSILON,
            "Score mismatch: expected {}",
            1.0 / 61.0
        );
        assert!((fused[1].score - (1.0 / 61.0)).abs() < f32::EPSILON);
    }
}
