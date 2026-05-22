//! Pure BM25 scoring functions.
// AGENT:05 STATUS:DONE DATE:2026-05-21

/// Calculates the BM25 score for a single term in a document.
///
/// # Arguments
/// * `tf` - Term frequency in the document
/// * `doc_len` - Length of the document (number of tokens)
/// * `avg_doc_len` - Average document length in the collection
/// * `df` - Document frequency (number of documents containing the term)
/// * `n` - Total number of documents in the collection
pub fn score_term(tf: u32, doc_len: u32, avg_doc_len: f32, df: u32, n: u32) -> f32 {
    // Constant parameters for BM25
    let k1 = 1.2;
    let b = 0.75;

    let tf = tf as f32;
    let doc_len = doc_len as f32;
    let df = df as f32;
    let n = n as f32;

    // Standard IDF formula, ensure we don't return negative IDF
    // If a term appears in more than half of the documents, the raw IDF formula can be negative.
    // We cap it at 0.01 to ensure terms always have some positive weight.
    let idf = f32::max(0.01, ((n - df + 0.5) / (df + 0.5)).ln());

    let norm_doc_len = if avg_doc_len > 0.0 {
        doc_len / avg_doc_len
    } else {
        1.0
    };

    let tf_numerator = tf * (k1 + 1.0);
    let tf_denominator = tf + k1 * (1.0 - b + b * norm_doc_len);

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
}
