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
        // AI-TAG[TEST-MIRRORING][MAJOR] Expected value `1.0/61.0` directly encodes `1/(k+0+1)` formula
        // BEFUND: `assert!((fused[0].score - (1.0 / 61.0)).abs() < f32::EPSILON)` lines 150+154.
        //         `1.0/61.0` = `1/(60+0+1)` — the exact formula from `reciprocal_rank_fusion()` line 20.
        //         An independent reference value would be e.g. a known-correct external score table for k=60.
        // RISIKO: If `k` or the rank-indexing formula changes (e.g. to 0-indexed), the test
        //         would still pass because both implementation and assertion would update identically.
        //         A mutation of `k = 60` → `k = 0` would break the comment but not the test invariant check.
        // EMPFEHLUNG: Use pre-computed float literals with explanatory comment:
        //             `const EXPECTED: f32 = 0.016393_f32; // 1/(60+1), computed independently`
        //             Or assert ordering/dominance only (e.g. "identical ranks → identical scores"),
        //             not the exact numeric value.
        // Both hit rank 0. Score = 1 / (60 + 0 + 1) = 1/61 = ~0.01639
        assert!(
            (fused[0].score - (1.0 / 61.0)).abs() < f32::EPSILON,
            "Score mismatch: expected {}",
            1.0 / 61.0
        );
        assert!((fused[1].score - (1.0 / 61.0)).abs() < f32::EPSILON);
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_rrf_never_panics(
                result_sets in prop::collection::vec(
                    prop::collection::vec(
                        any::<u32>().prop_map(|i| SearchResult {
                            id: format!("doc_{}", i % 100), // Collisions are good for testing
                            score: 0.0,
                            metadata: None,
                        }),
                        0..20
                    ),
                    0..5
                ),
                max_results in 0..50usize
            ) {
                let fused = reciprocal_rank_fusion(result_sets, max_results);
                assert!(fused.len() <= max_results);
            }
        }
    }
}
