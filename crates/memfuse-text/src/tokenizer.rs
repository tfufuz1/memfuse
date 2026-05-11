//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

/// Trait for text tokenization.
pub trait Tokenizer: Send + Sync {
    /// Tokenizes the input text.
    fn tokenize(&self, text: &str) -> Vec<String>;
}

/// Standard tokenizer with lowercase and stopword filtering.
pub struct StandardTokenizer;

static STOPWORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn get_stopwords() -> &'static HashSet<String> {
    STOPWORDS.get_or_init(|| {
        let mut stopwords = HashSet::new();
        // English stopwords
        for w in [
            "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "i", "if", "in", "into",
            "is", "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then",
            "there", "these", "they", "this", "to", "was", "will", "with",
        ] {
            stopwords.insert(w.to_string());
        }
        // German stopwords
        for w in [
            "der", "die", "das", "ein", "eine", "einer", "einem", "einen", "eines", "und", "oder",
            "aber", "denn", "da", "weil", "wenn", "als", "dass", "daß", "ist", "sind", "war",
            "wurde", "werden", "von", "zu", "mit", "für", "auf", "im", "in", "dem", "den", "des",
            "am",
        ] {
            stopwords.insert(w.to_string());
        }
        stopwords
    })
}

impl StandardTokenizer {
    /// Basic morphological compound splitter for German (WP-6.5 foundation).
    /// Currently just a placeholder that could be expanded with a dictionary.
    pub fn split_compounds(&self, token: &str) -> Vec<String> {
        // Placeholder for morphological inference optimization
        vec![token.to_string()]
    }
}

impl Tokenizer for StandardTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stopwords = get_stopwords();
        text.unicode_words()
            .map(|w| w.to_lowercase())
            .filter(|w| !stopwords.contains(w))
            .flat_map(|w| self.split_compounds(&w))
            .collect()
    }
}

/// Legacy helper for tokenization.
pub fn tokenize(text: &str) -> Vec<String> {
    StandardTokenizer.tokenize(text)
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
        let text = "The quick brown fox and the lazy dog";
        let tokens = tokenize(text);
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"and".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
    }

    #[test]
    fn test_tokenizer_german_stopwords() {
        let text = "Der schnelle braune Fuchs und der faule Hund";
        let tokens = tokenize(text);
        assert!(!tokens.contains(&"der".to_string()));
        assert!(!tokens.contains(&"und".to_string()));
        assert!(tokens.contains(&"schnelle".to_string()));
    }
}
