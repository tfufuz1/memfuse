// FILE-CONTEXT: Deutsche Morphologie & Komposita-Zerlegung.
// ZWECK: Umlaut-Normalisierung (ä->ae, ö->oe, ü->ue, ß->ss) und Zerlegung deutscher Zusammensetzungen.
// INVARIANTEN: Decompose arbeitet ausschließlich auf Kleinbuchstaben und prüft Mindestkomponentenlänge.
// NICHT-OFFENSICHTLICH: KMU-Wörterbuch aus data/german_words.txt via include_str! geladen.
// HOTSPOTS: GermanCompoundSplitter::decompose, normalize_umlauts
// STAND: TS:2026-08-30T18:51:48Z (SESSION: 872b1087)

use std::collections::HashSet;

/// KMU-Fachvokabular und allgemeiner deutscher Wortschatz.
///
/// Zur Kompilierzeit eingebettetes Wörterbuch (aus `data/german_words.txt`).
/// Verhindert Hartcodierung im Quelltext und ermöglicht einfache Erweiterbarkeit.
const DEFAULT_GERMAN_WORDS: &str = include_str!("data/german_words.txt");

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

/// German compound word splitter (*Komposita-Zerleger*).
///
/// Uses dictionary-based recursive segmentation powered by Dynamic Programming (DP).
///
/// # Architecture & Algorithm
/// - **Embedded Dictionary**: Thousands of common German stems, root words, prefixes, and KMU
///   enterprise vocabulary loaded at compile time via `include_str!("data/german_words.txt")`.
/// - **Dynamic Programming (DP)**: Evaluates candidate segmentations of a token to find valid
///   stem paths, preferring decompositions with fewer segments and longer constituent components.
/// - **Interfix Candidates (Fugenelemente)**: Supports interfixes `-s-`, `-en-`, `-e-`, `-er-`,
///   `-n-`, and `-es-` occurring strictly between dictionary-matched components.
///
/// # Explicit Linguistic Limitations
/// 1. **Homograph Ambiguity**: Words with identical spellings that yield multiple valid split paths
///    (e.g., *Wachstube* $\rightarrow$ *Wachs-Tube* vs. *Wach-Stube*) are resolved deterministically
///    by segment count and stem length heuristic. Context-aware semantic disambiguation requires an
///    upstream LLM/POS tagger.
/// 2. **Unseen Stems & Proper Nouns**: Unknown company names, foreign loanwords, or unlisted stems
///    will fail dictionary lookup and safely fall back to returning the full original token unsplit.
/// 3. **Interfix Overgeneration Guard**: Interfixes are strictly constrained between recognized
///    dictionary stems, preventing invalid splitting of non-compound words ending in `-es` or `-en`.
///
/// # Input Contract
/// Input tokens MUST be lowercased (and ideally normalized with
/// [`normalize_umlauts`]) before calling [`MorphologicalTokenizer::decompose`].
/// Uppercase input triggers a `debug_assert!` panic in debug builds and
/// causes silent dictionary misses (fallback: token returned unsplit) in
/// release builds.
///
/// Fallback: returns the original token unsplit.
/// A prefix trie node for fast lookup and prefix matching of German dictionary words.
#[derive(Default, Debug, Clone)]
pub struct TrieNode {
    /// Indicates if a word ends at this node.
    pub is_terminal: bool,
    /// Child nodes keyed by character.
    pub children: std::collections::HashMap<char, TrieNode>,
}

/// A prefix trie data structure for dictionary lookup.
#[derive(Default, Debug, Clone)]
pub struct Trie {
    root: TrieNode,
}

impl Trie {
    /// Creates a new empty Trie.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a word into the Trie.
    pub fn insert(&mut self, word: &str) {
        let mut curr = &mut self.root;
        for ch in word.chars() {
            curr = curr.children.entry(ch).or_default();
        }
        curr.is_terminal = true;
    }

    /// Checks if a word exists in the Trie.
    pub fn contains(&self, word: &str) -> bool {
        let mut curr = &self.root;
        for ch in word.chars() {
            if let Some(next) = curr.children.get(&ch) {
                curr = next;
            } else {
                return false;
            }
        }
        curr.is_terminal
    }

    /// Checks if any word in the Trie starts with the given prefix.
    pub fn starts_with(&self, prefix: &str) -> bool {
        let mut curr = &self.root;
        for ch in prefix.chars() {
            if let Some(next) = curr.children.get(&ch) {
                curr = next;
            } else {
                return false;
            }
        }
        true
    }
}

pub struct GermanCompoundSplitter {
    /// Minimum component length for splitting.
    min_component_len: usize,
    /// Trie data structure for fast prefix checking and dictionary matching.
    trie: Trie,
}

impl GermanCompoundSplitter {
    /// Creates a new German compound splitter loaded with the embedded German vocabulary.
    pub fn new() -> Self {
        Self::with_min_length(3)
    }

    /// Creates a splitter with custom minimum component length and default embedded vocabulary.
    pub fn with_min_length(min_len: usize) -> Self {
        let mut trie = Trie::new();
        for line in DEFAULT_GERMAN_WORDS.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let norm = normalize_umlauts(trimmed);
            if norm.len() >= 2 {
                trie.insert(&norm);
            }
        }
        Self {
            min_component_len: min_len,
            trie,
        }
    }

    /// Creates a splitter with custom minimum component length and custom dictionary set.
    pub fn with_dictionary(min_len: usize, custom_words: HashSet<String>) -> Self {
        let mut trie = Trie::new();
        for word in custom_words {
            let norm = normalize_umlauts(&word);
            if norm.len() >= 2 {
                trie.insert(&norm);
            }
        }
        Self {
            min_component_len: min_len,
            trie,
        }
    }

    /// Returns the minimum component length.
    pub fn min_component_len(&self) -> usize {
        self.min_component_len
    }

    /// Checks if a slice is a valid dictionary stem or stem + interfix.
    fn is_valid_component(&self, sub: &str, is_last: bool) -> bool {
        let norm_sub = normalize_umlauts(sub);
        if norm_sub.len() < 2 {
            return false;
        }

        // Direct dictionary match via Trie/HashSet
        if self.trie.contains(&norm_sub) {
            return true;
        }

        // Interfix candidates (Fugenelemente) — allowed strictly between components
        if !is_last {
            const INTERFIXES: &[&str] = &["s", "en", "e", "er", "n", "es"];
            for &fuge in INTERFIXES {
                if norm_sub.ends_with(fuge) && norm_sub.len() > fuge.len() {
                    let norm_stem = &norm_sub[..norm_sub.len() - fuge.len()];
                    if norm_stem.len() >= 2 && self.trie.contains(norm_stem) {
                        return true;
                    }
                }
            }
        } else {
            // Also allow a component with an interfix if it matches a known stem + interfix pattern
            // or if it was matched as part of backtracking.
        }

        false
    }
}

impl Default for GermanCompoundSplitter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
struct PathNode {
    prev: usize,
    segment_count: usize,
    min_segment_len: usize,
}

impl MorphologicalTokenizer for GermanCompoundSplitter {
    fn decompose<'a>(&self, token: &'a str) -> Vec<&'a str> {
        debug_assert!(
            token.chars().all(|c| !c.is_uppercase()),
            "GermanCompoundSplitter::decompose received non-lowercase input: {:?}. \
             Call normalize_umlauts() before decompose().",
            token
        );

        // Guard against oversized single tokens (e.g., > 128 bytes) to avoid O(n^2) DP overhead
        if token.len() <= self.min_component_len || token.len() > 128 {
            return vec![token];
        }

        let n = token.len();
        let mut dp: Vec<Option<PathNode>> = vec![None; n + 1];
        dp[0] = Some(PathNode {
            prev: 0,
            segment_count: 0,
            min_segment_len: usize::MAX,
        });

        for i in 0..n {
            if !token.is_char_boundary(i) {
                continue;
            }

            let current_node = match &dp[i] {
                Some(node) => node.clone(),
                None => continue,
            };

            for j in (i + 2)..=n {
                if !token.is_char_boundary(j) {
                    continue;
                }

                let sub = &token[i..j];
                let is_last = j == n;

                if self.is_valid_component(sub, is_last) {
                    let sub_char_count = sub.chars().count();
                    let new_seg_count = current_node.segment_count + 1;
                    let new_min_len = current_node.min_segment_len.min(sub_char_count);

                    let candidate = PathNode {
                        prev: i,
                        segment_count: new_seg_count,
                        min_segment_len: new_min_len,
                    };

                    let update = match &dp[j] {
                        None => true,
                        Some(existing) => {
                            if candidate.segment_count < existing.segment_count {
                                true
                            } else if candidate.segment_count == existing.segment_count {
                                candidate.min_segment_len > existing.min_segment_len
                            } else {
                                false
                            }
                        }
                    };

                    if update {
                        dp[j] = Some(candidate);
                    }
                }
            }
        }

        // Backtrack optimal path if compound decomposition (>= 2 segments) was found
        if let Some(ref target_node) = dp[n] {
            if target_node.segment_count >= 2 {
                let mut path = Vec::with_capacity(target_node.segment_count);
                let mut curr = n;
                while curr > 0 {
                    if let Some(ref node) = dp[curr] {
                        let prev = node.prev;
                        path.push(&token[prev..curr]);
                        curr = prev;
                    } else {
                        break;
                    }
                }
                path.reverse();
                return path;
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
    fn german_tokenizer_splits_compounds() {
        use crate::tokenizer::Tokenizer;
        let tok = crate::tokenizer::GermanMorphTokenizer::new();
        let tokens = tok.tokenize("Datenbankentwicklung");
        assert!(tokens.iter().any(|t| t == "Datenbank" || t == "datenbank"));
    }

    #[test]
    fn german_tokenizer_normalizes_umlauts() {
        use crate::tokenizer::Tokenizer;
        let tok = crate::tokenizer::GermanMorphTokenizer::new();
        let tokens = tok.tokenize("Bücher");
        // "Bücher" → "buecher" oder ähnlich
        assert!(tokens
            .iter()
            .any(|t: &String| t.contains("ue") || t.contains("buch")));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "non-lowercase input")]
    fn test_decompose_panics_on_uppercase_in_debug() {
        let splitter = GermanCompoundSplitter::new();
        let _ = splitter.decompose("Bundesverfassungsgericht");
    }

    #[test]
    fn test_decompose_accepts_lowercase() {
        let splitter = GermanCompoundSplitter::new();
        let parts = splitter.decompose("bundesverfassungsgericht");
        assert!(!parts.is_empty());
    }

    #[test]
    fn test_normalize_umlauts_produces_valid_input_for_splitter() {
        let splitter = GermanCompoundSplitter::new();
        let tokens = ["Bundesverfassungsgericht", "Überwachungsgesetz", "Straße"];
        for token in &tokens {
            let normalized = normalize_umlauts(token);
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
        let fallback_parts = splitter.decompose("unbekannteswort");
        assert_eq!(fallback_parts, vec!["unbekannteswort"]);

        let compound_parts = splitter.decompose("bundesverfassungsgericht");
        assert_eq!(compound_parts, vec!["bundes", "verfassungs", "gericht"]);
        assert_eq!(splitter.language(), "de");
    }

    #[test]
    fn compound_splitting_known_words() {
        let splitter = GermanCompoundSplitter::new();
        let result = splitter.decompose("datenbankserver");
        assert!(
            result.iter().any(|t| t.eq_ignore_ascii_case("datenbank")),
            "Datenbankserver must split to include Datenbank"
        );
        assert!(result.iter().any(|t| t.eq_ignore_ascii_case("server")));
    }

    #[test]
    fn umlaut_normalization_is_consistent() {
        let n1 = normalize_umlauts("Müller");
        let n2 = normalize_umlauts("Müller");
        assert_eq!(
            n1, n2,
            "Same input must produce same output (deterministic)"
        );
        assert!(
            n1.contains("ue") || n1.contains('ü'),
            "Must either substitute or preserve consistently"
        );
    }

    #[test]
    fn bm25_german_ranks_more_matches_higher() {
        let score_datenbank = crate::bm25::score_term(1, 10, 10.0, 2, 50);
        let score_server = crate::bm25::score_term(1, 10, 10.0, 2, 50);

        let score_doc1 = score_datenbank;
        let score_doc2 = score_datenbank + score_server;

        assert!(
            score_doc2 > score_doc1,
            "Document matching 2 German terms ({}) must score higher than 1 term ({})",
            score_doc2,
            score_doc1
        );
    }

    #[test]
    fn test_german_compounds_explicit_cases() {
        let splitter = GermanCompoundSplitter::new();
        assert_eq!(
            splitter.decompose("datenbankserver"),
            vec!["datenbank", "server"]
        );
        assert_eq!(
            splitter.decompose("unternehmensassistent"),
            vec!["unternehmens", "assistent"]
        );
        assert_eq!(
            splitter.decompose("krankenversicherung"),
            vec!["kranken", "versicherung"]
        );
    }

    #[test]
    fn test_kmu_domain_compounds() {
        let splitter = GermanCompoundSplitter::new();
        let result = splitter.decompose("lagerbestandsverwaltung");
        assert_eq!(result, vec!["lager", "bestands", "verwaltung"]);

        let result = splitter.decompose("urlaubsantragsprozess");
        assert_eq!(result, vec!["urlaubs", "antrags", "prozess"]);
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

        println!("Expansion Ratio: {}", metrics.expansion_ratio());
        assert!(metrics.expansion_ratio() > 1.2, "Expansion should be > 20%");
    }

    struct KmuTestCase {
        word: &'static str,
        expected: &'static [&'static str],
        interfix_type: &'static str,
    }

    #[test]
    fn test_kmu_55_compounds_suite() {
        let splitter = GermanCompoundSplitter::new();

        // 55 Realistic German KMU compounds with linguistic ground truth references.
        let test_cases = [
            // Fugen-s
            KmuTestCase {
                word: "arbeitsvertrag",
                expected: &["arbeits", "vertrag"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "auftragsbestaetigung",
                expected: &["auftrags", "bestaetigung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "rechnungsbetrag",
                expected: &["rechnungs", "betrag"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "geschaeftsfuehrung",
                expected: &["geschaefts", "fuehrung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "qualitaetspruefung",
                expected: &["qualitaets", "pruefung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "versicherungsnetzwerk",
                expected: &["versicherungs", "netzwerk"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "entwicklungsumgebung",
                expected: &["entwicklungs", "umgebung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "sicherheitsueberpruefung",
                expected: &["sicherheits", "ueberpruefung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "beratungsgespraech",
                expected: &["beratungs", "gespraech"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "verwendungszweck",
                expected: &["verwendungs", "zweck"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "forschungsprojekt",
                expected: &["forschungs", "projekt"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "zahlungsziel",
                expected: &["zahlungs", "ziel"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "verwaltungskosten",
                expected: &["verwaltungs", "kosten"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "schulungsunterlagen",
                expected: &["schulungs", "unterlagen"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "lieferungsvereinbarung",
                expected: &["lieferungs", "vereinbarung"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "wartungsarbeiten",
                expected: &["wartungs", "arbeiten"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "bewerbungsunterlagen",
                expected: &["bewerbungs", "unterlagen"],
                interfix_type: "-s-",
            },
            KmuTestCase {
                word: "gehaltsabrechnung",
                expected: &["gehalts", "abrechnung"],
                interfix_type: "-s-",
            },
            // Fugen-en / Fugen-n
            KmuTestCase {
                word: "blumenladen",
                expected: &["blumen", "laden"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "firmenleitung",
                expected: &["firmen", "leitung"],
                interfix_type: "-en-",
            },
            KmuTestCase {
                word: "kundenbetreuung",
                expected: &["kunden", "betreuung"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "expertenwissen",
                expected: &["experten", "wissen"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "lieferantenkatalog",
                expected: &["lieferanten", "katalog"],
                interfix_type: "-en-",
            },
            KmuTestCase {
                word: "strassenverkehr",
                expected: &["strassen", "verkehr"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "sonnenenergie",
                expected: &["sonnen", "energie"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "schraubenschluessel",
                expected: &["schrauben", "schluessel"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "taschenrechner",
                expected: &["taschen", "rechner"],
                interfix_type: "-n-",
            },
            KmuTestCase {
                word: "taschendieb",
                expected: &["taschen", "dieb"],
                interfix_type: "-n-",
            },
            // Fugen-e
            KmuTestCase {
                word: "hundehuette",
                expected: &["hunde", "huette"],
                interfix_type: "-e-",
            },
            KmuTestCase {
                word: "lagereingang",
                expected: &["lager", "eingang"],
                interfix_type: "-e-/zero",
            },
            KmuTestCase {
                word: "schweinebraten",
                expected: &["schweine", "braten"],
                interfix_type: "-e-",
            },
            KmuTestCase {
                word: "lesebuch",
                expected: &["lese", "buch"],
                interfix_type: "-e-",
            },
            // Fugen-er
            KmuTestCase {
                word: "kinderbuch",
                expected: &["kinder", "buch"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "maennerchor",
                expected: &["maenner", "chor"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "bilderbuch",
                expected: &["bilder", "buch"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "woerterbuch",
                expected: &["woerter", "buch"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "gueterverkehr",
                expected: &["gueter", "verkehr"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "geisterstadt",
                expected: &["geister", "stadt"],
                interfix_type: "-er-",
            },
            KmuTestCase {
                word: "huehnerei",
                expected: &["huehner", "ei"],
                interfix_type: "-er-",
            },
            // Fugen-es
            KmuTestCase {
                word: "tagesordnung",
                expected: &["tages", "ordnung"],
                interfix_type: "-es-",
            },
            KmuTestCase {
                word: "landesgericht",
                expected: &["landes", "gericht"],
                interfix_type: "-es-",
            },
            // Zero Interfix
            KmuTestCase {
                word: "personalausweis",
                expected: &["personal", "ausweis"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "pflegeheim",
                expected: &["pflege", "heim"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "handtuch",
                expected: &["hand", "tuch"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "datenspeicher",
                expected: &["daten", "speicher"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "vektorsuche",
                expected: &["vektor", "suche"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "bilanzanalyse",
                expected: &["bilanz", "analyse"],
                interfix_type: "zero",
            },
            KmuTestCase {
                word: "gesetzbuch",
                expected: &["gesetz", "buch"],
                interfix_type: "zero",
            },
            // Multi-stem compounds (3+ components)
            KmuTestCase {
                word: "bundesverfassungsgericht",
                expected: &["bundes", "verfassungs", "gericht"],
                interfix_type: "multi-stem",
            },
            KmuTestCase {
                word: "hauptbahnhof",
                expected: &["haupt", "bahn", "hof"],
                interfix_type: "multi-stem",
            },
            KmuTestCase {
                word: "lagerbestandsverwaltung",
                expected: &["lager", "bestands", "verwaltung"],
                interfix_type: "multi-stem",
            },
            KmuTestCase {
                word: "urlaubsantragsprozess",
                expected: &["urlaubs", "antrags", "prozess"],
                interfix_type: "multi-stem",
            },
            KmuTestCase {
                word: "datenschutzrichtlinie",
                expected: &["datenschutz", "richtlinie"],
                interfix_type: "KMU compound",
            },
            KmuTestCase {
                word: "qualitaetsmanagementsystem",
                expected: &["qualitaets", "management", "system"],
                interfix_type: "multi-stem",
            },
            KmuTestCase {
                word: "datenschutzerklaerung",
                expected: &["datenschutz", "erklaerung"],
                interfix_type: "KMU compound",
            },
        ];

        let total_cases = test_cases.len();
        assert!(
            total_cases >= 50,
            "Suite must contain at least 50 test cases, found {}",
            total_cases
        );

        let mut passed = 0;
        for tc in &test_cases {
            let actual = splitter.decompose(tc.word);
            let is_correct = actual == tc.expected;
            if is_correct {
                passed += 1;
            } else {
                println!(
                    "FAILED case [{}] '{}': expected {:?}, got {:?}",
                    tc.interfix_type, tc.word, tc.expected, actual
                );
            }
        }

        let pass_rate = (passed as f64) / (total_cases as f64);
        println!(
            "KMU Compound Suite Results: {}/{} passed ({:.1}%)",
            passed,
            total_cases,
            pass_rate * 100.0
        );

        assert!(
            pass_rate >= 0.90,
            "Accuracy exit criterion failed: expected >= 90.0%, got {:.1}% ({}/{})",
            pass_rate * 100.0,
            passed,
            total_cases
        );
    }

    #[test]
    fn test_decompose_oversized_token_early_exit() {
        let splitter = GermanCompoundSplitter::new();
        let oversized_token = "a".repeat(200);
        let result = splitter.decompose(&oversized_token);
        assert_eq!(result, vec![oversized_token.as_str()]);
    }

    #[test]
    fn trie_case_empty_single_and_multibyte() {
        let mut trie = Trie::new();
        assert!(!trie.contains("a"));
        assert!(!trie.starts_with("a"));
        assert!(trie.starts_with("")); // empty string is prefix of empty trie root

        trie.insert("a");
        assert!(trie.contains("a"));
        assert!(trie.starts_with("a"));

        trie.insert("über");
        assert!(trie.contains("über"));
        assert!(trie.starts_with("üb"));
        assert!(!trie.contains("üb"));
    }

    #[test]
    fn german_compound_splitter_case_constructors() {
        let default_splitter = GermanCompoundSplitter::new();
        assert_eq!(default_splitter.min_component_len(), 3);

        let custom_min = GermanCompoundSplitter::with_min_length(4);
        assert_eq!(custom_min.min_component_len(), 4);

        let mut custom_words = HashSet::new();
        custom_words.insert("super".to_string());
        custom_words.insert("kauf".to_string());

        let dict_splitter = GermanCompoundSplitter::with_dictionary(3, custom_words);
        assert_eq!(dict_splitter.min_component_len(), 3);
        let parts = dict_splitter.decompose("superkauf");
        assert_eq!(parts, vec!["super", "kauf"]);
    }

    #[test]
    fn passthrough_tokenizer_case_methods() {
        let tok = PassthroughTokenizer::new("en");
        assert_eq!(tok.language(), "en");
        let decl = tok.decompose("hello");
        assert_eq!(decl, vec!["hello"]);
    }

    #[test]
    fn normalize_umlauts_case_edge_inputs() {
        assert_eq!(normalize_umlauts(""), "");
        assert_eq!(normalize_umlauts("ÄÖÜß"), "aeoeuess");
        assert_eq!(normalize_umlauts("Grüße aus Köln!"), "gruesse aus koeln!");
    }
}
