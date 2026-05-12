//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

static STOPWORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn get_stopwords() -> &'static HashSet<String> {
    STOPWORDS.get_or_init(|| {
        let mut set = HashSet::new();
        // English stopwords
        let en = [
            "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "i", "if", "in", "into",
            "is", "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then",
            "there", "these", "they", "this", "to", "was", "will", "with",
        ];
        // German stopwords
        let de = [
            "aber", "als", "am", "an", "auch", "auf", "aus", "bei", "bin", "bis", "bist", "da",
            "dadurch", "daher", "darum", "das", "daß", "dass", "dein", "deine", "dem", "den",
            "der", "des", "dessen", "deshalb", "die", "dies", "dieser", "dieses", "doch", "dort",
            "du", "durch", "ein", "eine", "einem", "einen", "einer", "eines", "er", "es", "euer",
            "eure", "für", "hatte", "hatten", "hattest", "hattet", "hier", "hinter", "ich", "im",
            "in", "ist", "ja", "jede", "jedem", "jeden", "jeder", "jedes", "jener", "jenes",
            "jetzt", "kann", "kannst", "können", "könnt", "machen", "mein", "meine", "mit", "muß",
            "mußt", "müssen", "müsst", "nach", "nachdem", "nein", "nicht", "nun", "oder", "seid",
            "sein", "seine", "sich", "sie", "sind", "soll", "sollen", "sollst", "sollt", "sonst",
            "soweit", "sowie", "über", "und", "unser", "unsere", "unter", "von", "vor", "wann",
            "warum", "was", "weiter", "weitere", "wenn", "wer", "werde", "werden", "werdet",
            "weshalb", "wie", "wieder", "wieso", "wir", "wird", "wirst", "wo", "woher", "wohin",
            "zu", "zum", "zur", "zwar", "zwischen",
        ];

        for word in en.iter().chain(de.iter()) {
            set.insert(word.to_string());
        }
        set
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
        // "über" is in stopwords
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
