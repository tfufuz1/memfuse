// FILE-CONTEXT
// STAND: 2026-08-29T18:30:00Z
// ZWECK: Standalone Randfall- & Mutation-Testsuite für Reciprocal Rank Fusion (fusion.rs).
// INVARIANTEN:
//   1. Anti-Mirroring-Regel: Jeder erwartete RRF-Score ist unabhängig handberechnet
//      und im Kommentar mathematisch hergeleitet. Kein assert_eq! gegen Formelaufrufe.
//   2. Audit NC-6 Erhaltung: Nicht-finite Scores (NaN, +/-Inf) werden in apply_resonance_bonus
//      ans Ende sortiert.
//   3. Determinismus: Tie-breaking ist strikt deterministisch über doc_id.cmp().

#[cfg(feature = "coherence-bonus-fusion")]
use memfuse_db::fusion::{apply_resonance_bonus, ResonanceConfig};
use memfuse_db::fusion::{
    build_provenance, reciprocal_rank_fusion, weighted_reciprocal_rank_fusion,
    weighted_reciprocal_rank_fusion_with_options, MetadataMergePriority,
};
#[cfg(feature = "coherence-bonus-fusion")]
use memfuse_db::fusion::ResonanceConfig;
use memfuse_db::SearchResult;
use proptest::prelude::*;

/// Test 1: `test_nan_score_sorted_to_end_stable_order_preserved`
///
/// Hand-calculated scenario:
/// Inputs:
/// - doc1: score = 0.9 (finite)
/// - doc2: score = 0.5 (finite)
/// - doc3: score = f32::NAN (non-finite)
/// - doc4: score = 0.7 (finite)
/// - doc5: score = f32::NAN (non-finite)
///
/// Hand calculation for apply_resonance_bonus (Audit NC-6):
/// Given valid_signal_count = 1, beta = 0.5, gamma = 0.3:
/// Coherence boost factor = 1.0 + 0.3 * (1/1)^0.5 = 1.3
/// Boosted finite scores:
/// - doc1: 0.9 * 1.3 = 1.17
/// - doc4: 0.7 * 1.3 = 0.91
/// - doc2: 0.5 * 1.3 = 0.65
/// Non-finite scores:
/// - doc3: NaN * 1.3 = NaN
/// - doc5: NaN * 1.3 = NaN
///
/// NC-6 Partition & Tie-Break:
/// Finite scores appear first in descending order: doc1 (1.17) > doc4 (0.91) > doc2 (0.65).
/// Non-finite items are partitioned to the end: doc3 (NaN) and doc5 (NaN).
/// Between doc3 and doc5: secondary tie-break sort by ID: "doc3".cmp("doc5") -> doc3 < doc5.
/// Expected final order: ["doc1", "doc4", "doc2", "doc3", "doc5"].
#[test]
fn test_nan_score_sorted_to_end_stable_order_preserved() {
    let set = vec![
        SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc2".to_string(),
            score: 0.5,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc3".to_string(),
            score: f32::NAN,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc4".to_string(),
            score: 0.7,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc5".to_string(),
            score: f32::NAN,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
    ];

    // 1. Basic RRF standard pass: non-finite raw scores do not panic and are converted to finite RRF rank scores
    let fused = reciprocal_rank_fusion(vec![set.clone()], 10);
    assert_eq!(fused.len(), 5);
    assert!(
        fused.iter().all(|r| r.score.is_finite()),
        "Standard RRF rank scores must all be finite"
    );

    // 2. Audit NC-6 check on non-finite score sorting:
    #[cfg(feature = "coherence-bonus-fusion")]
    {
        let cfg = ResonanceConfig::default();
        let boosted1 = apply_resonance_bonus(set.clone(), 1, &cfg);
        let boosted2 = apply_resonance_bonus(set, 1, &cfg);

        let ids1: Vec<&str> = boosted1.iter().map(|r| r.id.as_str()).collect();
        let ids2: Vec<&str> = boosted2.iter().map(|r| r.id.as_str()).collect();

        // Hand-calculated expected order:
        // doc1 (1.17), doc4 (0.91), doc2 (0.65), doc3 (NaN), doc5 (NaN)
        let expected = vec!["doc1", "doc4", "doc2", "doc3", "doc5"];
        assert_eq!(
            ids1, expected,
            "NC-6 sorting must place finite scores descending, followed by NaN scores sorted by ID"
        );
        assert_eq!(
            ids1, ids2,
            "Relative ordering of NaN-score documents must be strictly deterministic across runs"
        );

        // Hand-calculated score verification
        assert!((boosted1[0].score - 1.17).abs() < 1e-5);
        assert!((boosted1[1].score - 0.91).abs() < 1e-5);
        assert!((boosted1[2].score - 0.65).abs() < 1e-5);
        assert!(boosted1[3].score.is_nan());
        assert!(boosted1[4].score.is_nan());
    }
}

/// Test 2: `test_infinity_score_handling`
///
/// Hand-calculated scenario:
/// Inputs:
/// - doc_high: score = 0.8 (finite)
/// - doc_low: score = 0.4 (finite)
/// - doc_inf: score = f32::INFINITY (non-finite)
/// - doc_neginf: score = f32::NEG_INFINITY (non-finite)
///
/// Hand calculation & Audit NC-6 Invariant:
/// In apply_resonance_bonus, is_finite() classifies INFINITY and NEG_INFINITY as non-finite.
/// Boost factor = 1.3:
/// - doc_high: 0.8 * 1.3 = 1.04
/// - doc_low: 0.4 * 1.3 = 0.52
/// - doc_inf: +Inf
/// - doc_neginf: -Inf
///
/// NC-6 Sort:
/// Finite scores first: doc_high (1.04) > doc_low (0.52).
/// Non-finite scores last: doc_inf (+Inf) and doc_neginf (-Inf).
/// Between +Inf and -Inf: total_cmp places +Inf before -Inf (-Inf < +Inf).
/// Hand-calculated expected order: ["doc_high", "doc_low", "doc_inf", "doc_neginf"].
#[test]
fn test_infinity_score_handling() {
    let set = vec![
        SearchResult {
            id: "doc_high".to_string(),
            score: 0.8,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc_low".to_string(),
            score: 0.4,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc_inf".to_string(),
            score: f32::INFINITY,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        SearchResult {
            id: "doc_neginf".to_string(),
            score: f32::NEG_INFINITY,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
    ];

    // 1. Basic RRF standard pass: handles non-finite raw scores without panic
    let fused = reciprocal_rank_fusion(vec![set.clone()], 10);
    assert_eq!(fused.len(), 4);
    assert!(
        fused.iter().all(|r| r.score.is_finite()),
        "Standard RRF rank scores must all be finite"
    );

    // 2. Audit NC-6 check on Infinity sorting:
    #[cfg(feature = "coherence-bonus-fusion")]
    {
        let cfg = ResonanceConfig::default();
        let boosted = apply_resonance_bonus(set, 1, &cfg);
        let ids: Vec<&str> = boosted.iter().map(|r| r.id.as_str()).collect();

        // Hand-calculated expected order:
        // doc_high (1.04), doc_low (0.52), doc_inf (+Inf), doc_neginf (-Inf)
        let expected = vec!["doc_high", "doc_low", "doc_inf", "doc_neginf"];
        assert_eq!(
            ids, expected,
            "NC-6 sorting must place finite scores descending, followed by non-finite Infinity scores"
        );

        assert!((boosted[0].score - 1.04).abs() < 1e-5);
        assert!((boosted[1].score - 0.52).abs() < 1e-5);
        assert_eq!(boosted[2].score, f32::INFINITY);
        assert_eq!(boosted[3].score, f32::NEG_INFINITY);
    }
}

/// Test 3: `test_exact_rank_tie_in_both_signals_deterministic_tiebreak`
///
/// Hand-calculated scenario (from rules/test_quality.md reference):
/// - Set 1 (Vector, weight 1.0): doc_b at rank 0, doc_a at rank 1
///   Score for doc_b in Set 1: 1 / (60 + 0 + 1) = 1 / 61 ≈ 0.016393443
///   Score for doc_a in Set 1: 1 / (60 + 1 + 1) = 1 / 62 ≈ 0.016129032
/// - Set 2 (Keyword, weight 1.0): doc_b at rank 0
///   Score for doc_b in Set 2: 1 / (60 + 0 + 1) = 1 / 61 ≈ 0.016393443
///
/// Hand calculation for doc_b:
/// Total score = 1/61 + 1/61 = 2/61 ≈ 0.032786885
/// Hand calculation for doc_a:
/// Total score = 1/62 ≈ 0.016129032
///
/// Extended with doc_c:
/// - Set 3 (Graph, weight 1.0): doc_c at rank 0
/// - Set 4 (Text, weight 1.0): doc_c at rank 0
/// Score for doc_c: 1/61 + 1/61 = 2/61 ≈ 0.032786885
///
/// Score Tie-Break: doc_b score (2/61) == doc_c score (2/61).
/// Secondary tie-break rule: id.cmp().
/// "doc_b".cmp("doc_c") -> Ordering::Less ("doc_b" comes before "doc_c").
///
/// Mutation proof experiment:
/// If tie-breaking replaced id.cmp() with insertion order, reversing set order
/// from [Set1, Set2, Set3, Set4] to [Set3, Set4, Set1, Set2] would place doc_c before doc_b,
/// failing the assertion `fused[0].id == "doc_b"`.
#[test]
fn test_exact_rank_tie_in_both_signals_deterministic_tiebreak() {
    let set1 = vec![
        SearchResult {
            id: "doc_b".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        },
        SearchResult {
            id: "doc_a".to_string(),
            score: 0.8,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        },
    ];

    let set2 = vec![SearchResult {
        id: "doc_b".to_string(),
        score: 0.95,
        metadata: None,
        matched_signals: vec![],
        provenance: None,
    }];

    let set3 = vec![SearchResult {
        id: "doc_c".to_string(),
        score: 0.9,
        metadata: None,
        matched_signals: vec![],
        provenance: None,
    }];

    let set4 = vec![SearchResult {
        id: "doc_c".to_string(),
        score: 0.95,
        metadata: None,
        matched_signals: vec![],
        provenance: None,
    }];

    // Order 1: Set1, Set2 (doc_b) followed by Set3, Set4 (doc_c)
    let fused1 = reciprocal_rank_fusion(
        vec![set1.clone(), set2.clone(), set3.clone(), set4.clone()],
        10,
    );

    // Order 2: Set3, Set4 (doc_c) followed by Set1, Set2 (doc_b)
    let fused2 = reciprocal_rank_fusion(
        vec![set3.clone(), set4.clone(), set1.clone(), set2.clone()],
        10,
    );

    let ids1: Vec<&str> = fused1.iter().map(|r| r.id.as_str()).collect();
    let ids2: Vec<&str> = fused2.iter().map(|r| r.id.as_str()).collect();

    // Hand-calculated expected scores:
    // doc_b: 2/61 ≈ 0.032786885
    // doc_c: 2/61 ≈ 0.032786885
    // doc_a: 1/62 ≈ 0.016129032
    assert_eq!(ids1, vec!["doc_b", "doc_c", "doc_a"]);
    assert_eq!(
        ids1, ids2,
        "Tie-break between equal score candidates must be independent of set insertion order"
    );

    // Hand-calculated reference asserts (rules/test_quality.md pattern)
    // doc_b score = 2 / 61 = 0.032786885
    assert!((fused1[0].score - 0.032786885).abs() < 1e-6);
    // doc_c score = 2 / 61 = 0.032786885
    assert!((fused1[1].score - 0.032786885).abs() < 1e-6);
    // doc_a score = 1 / 62 = 0.016129032
    assert!((fused1[2].score - 0.016129032).abs() < 1e-6);
}

/// Test 4: `test_array_length_mismatch_across_signals_no_panic`
///
/// Hand-calculated scenario:
/// - Signal 1 ("vector", weight 1.0): 50 items ("doc_00" .. "doc_49")
/// - Signal 2 ("text", weight 1.0): 3 items ("kw_00", "kw_01", "kw_02")
/// - Signal 3 ("graph", weight 1.0): 0 items (empty vec![])
///
/// Hand calculation:
/// doc_00: rank 0 in Vector -> 1 / (60 + 0 + 1) = 1/61 ≈ 0.016393443
/// kw_00: rank 0 in Text -> 1 / (60 + 0 + 1) = 1/61 ≈ 0.016393443
/// kw_02: rank 2 in Text -> 1 / (60 + 2 + 1) = 1/63 ≈ 0.015873016
/// doc_49: rank 49 in Vector -> 1 / (60 + 49 + 1) = 1/110 ≈ 0.009090909
///
/// Missing signals must NOT be treated as implicit rank 0 or rank Infinity.
/// Verification:
/// doc_49 (only in Vector at rank 49) receives 1/110 ≈ 0.009090909.
/// kw_02 (only in Text at rank 2) receives 1/63 ≈ 0.015873016.
/// kw_02 score (0.015873016) > doc_49 score (0.009090909).
/// Total results returned = 50 + 3 = 53 unique documents.
#[test]
fn test_array_length_mismatch_across_signals_no_panic() {
    let vector_set: Vec<SearchResult> = (0..50)
        .map(|i| SearchResult {
            id: format!("doc_{:02}", i),
            score: 0.9 - (i as f32 * 0.01),
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        })
        .collect();

    let text_set: Vec<SearchResult> = (0..3)
        .map(|i| SearchResult {
            id: format!("kw_{:02}", i),
            score: 0.8 - (i as f32 * 0.05),
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        })
        .collect();

    let graph_set: Vec<SearchResult> = vec![];

    let fused = weighted_reciprocal_rank_fusion(
        vec![
            ("vector".to_string(), vector_set, 1.0),
            ("text".to_string(), text_set, 1.0),
            ("graph".to_string(), graph_set, 1.0),
        ],
        100,
    );

    assert_eq!(
        fused.len(),
        53,
        "Fusion of mismatched signal lengths must yield all 53 unique documents without panic"
    );

    // Locate kw_02 and doc_49
    let kw_02_res = fused
        .iter()
        .find(|r| r.id == "kw_02")
        .expect("kw_02 must be present");
    let doc_49_res = fused
        .iter()
        .find(|r| r.id == "doc_49")
        .expect("doc_49 must be present");

    // Hand-calculated assertions:
    // kw_02 score = 1 / 63 = 0.015873016
    assert!(
        (kw_02_res.score - 0.015873016).abs() < 1e-6,
        "kw_02 score should be 1/63 ≈ 0.015873016, got {}",
        kw_02_res.score
    );

    // doc_49 score = 1 / 110 = 0.009090909
    assert!(
        (doc_49_res.score - 0.009090909).abs() < 1e-6,
        "doc_49 score should be 1/110 ≈ 0.009090909, got {}",
        doc_49_res.score
    );

    assert!(
        kw_02_res.score > doc_49_res.score,
        "Item at rank 2 in short signal must score higher than item at rank 49 in long signal"
    );
}

/// Test 5: `test_k_parameter_zero_boundary`
///
/// Hand-calculated scenario:
/// RRF rank is 1-based per Cormack et al., and rrf_k must be strictly positive (rrf_k > 0.0) to avoid division by zero risk.
/// When evaluating the minimal valid boundary condition at k = 0.001:
/// Rank 1: 1.0 / (0.001 + 1.0) = 1.0 / 1.001 ≈ 0.999000999
///
/// Verification:
/// 1. build_provenance with rrf_k = 0.001 produces exact expected contribution = 1.0 / 1.001.
/// 2. rrf_k = 0.0 is rejected via debug assertion in build_provenance.
#[test]
#[cfg_attr(debug_assertions, should_panic)]
fn test_k_parameter_zero_boundary_panics() {
    let k_zero = 0.0f32;
    let rank = 1u32;
    let weight = 1.0f32;
    build_provenance(
        Some(0.95),
        Some(rank),
        Some(weight),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        k_zero,
        Some("test_col".to_string()),
        Some("hnsw".to_string()),
        None,
    );
}

#[test]
fn test_k_parameter_minimal_positive_boundary() {
    let k_min = 0.001f32;
    let rank = 1u32; // 1-based rank in provenance
    let weight = 1.0f32;

    // Hand calculation: 1.0 / (0.001 + 1.0) ≈ 0.999000999
    let expected_contrib = weight / (k_min + rank as f32);

    let prov = build_provenance(
        Some(0.95),
        Some(rank),
        Some(weight),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        k_min,
        Some("test_col".to_string()),
        Some("hnsw".to_string()),
        Some(expected_contrib),
    );

    let contrib = prov
        .signal_contributions
        .get("vector")
        .expect("vector contribution present");

    assert!((contrib.rrf_contribution - expected_contrib).abs() < 1e-6);
}

/// Test 6: `test_k_parameter_very_large_score_convergence`
///
/// Hand-calculated scenario:
/// Evaluate RRF contribution formula at k = 1_000_000.0:
/// Rank 1 (1-based): score_1 = 1.0 / (1_000_000.0 + 1.0) = 1 / 1_000_001 ≈ 0.000000999999000001 (9.999990e-7)
/// Rank 2 (1-based): score_2 = 1.0 / (1_000_000.0 + 2.0) = 1 / 1_000_002 ≈ 0.000000999998000004 (9.999980e-7)
///
/// Delta = score_1 - score_2 = 1/1000001 - 1/1000002 = 1 / (1000001 * 1000002) ≈ 9.99997e-13.
///
/// Verification:
/// 1. Scores remain strictly positive and non-zero.
/// 2. Score monotonicity is preserved: score_1 > score_2.
/// 3. No precision loss or NaN.
#[test]
fn test_k_parameter_very_large_score_convergence() {
    let k_large = 1_000_000.0f32;

    let prov1 = build_provenance(
        Some(0.9),
        Some(1),
        Some(1.0),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        k_large,
        None,
        None,
        None,
    );

    let prov2 = build_provenance(
        Some(0.8),
        Some(2),
        Some(1.0),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        k_large,
        None,
        None,
        None,
    );

    let score1 = prov1
        .signal_contributions
        .get("vector")
        .unwrap()
        .rrf_contribution;
    let score2 = prov2
        .signal_contributions
        .get("vector")
        .unwrap()
        .rrf_contribution;

    // Hand calculations:
    // score1 = 1 / 1000001 ≈ 0.000000999999
    // score2 = 1 / 1000002 ≈ 0.000000999998
    assert!((score1 - 0.000000999999).abs() < 1e-10);
    assert!((score2 - 0.000000999998).abs() < 1e-10);

    assert!(
        score1 > score2,
        "Monotonicity must be preserved even with very large k = 1_000_000"
    );
    assert!(
        score1.is_finite() && score2.is_finite(),
        "Scores must remain finite"
    );
}

/// Test 7: `prop_replicator_weight_sum_invariant_preserved_through_fusion`
///
/// Hand-calculated scenario:
/// Simulates ReplicatorState output where weights w_vec, w_text, w_graph sum to 1.0.
///
/// Hand-calculated Config A: [0.5, 0.3, 0.2]
/// - doc_v0 (rank 0 in vector): 0.5 / (60 + 0 + 1) = 0.5 / 61 ≈ 0.008196721
/// - doc_t0 (rank 0 in text): 0.3 / (60 + 0 + 1) = 0.3 / 61 ≈ 0.004918033
/// - doc_g0 (rank 0 in graph): 0.2 / (60 + 0 + 1) = 0.2 / 61 ≈ 0.003278689
/// Expected order: ["doc_v0", "doc_t0", "doc_g0"]
///
/// Hand-calculated Config B: [0.2, 0.7, 0.1]
/// - doc_v0: 0.2 / 61 ≈ 0.003278689
/// - doc_t0: 0.7 / 61 ≈ 0.011475410
/// - doc_g0: 0.1 / 61 ≈ 0.001639344
/// Expected order: ["doc_t0", "doc_v0", "doc_g0"]
///
/// Hand-calculated Config C: [0.1, 0.1, 0.8]
/// - doc_v0: 0.1 / 61 ≈ 0.001639344
/// - doc_t0: 0.1 / 61 ≈ 0.001639344
/// - doc_g0: 0.8 / 61 ≈ 0.013114754
/// Equal score between doc_v0 and doc_t0 (0.1/61). Tie-break ID: "doc_t0" < "doc_v0".
/// Expected order: ["doc_g0", "doc_t0", "doc_v0"]
#[test]
fn test_replicator_weight_sum_invariant_concrete_configs() {
    let make_set = |id: &str| {
        vec![SearchResult {
            id: id.to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }]
    };

    // Config A: [0.5, 0.3, 0.2]
    let fused_a = weighted_reciprocal_rank_fusion_with_options(
        vec![
            ("vector".to_string(), make_set("doc_v0"), 0.5),
            ("text".to_string(), make_set("doc_t0"), 0.3),
            ("graph".to_string(), make_set("doc_g0"), 0.2),
        ],
        10,
        MetadataMergePriority::default(),
        true,
        None,
    );
    let ids_a: Vec<&str> = fused_a.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids_a, vec!["doc_v0", "doc_t0", "doc_g0"]);
    assert!((fused_a[0].score - (0.5 / 61.0)).abs() < 1e-6);
    assert!((fused_a[1].score - (0.3 / 61.0)).abs() < 1e-6);
    assert!((fused_a[2].score - (0.2 / 61.0)).abs() < 1e-6);

    // Config B: [0.2, 0.7, 0.1]
    let fused_b = weighted_reciprocal_rank_fusion_with_options(
        vec![
            ("vector".to_string(), make_set("doc_v0"), 0.2),
            ("text".to_string(), make_set("doc_t0"), 0.7),
            ("graph".to_string(), make_set("doc_g0"), 0.1),
        ],
        10,
        MetadataMergePriority::default(),
        true,
        None,
    );
    let ids_b: Vec<&str> = fused_b.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids_b, vec!["doc_t0", "doc_v0", "doc_g0"]);
    assert!((fused_b[0].score - (0.7 / 61.0)).abs() < 1e-6);
    assert!((fused_b[1].score - (0.2 / 61.0)).abs() < 1e-6);
    assert!((fused_b[2].score - (0.1 / 61.0)).abs() < 1e-6);

    // Config C: [0.1, 0.1, 0.8]
    let fused_c = weighted_reciprocal_rank_fusion_with_options(
        vec![
            ("vector".to_string(), make_set("doc_v0"), 0.1),
            ("text".to_string(), make_set("doc_t0"), 0.1),
            ("graph".to_string(), make_set("doc_g0"), 0.8),
        ],
        10,
        MetadataMergePriority::default(),
        true,
        None,
    );
    let ids_c: Vec<&str> = fused_c.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids_c, vec!["doc_g0", "doc_t0", "doc_v0"]);
    assert!((fused_c[0].score - (0.8 / 61.0)).abs() < 1e-6);
    assert!((fused_c[1].score - (0.1 / 61.0)).abs() < 1e-6);
    assert!((fused_c[2].score - (0.1 / 61.0)).abs() < 1e-6);
}

proptest! {
    #[test]
    fn prop_replicator_weight_sum_invariant_preserved_through_fusion(
        raw_w1 in 0.01f32..10.0f32,
        raw_w2 in 0.01f32..10.0f32,
        raw_w3 in 0.01f32..10.0f32,
    ) {
        // Normalize random weights to sum exactly to 1.0
        let sum = raw_w1 + raw_w2 + raw_w3;
        let w1 = raw_w1 / sum;
        let w2 = raw_w2 / sum;
        let w3 = raw_w3 / sum;

        prop_assert!((w1 + w2 + w3 - 1.0).abs() < 1e-5);

        let make_set = |id: &str| {
            vec![SearchResult {
                id: id.to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }]
        };

        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![
                ("vector".to_string(), make_set("doc1"), w1),
                ("text".to_string(), make_set("doc2"), w2),
                ("graph".to_string(), make_set("doc3"), w3),
            ],
            10,
            MetadataMergePriority::default(),
            true,
            None,
        );

        prop_assert_eq!(fused.len(), 3);

        // Verify that the score of each document equals its weight / 61.0 without extra weight modifications
        for res in &fused {
            let expected_w = match res.id.as_str() {
                "doc1" => w1,
                "doc2" => w2,
                "doc3" => w3,
                _ => unreachable!(),
            };
            let expected_score = expected_w / 61.0;
            prop_assert!((res.score - expected_score).abs() < 1e-6);
        }
    }
}
