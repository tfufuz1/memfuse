//! Pure BM25 scoring functions.

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
    // Constant parameters for BM25
    let k1 = 1.2;
    let b = 0.75;

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

    let norm_doc_len = if avg_doc_len > 0.0 {
        doc_len / avg_doc_len
    } else {
        1.0
    };

    let tf_numerator = tf * (k1 + 1.0);
    let tf_denominator = tf + k1 * (1.0 - b + b * norm_doc_len);

    // tf_denominator is guaranteed to be >= 0.3 since k1=1.2, b=0.75, tf>=0, norm_doc_len>=0
    // 1.2 * (0.25 + 0.75 * norm_doc_len) >= 0.3
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
}

#[cfg(test)]
mod stability_tests {
    use super::*;

    #[test]
    fn test_bm25_empty_index_stability() {
        let score = score_term(0, 0, 0.0, 0, 0);
        assert!(!score.is_nan());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_bm25_div_by_zero_potential() {
        let score = score_term(1, 10, 10.0, 1, 0);
        assert!(!score.is_nan());
        assert_eq!(score, 0.0);

        let score2 = score_term(1, 10, 0.0, 1, 10);
        assert!(!score2.is_nan());
        assert!(score2 > 0.0);
    }
}
