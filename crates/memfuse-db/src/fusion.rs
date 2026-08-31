//! Reciprocal Rank Fusion implementation.

// FILE-CONTEXT
// STAND: 2026-08-29T05:41:20Z (SESSION: f7999509)
// ZWECK: Reciprocal Rank Fusion (RRF) — vereint HNSW, BM25 und Graph-Ränge
// INVARIANTEN: k=60 Standard. Signale werden als Ränge fusioniert (NICHT rohe Scores).
//              Keine Score-Normalisierung nötig (Hauptvorteil von RRF, ADR-003).
// NICHT-OFFENSICHTLICH: Es existieren ZWEI öffentliche Funktionen:
//   1. `reciprocal_rank_fusion()` — gleichgewichtet (1.0 pro Signal)
//   2. `weighted_reciprocal_rank_fusion()` — mit Name + Gewicht pro Signal
//   NIEMALS eine dritte `execute_rrf()`-Funktion anlegen — sie würde diese duplizieren.
// SIEHE AUCH: DECISIONS.md ADR-003, crates/memfuse-db/AGENTS.md §4-Signal Fusion

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
    // The constant k=60 is the industry standard (Cormack et al., 2009).
    // It balances the precision/recall trade-off by smoothing rank impact:
    // higher k prevents top-ranked outliers in one signal from completely dominating,
    // while ensuring items appearing in multiple search signals accumulate significant boost.
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
    fn test_rrf_dual_signal_higher_than_single_signal() {
        let set1 = vec![SearchResult {
            id: "doc_both".to_string(),
            score: 0.99,
            metadata: None,
            matched_signals: vec![],
        }];
        let set2 = vec![
            SearchResult {
                id: "doc_both".to_string(),
                score: 0.95,
                metadata: None,
                matched_signals: vec![],
            },
            SearchResult {
                id: "doc_single".to_string(),
                score: 0.99,
                metadata: None,
                matched_signals: vec![],
            },
        ];

        let fused = reciprocal_rank_fusion(vec![set1, set2], 10);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].id, "doc_both", "Document ranked #1 in both signals must score higher than doc ranked #1 in only one signal");
        assert_eq!(fused[1].id, "doc_single");
        assert!(fused[0].score > fused[1].score);
    }

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
        let meta = doc.metadata.as_ref().unwrap().as_object().unwrap(); // unwrap
        assert_eq!(meta.get("vec_key").unwrap(), "val1"); // unwrap
        assert_eq!(meta.get("graph_key").unwrap(), "val2"); // unwrap
        assert_eq!(meta.get("shared_key").unwrap(), "from_vector"); // unwrap

        // Verify matched signals tracking
        assert_eq!(doc.matched_signals, vec!["vector", "graph"]);
    }

    #[test]
    fn test_weights_to_signal_factors_none_returns_equal_thirds() {
        let (vec_w, text_w, graph_w) = weights_to_signal_factors(None);
        // Anti-mirroring check: Expected 1/3 = 0.33333334
        assert!((vec_w - 0.33333334).abs() < 1e-5);
        assert!((text_w - 0.33333334).abs() < 1e-5);
        assert!((graph_w - 0.33333334).abs() < 1e-5);
    }

    #[test]
    fn test_weights_to_signal_factors_some_returns_exact_weights() {
        use memfuse_core::FusionWeights;
        let weights = FusionWeights::new(0.5, 0.3, 0.2).unwrap();
        let (v, t, g) = weights_to_signal_factors(Some(&weights));
        assert!((v - 0.5).abs() < 1e-5);
        assert!((t - 0.3).abs() < 1e-5);
        assert!((g - 0.2).abs() < 1e-5);
    }

    #[test]
    fn test_weighted_rrf_zero_max_results_returns_empty() {
        let set = vec![SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
        }];
        let fused = weighted_reciprocal_rank_fusion(vec![("vec".to_string(), set, 1.0)], 0);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_weighted_rrf_negative_weights_ignored() {
        let set = vec![SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
        }];
        let fused = weighted_reciprocal_rank_fusion(vec![("vec".to_string(), set, -0.5)], 10);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_anti_mirroring_rrf_reference_verification() {
        // Independent calculation formula:
        // score(doc) = sum_{s} (weight_s / (60 + rank_s + 1))
        // k = 60 constant
        let k = 60.0f32;

        // Test setup: 4 signals with custom results and weights
        let sig_vector = (
            "vector".to_string(),
            vec![
                SearchResult { id: "doc_a".into(), score: 0.9, metadata: None, matched_signals: vec![] },
                SearchResult { id: "doc_b".into(), score: 0.8, metadata: None, matched_signals: vec![] },
                SearchResult { id: "doc_c".into(), score: 0.7, metadata: None, matched_signals: vec![] },
            ],
            1.0f32,
        );

        let sig_text = (
            "text".to_string(),
            vec![
                SearchResult { id: "doc_b".into(), score: 10.0, metadata: None, matched_signals: vec![] },
                SearchResult { id: "doc_d".into(), score: 5.0, metadata: None, matched_signals: vec![] },
                SearchResult { id: "doc_a".into(), score: 1.0, metadata: None, matched_signals: vec![] },
            ],
            0.8f32,
        );

        let sig_graph = (
            "graph".to_string(),
            vec![
                SearchResult { id: "doc_c".into(), score: 0.95, metadata: None, matched_signals: vec![] },
                SearchResult { id: "doc_a".into(), score: 0.85, metadata: None, matched_signals: vec![] },
            ],
            0.5f32,
        );

        let sig_empty = (
            "empty".to_string(),
            vec![],
            1.0f32,
        );

        // Independent Reference Calculations:
        // doc_a:
        //   vector rank 0: 1.0 / (60 + 0 + 1) = 1.0 / 61.0
        //   text   rank 2: 0.8 / (60 + 2 + 1) = 0.8 / 63.0
        //   graph  rank 1: 0.5 / (60 + 1 + 1) = 0.5 / 62.0
        let ref_doc_a = (1.0 / (k + 1.0)) + (0.8 / (k + 3.0)) + (0.5 / (k + 2.0));

        // doc_b:
        //   vector rank 1: 1.0 / (60 + 1 + 1) = 1.0 / 62.0
        //   text   rank 0: 0.8 / (60 + 0 + 1) = 0.8 / 61.0
        let ref_doc_b = (1.0 / (k + 2.0)) + (0.8 / (k + 1.0));

        // doc_c:
        //   vector rank 2: 1.0 / (60 + 2 + 1) = 1.0 / 63.0
        //   graph  rank 0: 0.5 / (60 + 0 + 1) = 0.5 / 61.0
        let ref_doc_c = (1.0 / (k + 3.0)) + (0.5 / (k + 1.0));

        // doc_d:
        //   text   rank 1: 0.8 / (60 + 1 + 1) = 0.8 / 62.0
        let ref_doc_d = 0.8 / (k + 2.0);

        let fused = weighted_reciprocal_rank_fusion(
            vec![sig_vector, sig_text, sig_graph, sig_empty],
            10,
        );

        assert_eq!(fused.len(), 4);

        let get_score = |id: &str| -> f32 {
            fused.iter().find(|r| r.id == id).unwrap().score
        };

        assert!((get_score("doc_a") - ref_doc_a).abs() < 1e-6, "doc_a score mismatch: expected {}, got {}", ref_doc_a, get_score("doc_a"));
        assert!((get_score("doc_b") - ref_doc_b).abs() < 1e-6, "doc_b score mismatch: expected {}, got {}", ref_doc_b, get_score("doc_b"));
        assert!((get_score("doc_c") - ref_doc_c).abs() < 1e-6, "doc_c score mismatch: expected {}, got {}", ref_doc_c, get_score("doc_c"));
        assert!((get_score("doc_d") - ref_doc_d).abs() < 1e-6, "doc_d score mismatch: expected {}, got {}", ref_doc_d, get_score("doc_d"));
    }

    #[test]
    fn test_rrf_edge_case_all_signals_empty() {
        let fused = reciprocal_rank_fusion(vec![vec![], vec![], vec![], vec![]], 10);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_rrf_edge_case_single_signal_hits() {
        let set = vec![
            SearchResult { id: "doc1".into(), score: 0.9, metadata: None, matched_signals: vec![] },
            SearchResult { id: "doc2".into(), score: 0.8, metadata: None, matched_signals: vec![] },
        ];
        let fused = reciprocal_rank_fusion(vec![set, vec![], vec![]], 10);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
        assert_eq!(fused[0].score, 1.0 / 61.0);
        assert_eq!(fused[1].score, 1.0 / 62.0);
    }

    #[test]
    fn test_rrf_edge_case_identical_rankings() {
        let make_set = || vec![
            SearchResult { id: "doc1".into(), score: 0.9, metadata: None, matched_signals: vec![] },
            SearchResult { id: "doc2".into(), score: 0.8, metadata: None, matched_signals: vec![] },
            SearchResult { id: "doc3".into(), score: 0.7, metadata: None, matched_signals: vec![] },
        ];
        let fused = reciprocal_rank_fusion(vec![make_set(), make_set(), make_set(), make_set()], 10);
        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
        assert_eq!(fused[2].id, "doc3");
        assert_eq!(fused[0].score, 4.0 / 61.0);
        assert_eq!(fused[1].score, 4.0 / 62.0);
        assert_eq!(fused[2].score, 4.0 / 63.0);
    }

    #[test]
    fn test_rrf_edge_case_varying_result_counts_1_vs_10000() {
        let small_set = vec![
            SearchResult { id: "doc_rare".into(), score: 0.99, metadata: None, matched_signals: vec![] },
        ];
        let mut large_set: Vec<SearchResult> = vec![
            SearchResult { id: "doc_rare".into(), score: 0.99, metadata: None, matched_signals: vec![] },
        ];
        large_set.extend((1..10_000).map(|i| SearchResult {
            id: format!("doc_{i}"),
            score: 1.0 / (i + 1) as f32,
            metadata: None,
            matched_signals: vec![],
        }));

        let fused = reciprocal_rank_fusion(vec![small_set, large_set], 10);
        assert_eq!(fused.len(), 10);
        assert_eq!(fused[0].id, "doc_rare"); // Ranked #0 in small, #0 in large -> score = 2/61
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

            #[test]
            fn prop_rrf_score_monotonicity(
                rank in 0..10usize
            ) {
                let rrf_top = 1.0 / (60.0 + 1.0);
                let rrf_low = 1.0 / (60.0 + (rank + 1) as f32);
                assert!(rrf_top >= rrf_low);
            }
        }
    }
}
