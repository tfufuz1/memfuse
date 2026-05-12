//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

static STOPWORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn get_stopwords() -> &'static HashSet<String> {
    STOPWORDS.get_or_init(|| {
        let words = vec![
            "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is",
            "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there",
            "these", "they", "this", "to", "was", "will", "with", "i", "me", "my", "we", "our",
            "you", "your", "he", "she", "his", "her", "it", "its", "they", "them", "their",
            "der", "die", "das", "ein", "eine", "einer", "eines", "dem", "den", "des", "am", "im",
            "in", "an", "zu", "für", "und", "oder", "ist", "sind", "war", "von", "mit", "auf", "über",
        ];
        words.into_iter().map(|w| w.to_string()).collect()
    })
}

/// Tokenizes text into lowercase words using Unicode word boundaries and filters stopwords.
pub fn tokenize(text: &str) -> Vec<String> {
    let stopwords = get_stopwords();
    text.unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| !stopwords.contains(w))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_handles_unicode() {
        let text = "Ärger über Ölpreise";
        let tokens = tokenize(text);
        // "über" is in our stopword list now
        assert_eq!(tokens, vec!["ärger", "ölpreise"]);
    }

    #[test]
    fn test_tokenizer_filters_stopwords() {
        let text = "The quick brown fox jumps over the lazy dog";
        let tokens = tokenize(text);
        assert!(!tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
    }
}
