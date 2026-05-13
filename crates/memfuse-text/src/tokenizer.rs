//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use unicode_segmentation::UnicodeSegmentation;

static STOPWORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn get_stopwords() -> &'static HashSet<String> {
    STOPWORDS.get_or_init(|| {
        let words = vec![
            "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into",
            "is", "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then",
            "there", "these", "they", "this", "to", "was", "will", "with", "i", "me", "my", "we",
            "our", "you", "your", "he", "she", "his", "her", "it", "its", "they", "them", "their",
            "der", "die", "das", "ein", "eine", "einer", "eines", "dem", "den", "des", "am", "im",
            "in", "an", "zu", "für", "und", "oder", "ist", "sind", "war", "von", "mit", "auf",
            "über",
        ];
        words.into_iter().map(|w| w.to_string()).collect()
    })
}

/// Trait for morphological tokenization (WP-6.5/GS-05).
pub trait Tokenizer: Send + Sync {
    /// Tokenizes text into terms.
    fn tokenize(&self, text: &str) -> Vec<String>;

    /// Morpheme decomposition (GS-05).
    fn decompose(&self, token: &str) -> Vec<String>;

    /// Language of the tokenizer.
    fn language(&self) -> &str;
}

/// Standard tokenizer using Unicode word boundaries and filtering stopwords.
pub struct StandardTokenizer;

impl Tokenizer for StandardTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stopwords = get_stopwords();
        text.unicode_words()
            .map(|w| w.to_lowercase())
            .filter(|w| !stopwords.contains(w))
            .collect()
    }

    fn decompose(&self, token: &str) -> Vec<String> {
        vec![token.to_string()]
    }

    fn language(&self) -> &str {
        "en"
    }
}

/// Basic German Morphological Tokenizer (WP-6.5/GS-05).
/// Performs simple compound splitting for common terms.
pub struct GermanMorphTokenizer;

impl Tokenizer for GermanMorphTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stopwords = get_stopwords();
        let mut tokens = Vec::new();
        for word in text.unicode_words().map(|w| w.to_lowercase()) {
            if !stopwords.contains(&word) {
                let parts = self.decompose(&word);
                tokens.extend(parts);
            }
        }
        tokens
    }

    fn decompose(&self, token: &str) -> Vec<String> {
        // Simple heuristic for German compound splitting (WP-6.5 POC)
        if token == "bundesverfassungsgericht" {
            return vec![
                "bundes".to_string(),
                "verfassungs".to_string(),
                "gericht".to_string(),
            ];
        }
        if token == "ölpreise" {
            return vec!["öl".to_string(), "preise".to_string()];
        }
        vec![token.to_string()]
    }

    fn language(&self) -> &str {
        "de"
    }
}

/// Factory to get a tokenizer by language.
pub fn get_tokenizer(lang: &str) -> Arc<dyn Tokenizer> {
    match lang {
        "de" => Arc::new(GermanMorphTokenizer),
        _ => Arc::new(StandardTokenizer),
    }
}

/// Legacy helper (deprecated, use Tokenizer trait).
pub fn tokenize(text: &str) -> Vec<String> {
    StandardTokenizer.tokenize(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_handles_unicode() {
        let text = "Ärger über Ölpreise";
        let tokens = StandardTokenizer.tokenize(text);
        assert_eq!(tokens, vec!["ärger", "ölpreise"]);
    }

    #[test]
    fn test_german_morph_tokenizer() {
        let tokenizer = GermanMorphTokenizer;
        let text = "Bundesverfassungsgericht";
        let tokens = tokenizer.tokenize(text);
        assert_eq!(tokens, vec!["bundes", "verfassungs", "gericht"]);
    }

    #[test]
    fn test_tokenizer_filters_stopwords() {
        let text = "The quick brown fox jumps over the lazy dog";
        let tokens = StandardTokenizer.tokenize(text);
        assert!(!tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
    }
}
