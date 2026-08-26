//! Reciprocal Rank Fusion implementation.

use crate::SearchResult;
use std::collections::HashMap;

/// Fuses multiple sets of ranked search results into a single ranked list using Reciprocal Rank Fusion (RRF).
/// RRF score = sum(1 / (k + rank)) for each result set, where k = 60 by default.
pub fn reciprocal_rank_fusion(
    result_sets: Vec<Vec<SearchResult>>,
    max_results: usize,
) -> Vec<SearchResult> {
    let weighted_sets = result_sets
        .into_iter()
        .map(|set| ("unnamed".to_string(), set, 1.0))
        .collect();
    weighted_reciprocal_rank_fusion(weighted_sets, max_results)
}

/// Helper function to perform shallow merge of JSON metadata objects.
/// Later metadata keys supplement missing keys in existing metadata without overwriting.
fn merge_metadata(target: &mut Option<serde_json::Value>, source: Option<serde_json::Value>) {
    match (target, source) {
        (Some(t_val), Some(s_val)) => {
            if let (Some(t_obj), Some(s_obj)) = (t_val.as_object_mut(), s_val.as_object()) {
                for (k, v) in s_obj {
                    if !t_obj.contains_key(k) {
                        t_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (t @ None, Some(s_val)) => {
            *t = Some(s_val);
        }
        _ => {}
    }
}

/// Weighted Reciprocal Rank Fusion.
/// Multiplies the RRF contribution of each search signal set by its configured weight.
///
/// Accepts tuples of `(signal_name, result_set, weight)`.
pub fn weighted_reciprocal_rank_fusion(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
) -> Vec<SearchResult> {
    let k = 60;
    // Map: id -> (score, metadata, matched_signals)
    let mut fused: HashMap<String, (f32, Option<serde_json::Value>, Vec<String>)> = HashMap::new();

    for (signal_name, result_set, weight) in result_sets {
        if weight <= 0.0 {
            continue;
        }
        for (rank, doc) in result_set.into_iter().enumerate() {
            let score = weight / ((k + rank + 1) as f32);
            let entry = fused.entry(doc.id).or_insert((0.0, None, Vec::new()));
            entry.0 += score;
            merge_metadata(&mut entry.1, doc.metadata);
            if !signal_name.is_empty()
                && signal_name != "unnamed"
                && !entry.2.contains(&signal_name)
            {
                entry.2.push(signal_name.clone());
            }
        }
    }

    let mut ranked: Vec<SearchResult> = fused
        .into_iter()
        .map(|(id, (score, metadata, matched_signals))| SearchResult {
            id,
            score,
            metadata,
            matched_signals,
        })
        .collect();

    // AGT-DB-001 [CONCURRENCY][MAJOR]: Deterministic tie-breaking via secondary sort by ID.
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    ranked.truncate(max_results);
    ranked
}

/// Converts optional FusionWeights into (vector, text, graph) weight tuple.
pub fn weights_to_signal_factors(weights: Option<&memfuse_core::FusionWeights>) -> (f32, f32, f32) {
    match weights {
        Some(w) => (w.vector(), w.text(), w.graph()),
        None => (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0),
    }
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
                matched_signals: vec![],
            },
            SearchResult {
                id: "doc_b".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![],
            },
            SearchResult {
                id: "doc_c".to_string(),
                score: 0.7,
                metadata: None,
                matched_signals: vec![],
            },
        ];

        let keywords = vec![
            SearchResult {
                id: "doc_b".to_string(),
                score: 2.1,
                metadata: None,
                matched_signals: vec![],
            },
            SearchResult {
                id: "doc_d".to_string(),
                score: 1.5,
                metadata: None,
                matched_signals: vec![],
            },
        ];

        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 5);

        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["doc_b", "doc_a", "doc_d", "doc_c"]);
    }

    #[test]
    fn fusion_empty_inputs_returns_empty() {
        let result = reciprocal_rank_fusion(vec![vec![], vec![]], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn fusion_respects_max_results() {
        let large_set: Vec<SearchResult> = (0..100)
            .map(|i| SearchResult {
                id: format!("doc-{i}"),
                score: i as f32 / 100.0,
                metadata: None,
                matched_signals: vec![],
            })
            .collect();
        let result = reciprocal_rank_fusion(vec![large_set], 5);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn fusion_ignores_zero_or_negative_weight() {
        let set1 = vec![SearchResult {
            id: "doc-1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
        }];
        let set2 = vec![SearchResult {
            id: "doc-2".to_string(),
            score: 0.8,
            metadata: None,
            matched_signals: vec![],
        }];

        let result = weighted_reciprocal_rank_fusion(
            vec![
                ("signal1".to_string(), set1, 1.0),
                ("signal2".to_string(), set2, 0.0),
            ],
            10,
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "doc-1");
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
                matched_signals: vec![],
            })
            .collect();

        let keywords: Vec<SearchResult> = (5..15)
            .map(|i| SearchResult {
                id: format!("doc_{}", i),
                score: 0.88,
                metadata: None,
                matched_signals: vec![],
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
        let vectors = vec![
            SearchResult {
                id: "Y".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
            },
            SearchResult {
                id: "X".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
            },
        ];
        let keywords = vec![
            SearchResult {
                id: "X".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
            },
            SearchResult {
                id: "Y".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
            },
        ];

        // AGT-DB-001: Repeat 20 times to prove output ordering is strictly deterministic across iterations
        for _ in 0..20 {
            let fused = reciprocal_rank_fusion(vec![vectors.clone(), keywords.clone()], 2);
            assert_eq!(fused.len(), 2);
            assert_eq!(
                fused[0].id, "X",
                "Secondary sort by ID must place X before Y"
            );
            assert_eq!(fused[1].id, "Y");
            assert!((fused[0].score - fused[1].score).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_rrf_metadata_merging_and_matched_signals() {
        let vec_set = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: Some(serde_json::json!({"vec_key": "val1", "shared_key": "from_vector"})),
                matched_signals: vec![],
            }],
            1.0,
        );
        let graph_set = (
            "graph".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.8,
                metadata: Some(
                    serde_json::json!({"graph_key": "val2", "shared_key": "from_graph"}),
                ),
                matched_signals: vec![],
            }],
            1.0,
        );

        let fused = weighted_reciprocal_rank_fusion(vec![vec_set, graph_set], 1);
        assert_eq!(fused.len(), 1);
        let doc = &fused[0];
        assert_eq!(doc.id, "doc1");

        // Verify metadata merging (earlier signal key is retained, missing keys supplemented)
        let meta = doc.metadata.as_ref().unwrap().as_object().unwrap();
        assert_eq!(meta.get("vec_key").unwrap(), "val1");
        assert_eq!(meta.get("graph_key").unwrap(), "val2");
        assert_eq!(meta.get("shared_key").unwrap(), "from_vector");

        // Verify matched signals tracking
        assert_eq!(doc.matched_signals, vec!["vector", "graph"]);
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
                            matched_signals: vec![],
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
