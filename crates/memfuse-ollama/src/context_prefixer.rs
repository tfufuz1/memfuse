//! Anthropic Contextual Retrieval: LLM-generierte Kontext-Präfixe für Chunks.
//!
//! Implementiert das Contextual Retrieval Pattern von Anthropic (2024):
//! Jeder Chunk erhält vor BM25/Embedding ein dokumentenbezogenes Kurzpräfix.
//!
//! Empirisch: 49% weniger Retrieval-Fehler vs. naïves Chunking.
//! Mit Cross-Encoder Reranking: 67% Reduktion.

use crate::client::sanitize_prompt_input;
use crate::OllamaClient;
use memfuse_core::MemFuseError;

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
/// let client = OllamaClient::new("http://localhost:11434");
/// let engine = ContextPrefixEngine::new(client, ContextPrefixConfig::default());
/// let prefix = engine.generate_prefix("Volltext des Dokuments...", "Chunk-Inhalt...").await?;
/// # Ok(()) }
/// ```
pub struct ContextPrefixEngine {
    client: OllamaClient,
    config: ContextPrefixConfig,
}

/// Compatibility alias for ContextPrefixEngine.
pub type ContextPrefixer = ContextPrefixEngine;

impl ContextPrefixEngine {
    pub fn new(client: OllamaClient, config: ContextPrefixConfig) -> Self {
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

        // Sanitization gegen Prompt Injection
        let sanitized_doc = sanitize_prompt_input(full_document);
        let sanitized_chunk = sanitize_prompt_input(chunk_content);

        // Dokument kürzen um LLM-Kontextfenster nicht zu sprengen
        let doc_excerpt = truncate_chars(&sanitized_doc, self.config.max_document_chars);
        let max_p = self.config.max_prefix_tokens * 4; // Chars-Approximation

        let prompt = format!(
            "Hier ist ein Dokument:\n<document>\n{doc_excerpt}\n</document>\n\n\
             Hier ist ein spezifischer Abschnitt aus diesem Dokument:\n\
             <chunk>\n{sanitized_chunk}\n</chunk>\n\n\
             Schreibe 1-2 Sätze, die diesen Abschnitt im Kontext des \
             Gesamtdokuments beschreiben. Maximal {max_p} Zeichen. \
             Nur der beschreibende Text, keine Einleitung."
        );

        let raw = self
            .client
            .generate_text(&self.config.model, &prompt)
            .await?;

        // Prefix auf konfigurierte Länge kürzen
        Ok(truncate_chars(&raw, max_p))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(truncate_chars(s, 4), "Über", "4-char string must fit in max_chars=4");
        assert_eq!(truncate_chars(s, 3), "Übe", "truncation at char=3 must work");
    }

    #[test]
    fn test_truncate_chars_german_umlauts() {
        let s = "Größe und Stärke"; // 16 chars, >16 bytes
        let truncated = truncate_chars(s, 6);
        assert_eq!(truncated.chars().count(), 6);
        assert!(s.is_char_boundary(truncated.len()), "truncation must be at char boundary");
    }

    #[tokio::test]
    async fn test_generate_prefix_mock() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]);
                assert!(req_str.contains("<document>"));
                assert!(req_str.contains("<chunk>"));

                let body = serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": "Dies ist ein Kontext-Präfix."
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        let engine = ContextPrefixEngine::new(client, ContextPrefixConfig::default());
        let prefix = engine
            .generate_prefix("Gesamtdokument Inhalt", "Chunk Inhalt")
            .await
            .unwrap(); // unwrap
        assert_eq!(prefix, "Dies ist ein Kontext-Präfix.");
    }

    #[tokio::test]
    async fn test_generate_prefix_batch_mock() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": "Präfix"
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        let engine = ContextPrefixEngine::new(client, ContextPrefixConfig::default());
        let chunks = vec!["Chunk 1", "Chunk 2"];
        let prefixes = engine.generate_prefix_batch("Dokument", &chunks).await;
        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0].as_ref().unwrap(), "Präfix"); // unwrap
        assert_eq!(prefixes[1].as_ref().unwrap(), "Präfix"); // unwrap
    }
}
