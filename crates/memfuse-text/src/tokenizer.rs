//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

static STOPWORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn get_stopwords() -> &'static HashSet<String> {
    STOPWORDS.get_or_init(|| {
        let words = vec![
            "a", "am", "an", "and", "are", "as", "at", "auf", "be", "but", "by", "das", "dem",
            "den", "der", "des", "die", "ein", "eine", "einer", "eines", "for", "für", "he", "her",
            "his", "i", "if", "im", "in", "into", "is", "ist", "it", "its", "me", "mit", "my",
            "no", "not", "oder", "of", "on", "or", "our", "she", "sind", "such", "that", "the",
            "their", "them", "then", "there", "these", "they", "this", "to", "über", "und", "von",
            "war", "was", "we", "will", "with", "you", "your", "zu",
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

            // German morphological splitting for common suffixes
            // e.g., "Bundesverfassungsgericht" -> ["bundesverfassungsgericht", "gericht"]
            // Splitting logic: if word ends with suffix AND stem (word - suffix) length > 3
            let mut split = false;
            for suffix in &["gericht", "schaft", "keit", "ung"] {
                if lower.ends_with(suffix) && lower.len() > suffix.len() + 3 {
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

/// Tokenizes text into lowercase words using Unicode word boundaries and
/// filters stopwords. Deprecated: Use `DefaultTokenizer.tokenize()` instead.
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
    }

    #[test]
    fn test_german_morph_expanded_splitting() {
        let tokenizer = GermanMorphTokenizer;

        // "schaft" splitting
        let tokens = tokenizer.tokenize("Wissenschaft");
        assert!(tokens.contains(&"wissenschaft".to_string()));
        assert!(tokens.contains(&"schaft".to_string()));

        // "keit" splitting
        let tokens = tokenizer.tokenize("Freundlichkeit");
        assert!(tokens.contains(&"freundlichkeit".to_string()));
        assert!(tokens.contains(&"keit".to_string()));

        // "ung" splitting
        let tokens = tokenizer.tokenize("Heizung");
        assert!(tokens.contains(&"heizung".to_string()));
        assert!(tokens.contains(&"ung".to_string()));

        // "gericht" splitting
        let tokens = tokenizer.tokenize("Amtsgericht");
        assert!(tokens.contains(&"amtsgericht".to_string()));
        assert!(tokens.contains(&"gericht".to_string()));
    }

    #[test]
    fn test_german_morph_stem_length_gate() {
        let tokenizer = GermanMorphTokenizer;

        // "Jung" ends with "ung", but stem "j" is length 1 <= 3. Should NOT split.
        let tokens = tokenizer.tokenize("Jung");
        assert_eq!(tokens, vec!["jung"]);

        // "Schwung" ends with "ung", stem "schw" is length 4 > 3. SHOULD split.
        let tokens = tokenizer.tokenize("Schwung");
        assert!(tokens.contains(&"schwung".to_string()));
        assert!(tokens.contains(&"ung".to_string()));
    }
}
