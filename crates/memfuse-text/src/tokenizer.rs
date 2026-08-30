// FILE-CONTEXT: Tokenisierung & Stopword-Filterung.
// ZWECK: Zerschneidet Eingabetexte in normalisierte Wort-Tokens mit optionaler deutscher Morphologie.
// INVARIANTEN: Tokenisierung muss deterministisch zwischen Indexierung und Query-Pfad identisch sein.
// NICHT-OFFENSICHTLICH: Stoppwörter werden per OnceLock geladen; DefaultTokenizer filtert Alphatoken + Stoppwörter.
// HOTSPOTS: tokenize, DefaultTokenizer::tokenize, GermanMorphTokenizer::tokenize
// STAND: TS:2026-08-30T22:01:55Z (SESSION: cf1f75c6)

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

/// German tokenizer with morphological compound splitting.
pub struct GermanMorphTokenizer {
    splitter: crate::morphology::GermanCompoundSplitter,
}

impl GermanMorphTokenizer {
    pub fn new() -> Self {
        Self {
            splitter: crate::morphology::GermanCompoundSplitter::new(),
        }
    }
}

impl Default for GermanMorphTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for GermanMorphTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        use crate::morphology::{normalize_umlauts, MorphologicalTokenizer};
        let stopwords = get_stopwords();
        let mut tokens = Vec::new();

        for word in text.unicode_words() {
            let lower = word.to_lowercase();
            let normalized = normalize_umlauts(&lower);
            if stopwords.contains(&lower) || stopwords.contains(&normalized) {
                continue;
            }

            let components = if normalized.chars().all(|c| !c.is_uppercase()) {
                self.splitter.decompose(&normalized)
            } else {
                vec![normalized.as_str()]
            };
            if components.len() > 1 {
                // Collect component strings first to avoid borrow issues with lower
                let mut comp_strs: Vec<String> = Vec::new();
                for c in &components {
                    comp_strs.push(c.to_string());
                    let norm_c = normalize_umlauts(c);
                    if norm_c != *c {
                        comp_strs.push(norm_c);
                    }
                }
                // Keep original compound for exact matches
                tokens.push(lower.clone());
                if normalized != lower {
                    tokens.push(normalized);
                }
                // Add decomposed components for recall
                tokens.extend(comp_strs);
            } else {
                tokens.push(lower.clone());
                if normalized != lower {
                    tokens.push(normalized);
                }
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
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn tokenizer_never_panics(s in ".*") {
            let _ = DefaultTokenizer.tokenize(&s);
        }

        #[test]
        fn german_tokenizer_never_panics(s in ".*") {
            let _ = GermanMorphTokenizer::new().tokenize(&s);
        }
    }

    #[test]
    fn test_tokenizer_edge_cases_bounded() {
        let long_str = "a".repeat(100_000);
        let edge_cases = ["", " \t\n", "🦀🦀", long_str.as_str(), "\0", "ÄÖÜß"];
        let german_tok = GermanMorphTokenizer::new();
        for input in &edge_cases {
            let tokens = DefaultTokenizer.tokenize(input);
            assert!(tokens.len() <= input.chars().count() + 1);
            let g_tokens = german_tok.tokenize(input);
            let _ = g_tokens;
        }
    }

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
        let tokenizer = GermanMorphTokenizer::new();
        let tokens = tokenizer.tokenize("Das Bundesverfassungsgericht");
        // "Das" is stopword
        assert!(tokens.contains(&"bundesverfassungsgericht".to_string()));
        assert!(tokens.contains(&"gericht".to_string()));
    }

    #[test]
    fn german_morph_tokenizer_case_default_constructor() {
        let tokenizer = GermanMorphTokenizer::default();
        let tokens = tokenizer.tokenize("Datenschutzrichtlinie");
        assert!(!tokens.is_empty());
        assert!(tokens.contains(&"datenschutzrichtlinie".to_string()));
    }

    #[test]
    fn test_tokenizers_panic_free_edge_cases() {
        let long_string = "a".repeat(100_000);
        let edge_cases = ["", " ", ".", "🦀", long_string.as_str()];
        let german_tokenizer = GermanMorphTokenizer::new();

        for text in &edge_cases {
            let default_tokens = tokenize(text);
            assert!(
                default_tokens.len() <= text.len(),
                "DefaultTokenizer token count must not exceed text length"
            );

            let german_tokens = german_tokenizer.tokenize(text);
            // German morph tokenizer may expand compounds, but for these edge cases token count is within reasonable limits
            assert!(
                !german_tokens.is_empty()
                    || text.trim().is_empty()
                    || text == &"."
                    || text == &"🦀",
                "GermanMorphTokenizer must complete without panic for edge case input"
            );
        }
    }
}
