//! Tokenizer using `unicode-segmentation`.

use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// A simple list of English and German stopwords.
const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "but",
    "by",
    "for",
    "if",
    "in",
    "into",
    "is",
    "it",
    "no",
    "not",
    "of",
    "on",
    "or",
    "such",
    "that",
    "the",
    "their",
    "then",
    "there",
    "these",
    "they",
    "this",
    "to",
    "was",
    "will",
    "with",
    "i",
    "me",
    "my",
    "myself",
    "we",
    "our",
    "ours",
    "ourselves",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
    "he",
    "him",
    "his",
    "himself",
    "she",
    "her",
    "hers",
    "herself",
    "it",
    "its",
    "itself",
    "them",
    "their",
    "theirs",
    "themselves",
    "what",
    "which",
    "who",
    "whom",
    "this",
    "that",
    "these",
    "those",
    "am",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "having",
    "do",
    "does",
    "did",
    "doing",
    "would",
    "should",
    "could",
    "ought",
    "im",
    "youre",
    "hes",
    "shes",
    "its",
    "were",
    "theyre",
    "ive",
    "youve",
    "weve",
    "theyve",
    "id",
    "youd",
    "hed",
    "shed",
    "itd",
    "wed",
    "theyd",
    "ill",
    "youll",
    "hell",
    "shell",
    "itll",
    "well",
    "theyll",
    "isnt",
    "arent",
    "wasnt",
    "werent",
    "hasnt",
    "havent",
    "hadnt",
    "doesnt",
    "dont",
    "didnt",
    "wont",
    "shant",
    "shouldnt",
    "wouldnt",
    "cant",
    "couldnt",
    "mustnt",
    "lets",
    "thats",
    "whos",
    "whats",
    "heres",
    "theres",
    "whens",
    "wheres",
    "whys",
    "hows",
    "der",
    "die",
    "das",
    "und",
    "ist",
    "ein",
    "eine",
    "mit",
    "für",
    "von",
    "zu",
    "auf",
    "den",
    "dem",
    "nicht",
    "auch",
    "als",
    "im",
    "des",
    "am",
    "über",
    "mit",
    "einer",
    "einem",
    "einen",
    "eines",
];

static STOPWORDS_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| STOPWORDS.iter().copied().collect());

/// Tokenizes text into lowercase words, filters stopwords, and applies basic stemming.
pub fn tokenize(text: &str) -> Vec<String> {
    text.unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS_SET.contains(w.as_str()))
        .map(stem)
        .collect()
}

/// A basic suffix-stripping stemming algorithm.
fn stem(word: String) -> String {
    if word.len() <= 3 {
        return word;
    }

    if word.ends_with("ing") {
        word[..word.len() - 3].to_string()
    } else if word.ends_with("ies") {
        let mut s = word[..word.len() - 3].to_string();
        s.push('y');
        s
    } else if word.ends_with("ed") {
        word[..word.len() - 2].to_string()
    } else if word.ends_with('s') && !word.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_handles_unicode() {
        let text = "Ärger über Ölpreise";
        let tokens = tokenize(text);
        // "über" is not in our small stopword list currently, let's check what it returns
        // "ölpreise" ends with 'e', my simple stemmer doesn't handle 'e' yet.
        // If I add 'über' to stopwords:
        assert!(tokens.contains(&"ärger".to_string()));
        assert!(
            tokens.contains(&"ölpreise".to_string()) || tokens.contains(&"ölpreis".to_string())
        );
    }

    #[test]
    fn test_tokenizer_stopwords() {
        let text = "The quick brown fox jumps over the lazy dog";
        let tokens = tokenize(text);
        assert!(!tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
    }

    #[test]
    fn test_tokenizer_stemming() {
        assert_eq!(stem("jumping".to_string()), "jump");
        assert_eq!(stem("flies".to_string()), "fly");
        assert_eq!(stem("walked".to_string()), "walk");
        assert_eq!(stem("dogs".to_string()), "dog");
        // Ensure "less" doesn't become "les"
        assert_eq!(stem("less".to_string()), "less");
    }
}
