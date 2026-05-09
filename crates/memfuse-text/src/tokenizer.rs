// ANCHOR:ARCH:TOKENIZER-001 — Standard Tokenizer via unicode-segmentation.
// ZIEL: Konsistente Tokenisierung für BM25 (Lowercase, Word-Boundaries).
//! Simple tokenizer using unicode-segmentation.

use unicode_segmentation::UnicodeSegmentation;

/// Standard tokenizer for MemFuse.
pub struct Tokenizer;

impl Tokenizer {
    /// Tokenizes a string into a vector of lowercase tokens.
    pub fn tokenize(text: &str) -> Vec<String> {
        text.unicode_words()
            .map(|word| word.to_lowercase())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_basic() {
        let text = "Rust programming is fun!";
        let tokens = Tokenizer::tokenize(text);
        assert_eq!(tokens, vec!["rust", "programming", "is", "fun"]);
    }

    #[test]
    fn test_tokenizer_unicode() {
        let text = "Ärger über Ölpreise";
        let tokens = Tokenizer::tokenize(text);
        assert_eq!(tokens, vec!["ärger", "über", "ölpreise"]);
    }
}
