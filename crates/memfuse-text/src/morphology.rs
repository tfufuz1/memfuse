//! Morphologische Inferenz-Optimierung (WP-6.5).
//!
//! Sprachbewusste Tokenisierung für europäische Sprachen.
//! Compound-Splitting für Deutsch zur Token-Reduktion.

// ANCHOR:ARCH:MORPH-001 — Morphologische Inferenz-Optimierung (WP-6.5)
// WP:WP-6.5 PRIO:2 NEEDS:WP-2.1
// STATUS:DONE DATE:2026-05-23

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

/// German compound word splitter.
///
/// Uses dictionary-based approach with longest-match strategy.
/// Fallback: returns the original token unsplit.
pub struct GermanCompoundSplitter {
    /// Minimum component length for splitting.
    min_component_len: usize,
    /// Static dictionary for splitting.
    dictionary: Vec<&'static str>,
}

impl GermanCompoundSplitter {
    /// Creates a new German compound splitter with default dictionary.
    pub fn new() -> Self {
        let mut dictionary = vec![
            "bundes",
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
            "ordnung",
            "verordnung",
            "vertrag",
            "recht",
            "kauf",
            "haus",
            "staat",
            "gesellschaft",
            "grund",
            "arbeit",
            "leistung",
            "hilfe",
            "steuer",
            "zoll",
            "bund",
            "land",
            "kreis",
            "stadt",
            "gemeinde",
        ];
        // Sort by length descending for longest-match strategy
        dictionary.sort_by_key(|b| std::cmp::Reverse(b.len()));

        Self {
            min_component_len: 3,
            dictionary,
        }
    }

    /// Creates a splitter with custom minimum component length.
    pub fn with_min_length(min_len: usize) -> Self {
        let mut s = Self::new();
        s.min_component_len = min_len;
        s
    }

    /// Returns the minimum component length.
    pub fn min_component_len(&self) -> usize {
        self.min_component_len
    }
}

impl Default for GermanCompoundSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphologicalTokenizer for GermanCompoundSplitter {
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str> {
        if token.len() <= self.min_component_len {
            return vec![token];
        }

        // Longest-match strategy: try to find the longest matching prefix from dictionary
        for &word in &self.dictionary {
            if token.len() > word.len() && token.starts_with(word) {
                let rest = &token[word.len()..];

                // Handle Fugen-s (e.g., Verfassung-s-gericht, Daten-schutz-verordnung)
                let mut actual_rest = rest;
                if rest.starts_with('s') && rest.len() > self.min_component_len {
                    // Only strip 's' if the rest doesn't already match a dictionary word
                    // and the stripped version DOES match one.
                    let mut rest_matches = false;
                    for &w in &self.dictionary {
                        if rest.starts_with(w) {
                            rest_matches = true;
                            break;
                        }
                    }

                    if !rest_matches {
                        let tentative_rest = &rest[1..];
                        for &w in &self.dictionary {
                            if tentative_rest.starts_with(w) {
                                actual_rest = tentative_rest;
                                break;
                            }
                        }
                    }
                }

                if actual_rest.len() >= self.min_component_len {
                    let mut result = vec![&token[..word.len()]];
                    result.extend(self.decompose(actual_rest));
                    return result;
                }
            }
        }

        vec![token]
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
    fn test_german_splitter_verification() {
        let splitter = GermanCompoundSplitter::new();
        // The splitter expects lowercase input as per tokenize() implementation
        let result = splitter.decompose("bundesverfassungsgericht");
        assert_eq!(result, vec!["bundes", "verfassungs", "gericht"]);
        assert_eq!(splitter.language(), "de");
    }

    #[test]
    fn test_german_morph_tokenizer_expanded() {
        let splitter = GermanCompoundSplitter::new();
        // WP-6.5 expansion verification
        let result = splitter.decompose("kaufvertrag");
        assert_eq!(result, vec!["kauf", "vertrag"]);
        let result = splitter.decompose("datenschutzverordnung");
        assert_eq!(result, vec!["daten", "schutz", "verordnung"]);
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

        println!("Expansion Ratio: {}", metrics.expansion_ratio());
        assert!(metrics.expansion_ratio() > 1.2, "Expansion should be > 20%");
    }
}
