//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::OnceLock;
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

/// Tokenizer trait for different language-specific or morphological strategies.
pub trait Tokenizer: Send + Sync {
    /// Tokenizes text into lowercase words and filters stopwords.
    fn tokenize(&self, text: &str) -> Vec<String>;
}

/// Default tokenizer using Unicode word boundaries and generic stopwords.
pub struct DefaultTokenizer;

impl Tokenizer for DefaultTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stopwords = get_stopwords();
        text.unicode_words()
            .map(|w| w.to_lowercase())
            .filter(|w| !stopwords.contains(w))
            .collect()
    }
}

/// German tokenizer with basic compound splitting POC.
pub struct GermanMorphTokenizer;

impl Tokenizer for GermanMorphTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stopwords = get_stopwords();
        let mut tokens = Vec::new();
        let suffixes = ["gericht", "schaft", "keit", "ung"];

        for word in text.unicode_words() {
            let lower = word.to_lowercase();
            if stopwords.contains(&lower) {
                continue;
            }

            let mut split = false;
            for suffix in suffixes {
                if lower.ends_with(suffix) {
                    let word_char_count = lower.chars().count();
                    let suffix_char_count = suffix.chars().count();
                    if word_char_count > suffix_char_count + 3 {
                        tokens.push(lower.clone());
                        tokens.push(suffix.to_string());
                        split = true;
                        break;
                    }
                }
            }

            if !split {
                tokens.push(lower);
            }
        }
        tokens
    }
}

/// Tokenizes text into lowercase words using Unicode word boundaries and filters stopwords.
/// Deprecated: Use `DefaultTokenizer.tokenize()` instead.
pub fn tokenize(text: &str) -> Vec<String> {
    DefaultTokenizer.tokenize(text)
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

    #[test]
    fn test_german_morph_tokenizer() {
        let tokenizer = GermanMorphTokenizer;
        let tokens = tokenizer.tokenize("Das Bundesverfassungsgericht");
        // "Das" is stopword
        assert!(tokens.contains(&"bundesverfassungsgericht".to_string()));
        assert!(tokens.contains(&"gericht".to_string()));

        let tokens = tokenizer.tokenize("Wissenschaft und Gerechtigkeit");
        assert!(tokens.contains(&"wissenschaft".to_string()));
        assert!(tokens.contains(&"schaft".to_string()));
        assert!(tokens.contains(&"gerechtigkeit".to_string()));
        assert!(tokens.contains(&"keit".to_string()));

        let tokens = tokenizer.tokenize("Besserung");
        assert!(tokens.contains(&"besserung".to_string()));
        assert!(tokens.contains(&"ung".to_string()));

        // Test stem length constraint (> 3)
        // "Heilung" -> stem "heil" (length 4) -> split
        let tokens = tokenizer.tokenize("Heilung");
        assert!(tokens.contains(&"heilung".to_string()));
        assert!(tokens.contains(&"ung".to_string()));

        // "Übung" -> stem "üb" (length 2) -> NO split
        let tokens = tokenizer.tokenize("Übung");
        assert!(tokens.contains(&"übung".to_string()));
        assert!(!tokens.contains(&"ung".to_string()));

        // "Zeitung" -> stem "zeit" (length 4) -> split
        let tokens = tokenizer.tokenize("Zeitung");
        assert!(tokens.contains(&"zeitung".to_string()));
        assert!(tokens.contains(&"ung".to_string()));

        // "Dung" -> stem "d" (length 1) -> NO split
        let tokens = tokenizer.tokenize("Dung");
        assert!(tokens.contains(&"dung".to_string()));
        assert!(!tokens.contains(&"ung".to_string()));
    }
}
