//! Pure BM25 scoring functions.

/// Default k1 parameter for BM25 term frequency saturation scaling.
pub const BM25_K1: f32 = 1.2;
/// Default b parameter for BM25 document length normalization penalty tuning.
pub const BM25_B: f32 = 0.75;

/// Calculates the BM25 score for a single term in a document.
///
/// # Arguments
/// * `tf` - Term frequency in the document
/// * `doc_len` - Length of the document (number of tokens)
/// * `avg_doc_len` - Average document length in the collection
/// * `df` - Document frequency (number of documents containing the term)
/// * `n` - Total number of documents in the collection
pub fn score_term(tf: u32, doc_len: u32, avg_doc_len: f32, df: u32, n: u32) -> f32 {
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

    let tf_numerator = tf * (BM25_K1 + 1.0);
    let tf_denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * norm_doc_len);

    idf * (tf_numerator / tf_denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(score.is_finite(), "BM25 must return finite score for extreme tf");
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
