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

        for word in text.unicode_words() {
            let lower = word.to_lowercase();
            if stopwords.contains(&lower) {
                continue;
            }

            // POC for compound splitting: "gericht", "amt", "haus", "stelle", "wesen"
            // e.g., "Bundesverfassungsgericht" -> ["bundesverfassungsgericht", "gericht"]
            let mut split = false;
            for suffix in &["gericht", "amt", "haus", "stelle", "wesen"] {
                if lower.ends_with(suffix) && lower.len() > suffix.len() + 2 {
                    tokens.push(lower.clone());
                    tokens.push(suffix.to_string());
                    split = true;
                    break;
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

        let tokens = tokenizer.tokenize("Finanzamt");
        assert!(tokens.contains(&"finanzamt".to_string()));
        assert!(tokens.contains(&"amt".to_string()));

        let tokens = tokenizer.tokenize("Krankenhaus");
        assert!(tokens.contains(&"krankenhaus".to_string()));
        assert!(tokens.contains(&"haus".to_string()));

        let tokens = tokenizer.tokenize("Arbeitsstelle");
        assert!(tokens.contains(&"arbeitsstelle".to_string()));
        assert!(tokens.contains(&"stelle".to_string()));

        let tokens = tokenizer.tokenize("Bildungswesen");
        assert!(tokens.contains(&"bildungswesen".to_string()));
        assert!(tokens.contains(&"wesen".to_string()));
    }
}
