// FILE-CONTEXT
// STAND: 2026-08-30T18:54:39Z (SESSION: ed7b7b38)
// ZWECK: Anthropic Contextual Retrieval — LLM-basierte Präfix-Generierung für Chunks
// INVARIANTEN: XML-Escaping vor Prompt-Bau; Truncation wahrt Unicode-Codepoint- & Wortgrenzen
// NICHT-OFFENSICHTLICH: Document-Exzerpt wird vor Prefix-Erzeugung hard auf max_document_chars gekürzt
// HOTSPOTS: generate_prefix, truncate_prefix, truncate_chars

//! Anthropic Contextual Retrieval: LLM-generierte Kontext-Präfixe für Chunks.
//!
//! Implementiert das Contextual Retrieval Pattern von Anthropic (2024):
//! Jeder Chunk erhält vor BM25/Embedding ein dokumentenbezogenes Kurzpräfix.
//!
//! Empirisch: 49% weniger Retrieval-Fehler vs. naïves Chunking.
//! Mit Cross-Encoder Reranking: 67% Reduktion.

use crate::api::OllamaApi;
use crate::prompt::xml_escape;
use memfuse_core::MemFuseError;
use std::sync::Arc;

/// Konfiguration für Kontext-Präfix-Generierung.
#[derive(Debug, Clone)]
pub struct ContextPrefixConfig {
    /// Ollama-Modell für Prefix-Generierung (sollte schnell & klein sein).
    /// Empfehlung: "llama3.2:3b" oder "gemma2:2b" — nicht das Haupt-Chat-Modell.
    pub model: String,
    /// Maximale Länge des Gesamtdokuments für Kontext (in Zeichen).
    /// Verhindert Überschreitung des LLM-Kontextfensters.
    /// Standard: 8000 Zeichen ≈ 2000 Tokens.
    pub max_document_chars: usize,
    /// Maximale Länge des generierten Präfixes in Tokens (Schätzwert).
    /// Anthropic empfiehlt 50-100 Tokens. Standard: 80.
    pub max_prefix_tokens: usize,
}

impl Default for ContextPrefixConfig {
    fn default() -> Self {
        Self {
            model: "llama3.2".into(),
            max_document_chars: 8000,
            max_prefix_tokens: 80,
        }
    }
}

/// Generiert LLM-basierte Kontext-Präfixe für Dokument-Chunks.
///
/// # Verwendung
/// ```no_run
/// # async fn example() -> Result<(), memfuse_core::MemFuseError> {
/// use memfuse_ollama::{OllamaClient, ContextPrefixEngine, ContextPrefixConfig};
/// use std::sync::Arc;
/// let client = Arc::new(OllamaClient::new("http://localhost:11434"));
/// let engine = ContextPrefixEngine::new(client, ContextPrefixConfig::default());
/// let prefix = engine.generate_prefix("Volltext des Dokuments...", "Chunk-Inhalt...").await?;
/// # Ok(()) }
/// ```
pub struct ContextPrefixEngine {
    client: Arc<dyn OllamaApi>,
    config: ContextPrefixConfig,
}

/// Compatibility alias for ContextPrefixEngine.
pub type ContextPrefixer = ContextPrefixEngine;

impl ContextPrefixEngine {
    pub fn new(client: Arc<dyn OllamaApi>, config: ContextPrefixConfig) -> Self {
        Self { client, config }
    }

    /// Generiert ein Kontext-Präfix für einen einzelnen Chunk.
    ///
    /// Der Prompt folgt dem Anthropic-Pattern:
    /// "Describe this chunk in context of the full document (1-2 sentences)."
    ///
    /// # Fehler
    /// - `MemFuseError::Storage` / `MemFuseError::Io` wenn Ollama nicht erreichbar
    /// - `MemFuseError::InvalidInput` wenn document oder chunk leer
    pub async fn generate_prefix(
        &self,
        full_document: &str,
        chunk_content: &str,
    ) -> Result<String, MemFuseError> {
        if full_document.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "full_document must not be empty".into(),
            ));
        }
        if chunk_content.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "chunk_content must not be empty".into(),
            ));
        }

        // Escaping gegen Tag-Injection in XML-Struktur
        let escaped_doc = xml_escape(full_document);
        let escaped_chunk = xml_escape(chunk_content);

        // AI-TAG[CORRECTNESS][MINOR] XML entity truncation order risk (ID: AGT-OLLAMA-7eed57ec) (TS: 2026-09-04T12:59:47Z) (SESSION: d97322aa)
        // BEFUND: truncate_chars() wird nach xml_escape() aufgerufen. Wird der String an einer XML-Entity wie '&amp;' abgeschnitten, kann ein unvollständiges Ampersand entstehen.
        // RISIKO: Unvollständige XML-Entities (z.B. '&am') in doc_excerpt können XML-Parser bei Downstream-Verarbeitung verwirren.
        // EMPFEHLUNG: Zuerst truncate_chars() auf den unescapten Text anwenden und erst danach xml_escape() aufrufen.
        // Dokument kürzen um LLM-Kontextfenster nicht zu sprengen
        let doc_excerpt = truncate_chars(&escaped_doc, self.config.max_document_chars);
        let max_p = self.config.max_prefix_tokens * 4; // Chars-Approximation

        let prompt = format!(
            "Hier ist ein Dokument:\n<document>\n{doc_excerpt}\n</document>\n\n\
             Hier ist ein spezifischer Abschnitt aus diesem Dokument:\n\
             <chunk>\n{escaped_chunk}\n</chunk>\n\n\
             Schreibe 1-2 Sätze, die diesen Abschnitt im Kontext des \
             Gesamtdokuments beschreiben. Maximal {max_p} Zeichen. \
             Nur der beschreibende Text, keine Einleitung."
        );

        let raw = self
            .client
            .chat(&self.config.model, &prompt)
            .await?;

        // Prefix auf konfigurierte Token/Wort- und Zeichen-Grenze kürzen
        Ok(truncate_prefix(&raw, self.config.max_prefix_tokens, max_p))
    }

    /// Generiert Präfixe für einen Batch von Chunks desselben Dokuments.
    ///
    /// Nutzt sequentielle Verarbeitung (Ollama ist single-threaded per default).
    /// Für Parallel-Verarbeitung: Mehrere ContextPrefixEngine-Instanzen nutzen.
    pub async fn generate_prefix_batch(
        &self,
        full_document: &str,
        chunks: &[&str],
    ) -> Vec<Result<String, MemFuseError>> {
        if let Err(e) = crate::client::validate_batch_size(chunks.len()) {
            return vec![Err(e)];
        }
        let mut results = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            results.push(self.generate_prefix(full_document, chunk).await);
        }
        results
    }
}

pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    // Count Unicode scalar values (chars), not bytes.
    // For ASCII: identical. For UTF-8 multibyte (Ü, ä, ß, 中): chars < bytes.
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

/// Truncates context prefix text while preserving word boundaries up to `max_tokens` (words)
/// and hard character limit `max_chars`.
pub fn truncate_prefix(s: &str, max_tokens: usize, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Step 1: Truncate by estimated token/word boundary
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let word_bounded = if words.len() > max_tokens {
        words[..max_tokens].join(" ")
    } else {
        trimmed.to_string()
    };

    // Step 2: Enforce hard character boundary at word/space boundary if possible
    if word_bounded.chars().count() <= max_chars {
        word_bounded
    } else {
        let char_truncated = truncate_chars(&word_bounded, max_chars);
        if let Some(last_space) = char_truncated.rfind(' ') {
            char_truncated[..last_space].trim_end().to_string()
        } else {
            char_truncated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_prefix_word_boundary() {
        let text = "Dies ist ein sehr langes Dokument mit vielen Worten und Informationen";
        let res = truncate_prefix(text, 5, 200);
        assert_eq!(res, "Dies ist ein sehr langes");

        let res2 = truncate_prefix(text, 10, 20);
        assert_eq!(res2, "Dies ist ein sehr");
        assert!(res2.chars().count() <= 20);
    }

    #[test]
    fn test_truncate_chars_short_string() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_chars_exact_limit() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_chars_over_limit() {
        let result = truncate_chars("hello world", 5);
        assert_eq!(result.len(), 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_chars_unicode_boundary() {
        // "Ü" = 2 bytes — must not truncate mid-codepoint
        let s = "Über die Welt";
        let truncated = truncate_chars(s, 3);
        assert!(s.is_char_boundary(truncated.len()));
    }

    #[test]
    fn test_truncate_chars_unicode_no_premature_truncation() {
        // "Über" = 4 chars (Ü=2bytes, b=1, e=1, r=1) = 5 bytes
        // With max_chars=4: should return "Über" (fits exactly)
        let s = "Über";
        assert_eq!(s.chars().count(), 4);
        assert_eq!(s.len(), 5); // bytes
        assert_eq!(
            truncate_chars(s, 4),
            "Über",
            "4-char string must fit in max_chars=4"
        );
        assert_eq!(
            truncate_chars(s, 3),
            "Übe",
            "truncation at char=3 must work"
        );
    }

    #[test]
    fn test_truncate_chars_german_umlauts() {
        let s = "Größe und Stärke"; // 16 chars, >16 bytes
        let truncated = truncate_chars(s, 6);
        assert_eq!(truncated.chars().count(), 6);
        assert!(
            s.is_char_boundary(truncated.len()),
            "truncation must be at char boundary"
        );
    }

    use crate::mock::MockOllamaClient;

    #[tokio::test]
    async fn test_generate_prefix_mock() {
        let mock = Arc::new(MockOllamaClient::new(vec![], "Dies ist ein Kontext-Präfix."));
        let engine = ContextPrefixEngine::new(mock, ContextPrefixConfig::default());
        let prefix = engine
            .generate_prefix("Gesamtdokument Inhalt", "Chunk Inhalt")
            .await
            .unwrap(); // unwrap
        assert_eq!(prefix, "Dies ist ein Kontext-Präfix.");
    }

    #[tokio::test]
    async fn test_generate_prefix_batch_mock() {
        let mock = Arc::new(MockOllamaClient::new(vec![], "Präfix"));
        let engine = ContextPrefixEngine::new(mock, ContextPrefixConfig::default());
        let chunks = vec!["Chunk 1", "Chunk 2"];
        let prefixes = engine.generate_prefix_batch("Dokument", &chunks).await;
        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0].as_ref().unwrap(), "Präfix"); // unwrap
        assert_eq!(prefixes[1].as_ref().unwrap(), "Präfix"); // unwrap
    }
}
