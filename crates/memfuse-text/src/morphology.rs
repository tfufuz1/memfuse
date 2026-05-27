//! Morphologische Inferenz-Optimierung (WP-6.5).
//!
//! Sprachbewusste Tokenisierung für europäische Sprachen.
//! Compound-Splitting für Deutsch zur Token-Reduktion.

// ANCHOR:ARCH:MORPH-001 — Morphologische Inferenz-Optimierung (WP-6.5)
// WP:WP-6.5 PRIO:2 NEEDS:WP-2.1
// STATUS:DONE DATE:2026-05-17

/// Trait for morphological tokenization.
///
/// Decomposes compound words into constituent morphemes.
/// Example: "Bundesverfassungsgericht" -> ["Bundes", "verfassungs", "gericht"]
pub trait MorphologicalTokenizer: Send + Sync {
    /// Decomposes a token into its morphological components.
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str>;

    /// Returns the language code of this tokenizer (e.g. "de", "en").
    fn language(&self) -> &str;
}

use std::collections::HashSet;
use std::sync::OnceLock;

static GERMAN_DICT: OnceLock<HashSet<&'static str>> = OnceLock::new();

fn get_german_dict() -> &'static HashSet<&'static str> {
    GERMAN_DICT.get_or_init(|| {
        [
            "bundes",
            "verfassung",
            "verfassungs",
            "gericht",
            "gesetz",
            "entwurf",
            "daten",
            "bank",
            "speicher",
            "vektor",
            "suche",
            "system",
            "steuerung",
            "verwaltung",
            "bericht",
            "prüfung",
            "schutz",
            "sicherheit",
            "zugriff",
            "rechte",
            "grund",
            "buch",
            "anwendung",
            "benutzer",
            "dienst",
            "leistung",
            "arbeit",
            "markt",
            "wirtschaft",
            "finanz",
            "haushalt",
            "steuer",
            "recht",
            "ordnung",
            "haftung",
            "vertrag",
        ]
        .into_iter()
        .collect()
    })
}

/// German compound word splitter.
///
/// Uses dictionary-based greedy approach.
/// Fallback: returns the original token unsplit.
pub struct GermanCompoundSplitter {
    /// Minimum component length for splitting.
    min_component_len: usize,
}

impl GermanCompoundSplitter {
    /// Creates a new German compound splitter.
    pub fn new() -> Self {
        Self {
            min_component_len: 3,
        }
    }

    /// Creates a splitter with custom minimum component length.
    pub fn with_min_length(min_len: usize) -> Self {
        Self {
            min_component_len: min_len,
        }
    }

    /// Returns the minimum component length.
    pub fn min_component_len(&self) -> usize {
        self.min_component_len
    }

    /// Recursive greedy splitting.
    fn greedy_split<'a>(&self, token: &'a str) -> Vec<&'a str> {
        if token.len() <= self.min_component_len {
            return vec![token];
        }

        let dict = get_german_dict();

        // Try longest prefix match first (greedy)
        for i in (self.min_component_len..=token.len()).rev() {
            if !token.is_char_boundary(i) {
                continue;
            }
            let prefix = &token[..i];
            if dict.contains(prefix) {
                let rest = &token[i..];

                if rest.is_empty() {
                    return vec![prefix];
                }

                // Handle Fugen-s
                let actual_rest = if rest.starts_with('s') && rest.len() > self.min_component_len
                {
                    &rest[1..]
                } else {
                    rest
                };

                if actual_rest.len() >= self.min_component_len {
                    let mut components = vec![prefix];
                    let rest_split = self.greedy_split(actual_rest);
                    // Only accept the split if the rest could also be split or is in dict
                    if rest_split.len() > 1 || dict.contains(actual_rest) {
                        components.extend(rest_split);
                        return components;
                    }
                }
            }
        }

        vec![token]
    }
}

impl Default for GermanCompoundSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphologicalTokenizer for GermanCompoundSplitter {
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str> {
        self.greedy_split(token)
    }

    fn language(&self) -> &str {
        "de"
    }
}

/// Passthrough tokenizer for languages without compound words.
pub struct PassthroughTokenizer {
    lang: String,
}

impl PassthroughTokenizer {
    /// Creates a passthrough tokenizer for the given language.
    pub fn new(lang: impl Into<String>) -> Self {
        Self { lang: lang.into() }
    }
}

impl MorphologicalTokenizer for PassthroughTokenizer {
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str> {
        vec![token]
    }

    fn language(&self) -> &str {
        &self.lang
    }
}

/// Metrics for measuring token reduction effectiveness.
#[derive(Debug, Clone, Copy)]
pub struct TokenReductionMetrics {
    /// Number of original tokens before morphological decomposition.
    pub original_tokens: usize,
    /// Number of tokens after decomposition.
    pub decomposed_tokens: usize,
}

impl TokenReductionMetrics {
    /// Computes the token reduction ratio.
    ///
    /// A ratio > 1.0 means decomposition increased tokens (expected for compounds).
    /// Target: > 20% token-count increase for German technical texts
    /// (which leads to better BM25 recall).
    pub fn expansion_ratio(&self) -> f32 {
        if self.original_tokens == 0 {
            return 0.0;
        }
        self.decomposed_tokens as f32 / self.original_tokens as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_german_splitter_basic() {
        let splitter = GermanCompoundSplitter::new();
        // Now it should split!
        let result = splitter.decompose("bundesverfassungsgericht");
        assert_eq!(result, vec!["bundes", "verfassungs", "gericht"]);
        assert_eq!(splitter.language(), "de");
    }

    #[test]
    fn test_passthrough_tokenizer() {
        let tok = PassthroughTokenizer::new("en");
        assert_eq!(tok.decompose("hello"), vec!["hello"]);
        assert_eq!(tok.language(), "en");
    }

    #[test]
    fn test_german_expansion_ratio() {
        let splitter = GermanCompoundSplitter::new();
        let text = "Das Bundesverfassungsgericht prüft den Gesetzentwurf zur Datensicherheit.";
        let words: Vec<&str> = text
            .split_whitespace()
            .map(|w| w.trim_matches('.'))
            .collect();

        let mut original_count = 0;
        let mut decomposed_count = 0;

        for word in words {
            let lower = word.to_lowercase();
            let components = splitter.decompose(&lower);
            original_count += 1;
            decomposed_count += components.len();
        }

        let metrics = TokenReductionMetrics {
            original_tokens: original_count,
            decomposed_tokens: decomposed_count,
        };

        // Bundesverfassungsgericht -> [bundes, verfassungs, gericht] (+2)
        // Gesetzentwurf -> [gesetz, entwurf] (+1)
        // Datensicherheit -> [daten, sicherheit] (+1)
        // Total original: 8
        // Total decomposed: 8 + 2 + 1 + 1 = 12
        // Ratio: 12/8 = 1.5 (+50%)

        println!("Expansion Ratio: {}", metrics.expansion_ratio());
        assert!(metrics.expansion_ratio() > 1.2, "Expansion should be > 20%");
    }
}
