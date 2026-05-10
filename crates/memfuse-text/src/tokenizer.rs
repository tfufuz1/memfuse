//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

/// Tokenizes text into lowercase words using Unicode word boundaries.
pub fn tokenize(text: &str) -> Vec<String> {
    let stopwords: HashSet<&str> = [
        "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is",
        "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there",
        "these", "they", "this", "to", "was", "will", "with",
    ]
    .into_iter()
    .collect();

    text.unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| !stopwords.contains(w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_handles_unicode() {
        let text = "Ärger über Ölpreise";
        let tokens = tokenize(text);
        assert_eq!(tokens, vec!["ärger", "über", "ölpreise"]);
    }

    #[test]
    fn test_tokenizer_filters_stopwords() {
        let text = "This is a test and it should work.";
        let tokens = tokenize(text);
        // "this", "is", "a", "and", "it", "should", "work"
        // stopwords: "this", "is", "a", "and", "it"
        // remaining: "test", "should", "work"
        assert_eq!(tokens, vec!["test", "should", "work"]);
    }
}
