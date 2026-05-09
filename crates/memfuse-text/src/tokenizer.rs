//! Tokenizer using `unicode-segmentation`.

use unicode_segmentation::UnicodeSegmentation;

/// Tokenizes text into lowercase words using Unicode word boundaries.
pub fn tokenize(text: &str) -> Vec<String> {
    text.unicode_words().map(|w| w.to_lowercase()).collect()
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
}
