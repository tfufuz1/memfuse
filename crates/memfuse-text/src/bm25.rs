// FILE-CONTEXT: BM25 Scoring & Parameter Struct.
// ZWECK: Berechnet mathematisch BM25 Term-Scores und validiert Hyperparameter (k1, b).
// INVARIANTEN: k1 >= 0.0 (non-NaN, finite), 0.0 <= b <= 1.0 (non-NaN, finite).
// NICHT-OFFENSICHTLICH: Log-IDF ist mit Robertson-Spärck-Jones Glättung implementiert (ln(1 + (N - df + 0.5) / (df + 0.5))).
// HOTSPOTS: score_term, score_term_with_params
// STAND: TS:2026-08-30T22:01:55Z (SESSION: cf1f75c6)

//! Pure BM25 scoring functions and parameter structure.

use memfuse_core::{MemFuseError, Result};

/// Default k1 parameter for BM25 term frequency saturation scaling.
pub const BM25_K1: f32 = 1.5;
/// Default b parameter for BM25 document length normalization penalty tuning.
pub const BM25_B: f32 = 0.75;

/// BM25 scoring model parameters.
///
/// Recommendations (Robertson-Walker defaults):
/// - `k1`: Term frequency saturation control parameter. Recommended default value is `1.5` (well-studied range `1.2..2.0`). Must be non-negative (`k1 >= 0.0`).
/// - `b`: Document length normalization penalty parameter. Recommended default value is `0.75` (range `0.0..1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BM25 {
    pub k1: f32,
    pub b: f32,
}

impl BM25 {
    /// Creates a new `BM25` configuration after validating `k1` and `b`.
    ///
    /// # Errors
    /// Returns `MemFuseError::InvalidInput` if `k1 < 0.0` or `k1` is NaN,
    /// or if `b` is outside `[0.0, 1.0]` or is NaN.
    pub fn new(k1: f32, b: f32) -> Result<Self> {
        if k1 < 0.0 || k1.is_nan() {
            return Err(MemFuseError::InvalidInput("k1 must be >= 0.0".into()));
        }
        if !(0.0..=1.0).contains(&b) || b.is_nan() {
            return Err(MemFuseError::InvalidInput("b must be in [0.0, 1.0]".into()));
        }
        Ok(Self { k1, b })
    }

    /// Calculates the BM25 score for a single term using this instance's `k1` and `b`.
    pub fn score_term(&self, tf: u32, doc_len: u32, avg_doc_len: f32, df: u32, n: u32) -> f32 {
        score_term_with_params(tf, doc_len, avg_doc_len, df, n, self.k1, self.b)
    }
}

impl Default for BM25 {
    /// Returns default `BM25` parameters (`k1 = 1.5`, `b = 0.75`), matching `BM25_K1` and `BM25_B`.
    fn default() -> Self {
        Self {
            k1: BM25_K1,
            b: BM25_B,
        }
    }
}

/// Calculates the BM25 score for a single term in a document using default `BM25_K1` and `BM25_B`.
///
/// # Arguments
/// * `tf` - Term frequency in the document
/// * `doc_len` - Length of the document (number of tokens)
/// * `avg_doc_len` - Average document length in the collection
/// * `df` - Document frequency (number of documents containing the term)
/// * `n` - Total number of documents in the collection
pub fn score_term(tf: u32, doc_len: u32, avg_doc_len: f32, df: u32, n: u32) -> f32 {
    score_term_with_params(tf, doc_len, avg_doc_len, df, n, BM25_K1, BM25_B)
}

/// Calculates the BM25 score for a single term in a document using custom `k1` and `b` parameters.
pub fn score_term_with_params(
    tf: u32,
    doc_len: u32,
    avg_doc_len: f32,
    df: u32,
    n: u32,
    k1: f32,
    b: f32,
) -> f32 {
    if n == 0 || df == 0 || tf == 0 {
        return 0.0;
    }

    let tf = tf as f32;
    let doc_len = doc_len as f32;
    let df = df as f32;
    let n = n as f32;

    // Standard IDF formula: ln((N - df + 0.5) / (df + 0.5))
    // If a term appears in more than half of the documents, the raw IDF formula can be negative.
    // If df > n, the argument to ln() becomes negative, leading to NaN.
    let idf_arg = (n - df + 0.5) / (df + 0.5);
    let idf = if idf_arg <= 1.0 {
        // If the term is very common (df > N/2 approximately), we use a small positive floor.
        // This also handles the corruption case where df > n.
        1e-6
    } else {
        idf_arg.ln()
    };

    let avg_doc = avg_doc_len.max(1.0);
    let norm_doc_len = doc_len / avg_doc;

    let tf_numerator = tf * (k1 + 1.0);
    let tf_denominator = tf + k1 * (1.0 - b + b * norm_doc_len);

    idf * (tf_numerator / tf_denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_no_nan_or_infinity() {
        // Alle Grenzfall-Kombinationen
        let cases = [
            (0, 0, 0.0, 0, 0),               // alles null
            (1, 0, 0.0, 0, 1),               // doc_len = 0
            (1, 1, 0.0, 0, 1),               // avg_doc_len = 0
            (u32::MAX, 1, 1.0, 1, 1),        // maximale tf
            (1, 1, 1.0, u32::MAX, u32::MAX), // df = n (alle Docs)
        ];
        for (tf, doc_len, avg, df, n) in cases {
            let score = score_term(tf, doc_len, avg, df, n);
            assert!(!score.is_nan(), "NaN für {:?}", (tf, doc_len, avg, df, n));
            assert!(
                !score.is_infinite(),
                "Infinity für {:?}",
                (tf, doc_len, avg, df, n)
            );
            assert!(score >= 0.0, "Negativ für {:?}", (tf, doc_len, avg, df, n));
        }
    }

    #[test]
    fn test_bm25_new_validation() {
        assert!(BM25::new(1.5, 0.75).is_ok());
        assert!(BM25::new(0.0, 0.0).is_ok());
        assert!(BM25::new(2.0, 1.0).is_ok());

        // Negative k1
        let err_k1 = BM25::new(-0.1, 0.75).unwrap_err();
        assert!(matches!(err_k1, MemFuseError::InvalidInput(_)));
        if let MemFuseError::InvalidInput(msg) = err_k1 {
            assert_eq!(msg, "k1 must be >= 0.0");
        }

        // NaN k1
        let err_k1_nan = BM25::new(f32::NAN, 0.75).unwrap_err();
        assert!(matches!(err_k1_nan, MemFuseError::InvalidInput(_)));

        // b out of bounds
        let err_b_neg = BM25::new(1.5, -0.01).unwrap_err();
        assert!(matches!(err_b_neg, MemFuseError::InvalidInput(_)));
        if let MemFuseError::InvalidInput(msg) = err_b_neg {
            assert_eq!(msg, "b must be in [0.0, 1.0]");
        }

        let err_b_high = BM25::new(1.5, 1.01).unwrap_err();
        assert!(matches!(err_b_high, MemFuseError::InvalidInput(_)));

        let err_b_nan = BM25::new(1.5, f32::NAN).unwrap_err();
        assert!(matches!(err_b_nan, MemFuseError::InvalidInput(_)));
    }

    #[test]
    fn test_bm25_default() {
        let default_bm25 = BM25::default();
        assert_eq!(default_bm25.k1, 1.5);
        assert_eq!(default_bm25.b, 0.75);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_bm25_score_term_finite_and_non_negative(
            tf in 0..10_000u32,
            doc_len in 0..10_000u32,
            avg_doc_len in 0.0..10_000.0f32,
            df in 0..10_000u32,
            n in 0..10_000u32,
        ) {
            let score = score_term(tf, doc_len, avg_doc_len, df, n);
            prop_assert!(score.is_finite(), "Score must be finite");
            prop_assert!(score >= 0.0, "Score must be non-negative");
        }
    }

    #[test]
    fn score_term_case_exact_anti_mirroring_value() {
        // Independent mathematical calculation:
        // tf = 1, doc_len = 10, avg_doc_len = 10.0, df = 1, n = 10, k1 = 1.5, b = 0.75
        // idf_arg = (10 - 1 + 0.5) / (1 + 0.5) = 9.5 / 1.5 = 6.333333333...
        // idf = ln(19/3) = 1.8458268
        // norm_doc_len = 10 / 10 = 1.0
        // tf_num = 1 * 2.5 = 2.5
        // tf_den = 1 + 1.5 * (0.25 + 0.75) = 2.5
        // tf_factor = 2.5 / 2.5 = 1.0
        // expected score = 1.8458268
        let score = score_term(1, 10, 10.0, 1, 10);
        let expected = 1.845_826_8;
        assert!(
            (score - expected).abs() < 1e-6,
            "Expected score close to {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn score_term_with_params_case_b_zero_length_norm_disabled() {
        // Independent mathematical computation:
        // tf = 2, doc_len = 100, avg_doc_len = 50.0, df = 5, n = 100, k1 = 1.5, b = 0.0
        // idf_arg = (100 - 5 + 0.5) / (5 + 0.5) = 95.5 / 5.5 = 17.363636...
        // idf = ln(95.5 / 5.5) = 2.8543787
        // tf_num = 2 * (1.5 + 1.0) = 5.0
        // tf_den = 2 + 1.5 * (1.0 - 0.0 + 0.0) = 3.5
        // score = 2.8543787 * (5.0 / 3.5) = 4.077684
        let score = score_term_with_params(2, 100, 50.0, 5, 100, 1.5, 0.0);
        let expected = 4.077_684;
        assert!(
            (score - expected).abs() < 1e-5,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn score_term_with_params_case_k1_zero_saturation_disabled() {
        // Independent mathematical computation:
        // tf = 5, doc_len = 50, avg_doc_len = 50.0, df = 10, n = 100, k1 = 0.0, b = 0.75
        // idf_arg = (100 - 10 + 0.5) / (10 + 0.5) = 90.5 / 10.5 = 8.6190476
        // idf = ln(90.5 / 10.5) = 2.1539836
        // tf_num = 5 * 1.0 = 5.0
        // tf_den = 5 + 0 = 5.0
        // score = idf * (5.0 / 5.0) = 2.1539836
        let score = score_term_with_params(5, 50, 50.0, 10, 100, 0.0, 0.75);
        let expected = 2.153_983_6;
        assert!(
            (score - expected).abs() < 1e-5,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn bm25_struct_score_term_case_matches_standalone_function() {
        let bm25 = BM25::new(1.5, 0.75).expect("valid parameters");
        let struct_score = bm25.score_term(2, 50, 50.0, 5, 100);
        let fn_score = score_term(2, 50, 50.0, 5, 100);
        assert_eq!(struct_score, fn_score);
    }

    #[test]
    fn test_bm25_score() {
        let score = score_term(2, 100, 150.0, 10, 1000);
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_score_zero_n() {
        let score = score_term(2, 100, 150.0, 10, 0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_bm25_score_zero_doc_len() {
        let score = score_term(2, 0, 150.0, 10, 1000);
        assert!(!score.is_nan());
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_score_zero_avg_doc_len() {
        let score = score_term(2, 100, 0.0, 10, 1000);
        assert!(!score.is_nan());
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_score_corruption_df_gt_n() {
        // df = 2000, n = 1000 -> (1000 - 2000 + 0.5) / (2000 + 0.5) = -999.5 / 2000.5 -> negative
        // ln of negative is NaN
        let score = score_term(2, 100, 150.0, 2000, 1000);
        assert!(!score.is_nan());
        assert!(!score.is_infinite());
        assert!(score >= 0.0);
    }

    #[test]
    fn bm25_handles_extreme_tf() {
        // tf = u32::MAX (4 billion occurrences)
        let score = score_term(u32::MAX, 1000, 500.0, 1, 1_000_000);
        assert!(
            score.is_finite(),
            "BM25 must return finite score for extreme tf"
        );
        assert!(score >= 0.0);
    }

    #[test]
    fn test_bm25_extreme_values() {
        // Very small avg_doc_len
        let score = score_term(1, 1, 1e-10, 1, 10);
        assert!(!score.is_nan());
        assert!(!score.is_infinite());

        // df very close to n
        let score = score_term(1, 10, 10.0, 10, 10);
        assert!(!score.is_nan());
        assert!(score >= 0.0);
    }

    #[test]
    fn test_bm25_score_monotonicity_more_matches() {
        // Single term match ("Katze" in doc 1) vs dual term match ("Katze" and "Hund" in doc 2)
        // Corpus: N = 100, avg_doc_len = 10.0
        // Term "Katze": df = 5
        // Term "Hund": df = 5
        // Doc 1: "Die Katze saß auf dem Dach" (doc_len = 6, Katze tf = 1, Hund tf = 0)
        // Doc 2: "Die Katze und der Hund auf dem Dach" (doc_len = 8, Katze tf = 1, Hund tf = 1)
        let score_katze = score_term(1, 6, 10.0, 5, 100);
        let score_hund = score_term(1, 8, 10.0, 5, 100);

        let score_1 = score_katze; // 1 term matched
        let score_2 = score_katze + score_hund; // 2 terms matched

        assert!(
            score_2 > score_1,
            "Mehr Term-Matches müssen höheren Score geben (score_2: {}, score_1: {})",
            score_2,
            score_1
        );
    }
}
