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
            "über", "als", "auch", "bei", "bin", "bis", "bist", "da", "dadurch", "daher", "darum",
            "das", "daß", "dass", "dein", "deine", "dem", "den", "der", "des", "dessen", "deshalb",
            "die", "dies", "dieser", "dieses", "doch", "dort", "du", "durch", "ein", "eine",
            "einem", "einen", "einer", "eines", "er", "es", "euer", "eure", "für", "hatte",
            "hatten", "hattest", "hattet", "hier", "hinter", "ich", "ihm", "ihn", "ihr", "ihre",
            "im", "in", "ist", "ja", "jede", "jedem", "jeden", "jeder", "jedes", "jener", "jenes",
            "jetzt", "kann", "können", "könnte", "machen", "man", "mein", "meine", "mit", "muß",
            "musst", "müssen", "müßt", "nach", "nachdem", "nein", "nicht", "nun", "oder", "seid",
            "sein", "seine", "sich", "sie", "sind", "soll", "sollen", "sollte", "sollten", "sonst",
            "soweit", "sowie", "und", "unser", "unsere", "unter", "vom", "von", "vor", "wann",
            "war", "waren", "warst", "wart", "was", "weg", "weil", "weiter", "welche", "welchem",
            "welchen", "welcher", "welches", "wenn", "werde", "werden", "wie", "wir", "wo", "wollen",
            "wollte", "wollten", "während", "würde", "würden", "zu", "zum", "zur", "zwar", "zwischen",
            "about", "above", "after", "again", "against", "all", "am", "an", "any", "are", "aren't",
            "as", "at", "be", "because", "been", "before", "being", "below", "between", "both",
            "but", "by", "can't", "cannot", "could", "couldn't", "did", "didn't", "do", "does",
            "doesn't", "doing", "don't", "down", "during", "each", "few", "further", "had",
            "hadn't", "has", "hasn't", "have", "haven't", "having", "he", "he'd", "he'll", "he's",
            "her", "here", "here's", "hers", "herself", "him", "himself", "his", "how", "how's",
            "i'd", "i'll", "i'm", "i've", "if", "in", "into", "is", "isn't", "it", "it's", "its",
            "itself", "let's", "me", "more", "most", "mustn't", "my", "myself", "no", "nor", "not",
            "of", "off", "on", "once", "only", "or", "other", "ought", "our", "ours", "ourselves",
            "out", "over", "own", "same", "shan't", "she", "she'd", "she'll", "she's", "should",
            "shouldn't", "so", "some", "such", "than", "that", "that's", "the", "their", "theirs",
            "them", "themselves", "then", "there", "there's", "these", "they", "they'd", "they'll",
            "they're", "they've", "this", "those", "through", "to", "too", "under", "until", "up",
            "very", "was", "wasn't", "we", "we'd", "we'll", "we're", "we've", "were", "weren't",
            "what", "what's", "when", "when's", "where", "where's", "which", "while", "who",
            "who's", "whom", "why", "why's", "with", "won't", "would", "wouldn't", "you", "you'd",
            "you'll", "you're", "you've", "your", "yours", "yourself", "yourselves",
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

            // POC for compound splitting and suffix extraction
            // e.g., "Bundesverfassungsgericht" -> ["bundesverfassungsgericht", "gericht"]
            let mut split = false;
            for suffix in &["gericht", "schaft", "keit", "ung", "ismus"] {
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
    }
}
