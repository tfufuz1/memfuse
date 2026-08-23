//! Morphologische Inferenz-Optimierung (WP-6.5).
//!
//! Sprachbewusste Tokenisierung für europäische Sprachen.
//! Compound-Splitting für Deutsch zur Token-Reduktion.

// INVARIANT: Morphologische Inferenz-Optimierung (WP-6.5)

/// KMU-Fachvokabular — ergänzt das Basis-Wörterbuch für Unternehmenskontexte.
const KMU_DOMAIN_VOCABULARY: &[&str] = &[
    // Geschäftsprozesse
    "auftrags",
    "angebots",
    "rechnungs",
    "lieferungs",
    "bestellungs",
    "kunden",
    "lieferanten",
    "vertrags",
    "zahlungs",
    // HR
    "mitarbeiter",
    "personal",
    "urlaubs",
    "gehalts",
    "arbeits",
    "bewerbungs",
    "schulungs",
    // Logistik
    "lager",
    "bestands",
    "transport",
    "versand",
    "liefer",
    "fracht",
    // Produktion
    "fertigungs",
    "produktions",
    "qualitäts",
    "wartungs",
    "maschinen",
    "prüfungs",
    "prozess",
    // Compliance & Recht
    "datenschutz",
    "compliance",
    "richtlinie",
    "genehmigungs",
    "zertifizierungs",
    "haftungs",
    // Finanzen
    "finanz",
    "steuer",
    "buchhaltungs",
    "bilanz",
    "liquiditäts",
];

/// Normalisiert deutsche Umlaute für robusten Suchabgleich.
///
/// # Example
/// ```
/// use memfuse_text::morphology::{normalize_umlauts, GermanCompoundSplitter};
/// use memfuse_text::morphology::MorphologicalTokenizer;
///
/// let splitter = GermanCompoundSplitter::new();
/// let normalized = normalize_umlauts("Bundesverfassungsgericht");
/// // normalized is now "bundesverfassungsgericht"
/// let parts = splitter.decompose(&normalized);
/// // parts contains morphological components
/// ```
pub fn normalize_umlauts(input: &str) -> String {
    input
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
}

/// Trait for morphological tokenization.
///
/// Decomposes compound words into constituent morphemes.
///
/// # Input Contract
/// Input tokens MUST be lowercased before passing to `decompose`. Passing
/// mixed-case or uppercase tokens causes silent dictionary misses and returns
/// the original token without decomposition. Use `normalize_umlauts()` from
/// this module to prepare input correctly.
///
/// Example: "bundesverfassungsgericht" -> ["bundes", "verfassungs", "gericht"]
pub trait MorphologicalTokenizer: Send + Sync {
    /// Decomposes a token into its morphological components.
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str>;

    /// Returns the language code of this tokenizer (e.g. "de", "en").
    fn language(&self) -> &str;
}

/// German compound word splitter.
///
/// Uses dictionary-based + frequency statistics approach.
///
/// # Input Contract
/// Input tokens MUST be lowercased (and ideally normalized with
/// [`normalize_umlauts`]) before calling [`MorphologicalTokenizer::decompose`].
/// Uppercase input causes silent dictionary misses; there is no partial match.
///
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
}

impl Default for GermanCompoundSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphologicalTokenizer for GermanCompoundSplitter {
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str> {
        debug_assert!(
            token == token.to_lowercase(),
            "GermanCompoundSplitter::decompose received non-lowercase input: {:?}. \
             Call normalize_umlauts() or to_lowercase() before decompose().",
            token
        );

        // Simple recursive splitting based on a set of known components
        // and common German compound patterns (Fugen-S etc.)

        if token.len() <= self.min_component_len {
            return vec![token];
        }

        // Common components in technical/legal German compounds
        let dictionary = [
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
        ];

        let dictionary_iter = dictionary.iter().chain(KMU_DOMAIN_VOCABULARY.iter());

        for &word in dictionary_iter {
            let norm_word = normalize_umlauts(word);
            let matched_len = if token.starts_with(word) {
                Some(word.len())
            } else if norm_word != word && token.starts_with(&norm_word) {
                Some(norm_word.len())
            } else {
                None
            };

            if let Some(w_len) = matched_len {
                if token.len() > w_len {
                    let rest = &token[w_len..];

                    // Handle Fugen-s (e.g., Verfassung-s-gericht)
                    let actual_rest = if rest.starts_with('s') && rest.len() > 1 {
                        &rest[1..]
                    } else {
                        rest
                    };

                    if actual_rest.len() >= self.min_component_len {
                        let mut result = vec![&token[..w_len]];
                        result.extend(self.decompose(actual_rest));
                        return result;
                    }
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
    #[should_panic(expected = "non-lowercase input")]
    fn test_decompose_panics_on_uppercase_in_debug() {
        let splitter = GermanCompoundSplitter::new();
        // This must panic in debug mode because input is not lowercased.
        let _ = splitter.decompose("Bundesverfassungsgericht");
    }

    #[test]
    fn test_decompose_accepts_lowercase() {
        let splitter = GermanCompoundSplitter::new();
        // Must not panic — correctly lowercased input.
        let parts = splitter.decompose("bundesverfassungsgericht");
        // The splitter should return at least the original token as a fallback.
        assert!(!parts.is_empty());
    }

    #[test]
    fn test_normalize_umlauts_produces_valid_input_for_splitter() {
        let splitter = GermanCompoundSplitter::new();
        let tokens = ["Bundesverfassungsgericht", "Überwachungsgesetz", "Straße"];
        for token in &tokens {
            let normalized = normalize_umlauts(token);
            // After normalization, decompose must not panic.
            let parts = splitter.decompose(&normalized);
            assert!(
                !parts.is_empty(),
                "decompose must return at least one part for {:?}",
                token
            );
        }
    }

    #[test]
    fn test_german_splitter_scaffold() {
        let splitter = GermanCompoundSplitter::new();
        // Fallback: returns original token for unknown words
        let fallback_parts = splitter.decompose("unbekannteswort");
        assert_eq!(fallback_parts, vec!["unbekannteswort"]);
        // Decomposition: splits known German compound words
        let compound_parts = splitter.decompose("bundesverfassungsgericht");
        assert_eq!(compound_parts, vec!["bundes", "verfassungs", "gericht"]);
        assert_eq!(splitter.language(), "de");
    }

    #[test]
    fn test_kmu_domain_compounds() {
        let splitter = GermanCompoundSplitter::new();
        let result = splitter.decompose("lagerbestandsverwaltung");
        assert!(result.len() > 1);

        let result = splitter.decompose("urlaubsantragsprozess");
        assert!(result.len() > 1);
    }

    #[test]
    fn test_umlaut_normalization_kmu_terms() {
        assert_eq!(normalize_umlauts("Änderungsantrag"), "aenderungsantrag");
        assert_eq!(normalize_umlauts("Qualitätsprüfung"), "qualitaetspruefung");
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
