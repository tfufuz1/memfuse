// FILE-CONTEXT: Independent Anti-Mirroring BM25 Audit & Verification Suite.
// ZWECK: Verifiziert BM25 Scoring gegen handberechnete mathematische Werte.

use memfuse_text::bm25::{score_term, score_term_with_params, BM25};

#[test]
fn test_hand_calculated_bm25_corpus_scores() {
    // Hand calculated Corpus:
    // N = 5, avg_doc_len = 3.0, k1 = 1.5, b = 0.75
    let n = 5;
    let avg_doc_len = 3.0f32;

    // Term: "cherry" (df = 2)
    // IDF_arg = (5 - 2 + 0.5) / (2 + 0.5) = 3.5 / 2.5 = 1.4
    // IDF = ln(1.4) = 0.3364722366
    let df_cherry = 2;
    let expected_idf_cherry = (1.4f32).ln();

    // D2: doc_len = 3, tf = 1
    // norm_len = 3 / 3.0 = 1.0
    // tf_num = 1 * 2.5 = 2.5
    // tf_den = 1 + 1.5 * (0.25 + 0.75 * 1.0) = 2.5
    // tf_factor = 1.0
    // Expected score = ln(1.4) * 1.0 = 0.3364722366
    let score_d2_cherry = score_term(1, 3, avg_doc_len, df_cherry, n);
    let expected_d2 = expected_idf_cherry * 1.0;
    assert!(
        (score_d2_cherry - expected_d2).abs() < 1e-6,
        "Expected D2 score {}, got {}",
        expected_d2,
        score_d2_cherry
    );

    // D3: doc_len = 4, tf = 1
    // norm_len = 4 / 3.0 = 1.333333333
    // tf_den = 1 + 1.5 * (0.25 + 0.75 * (4/3)) = 1 + 1.5 * 1.25 = 2.875
    // tf_factor = 2.5 / 2.875 = 20 / 23 = 0.869565217
    // Expected score = ln(1.4) * (20/23) = 0.29258455
    let score_d3_cherry = score_term(1, 4, avg_doc_len, df_cherry, n);
    let expected_d3 = expected_idf_cherry * (20.0 / 23.0);
    assert!(
        (score_d3_cherry - expected_d3).abs() < 1e-6,
        "Expected D3 score {}, got {}",
        expected_d3,
        score_d3_cherry
    );

    // Length normalization effect verification:
    // D2 is shorter than D3, so D2 must score higher for same term frequency
    assert!(
        score_d2_cherry > score_d3_cherry,
        "Shorter document D2 must score higher than longer D3"
    );

    // Term: "elderberry" (df = 1)
    // IDF_arg = (5 - 1 + 0.5) / (1 + 0.5) = 4.5 / 1.5 = 3.0
    // IDF = ln(3.0) = 1.0986122887
    // D4: doc_len = 3, tf = 1 -> norm_len = 1.0 -> tf_factor = 1.0
    let score_d4_elderberry = score_term(1, 3, avg_doc_len, 1, n);
    let expected_d4 = (3.0f32).ln();
    assert!(
        (score_d4_elderberry - expected_d4).abs() < 1e-6,
        "Expected D4 score {}, got {}",
        expected_d4,
        score_d4_elderberry
    );
}

#[test]
fn test_idf_edge_cases() {
    let n = 10;
    let avg_doc_len = 10.0;

    // Case 1: Term in 0 docs (df = 0 or tf = 0)
    assert_eq!(score_term(0, 10, avg_doc_len, 1, n), 0.0);
    assert_eq!(score_term(1, 10, avg_doc_len, 0, n), 0.0);

    // Case 2: Term in exactly 1 doc (df = 1)
    let score_df1 = score_term(1, 10, avg_doc_len, 1, n);
    let idf_df1 = ((10.0f32 - 1.0 + 0.5) / (1.0 + 0.5)).ln(); // ln(9.5 / 1.5) = ln(6.3333333) = 1.8458268
    assert!((score_df1 - idf_df1).abs() < 1e-5);

    // Case 3: Term in ALL docs (df = N = 10)
    // Standard RSJ IDF arg = (10 - 10 + 0.5)/(10 + 0.5) = 0.5 / 10.5 = 0.0476 <= 1.0
    // Standard RSJ ln(0.0476) would be negative (-3.04).
    // The implementation handles df >= N/2 by clamping IDF to 1e-6 floor to avoid negative scores.
    let score_all = score_term(1, 10, avg_doc_len, 10, n);
    assert_eq!(score_all, 1e-6);

    // Case 4: Corrupt df > N (df = 15, N = 10)
    // (10 - 15 + 0.5)/(15 + 0.5) = -4.5 / 15.5 < 0 -> ln would be NaN.
    // Implementation clamps idf_arg <= 1.0 -> score = 1e-6 (no NaN).
    let score_corrupt = score_term(1, 10, avg_doc_len, 15, n);
    assert_eq!(score_corrupt, 1e-6);
    assert!(!score_corrupt.is_nan());
}

#[test]
fn test_b_parameter_sensitivity() {
    let tf = 1;
    let doc_len = 6;
    let avg_doc_len = 3.0;
    let df = 2;
    let n = 5;
    let k1 = 1.5;

    // b = 0.0: No length normalization (doc length is ignored)
    let score_b0 = score_term_with_params(tf, doc_len, avg_doc_len, df, n, k1, 0.0);
    let score_b0_ref = score_term_with_params(tf, 3, avg_doc_len, df, n, k1, 0.0);
    assert_eq!(
        score_b0, score_b0_ref,
        "When b=0, document length must not affect score"
    );

    // b = 1.0: Full length penalty
    let score_b1 = score_term_with_params(tf, doc_len, avg_doc_len, df, n, k1, 1.0);
    assert!(
        score_b1 < score_b0,
        "When b=1.0, longer doc (len=6 vs avg=3) receives full length penalty (b1: {}, b0: {})",
        score_b1,
        score_b0
    );
}

#[test]
fn test_bm25_extreme_document_lengths() {
    let n = 100;
    let avg_doc_len = 50.0;
    let df = 5;

    // Empty doc (doc_len = 0)
    let score_empty = score_term(1, 0, avg_doc_len, df, n);
    assert!(!score_empty.is_nan());
    assert!(score_empty > 0.0);

    // Short doc (doc_len = 1) vs extremely long doc (doc_len = 10_000)
    let score_short = score_term(1, 1, avg_doc_len, df, n);
    let score_long = score_term(1, 10_000, avg_doc_len, df, n);

    assert!(score_short > score_long);
    assert!(score_long > 0.0);
    assert!(!score_long.is_nan() && !score_long.is_infinite());
}

#[test]
fn test_bm25_struct_validation_and_methods() {
    let bm25 = BM25::new(1.2, 0.5).expect("valid parameters");
    assert_eq!(bm25.k1, 1.2);
    assert_eq!(bm25.b, 0.5);

    let score = bm25.score_term(2, 10, 10.0, 1, 10);
    let score_param = score_term_with_params(2, 10, 10.0, 1, 10, 1.2, 0.5);
    assert_eq!(score, score_param);
}
