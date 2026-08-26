use crate::client::OllamaClient;
use memfuse_core::Result;

/// Generiert LLM-Kontextpräfixe für Chunks via Ollama.
pub struct ContextPrefixer {
    client: OllamaClient,
    model: String,
    /// Maximale Länge des gesamten Dokument-Kontexts im Prompt (Tokens-Heuristik: chars/4)
    max_doc_context_chars: usize,
}

impl ContextPrefixer {
    /// Erstellt einen neuen `ContextPrefixer` mit der angegebenen Ollama-Client-Instanz und dem Modell.
    pub fn new(client: OllamaClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
            max_doc_context_chars: 8_000, // ~2000 Tokens – Ollama-kompatibel
        }
    }

    /// Generiert ein Kontext-Präfix für einen einzelnen Chunk.
    ///
    /// `whole_doc` ist der gekürzte Gesamtdokument-Kontext (für Anthropic Prompt Caching Pattern).
    /// `chunk_text` ist der raw Chunk-Inhalt.
    ///
    /// # Sicherheit
    /// Der `whole_doc`-String wird auf `max_doc_context_chars` gekürzt (DENY Prompt Injection).
    /// Newlines in `chunk_text` werden NICHT gefiltert (absichtlich – Inhaltsteil).
    pub async fn generate_prefix(
        &self,
        whole_doc: &str,
        chunk_text: &str,
    ) -> Result<String> {
        let doc_context: String = whole_doc
            .chars()
            .take(self.max_doc_context_chars)
            .collect();

        let prompt = format!(
            "<document>\n{doc_context}\n</document>\n\n\
             <chunk>\n{chunk_text}\n</chunk>\n\n\
             Gib eine kurze Beschreibung (1–2 Sätze) des Chunks im Kontext des Dokuments. \
             Antwort NUR mit der Beschreibung, kein Präambel.",
        );

        self.client.generate(&self.model, &prompt).await
    }

    /// Generiert Präfixe für mehrere Chunks eines Dokuments.
    /// Nutzt das gleiche `whole_doc` für alle Chunks (= Prompt Caching Pattern).
    pub async fn generate_batch(
        &self,
        whole_doc: &str,
        chunks: &[String],
    ) -> Result<Vec<String>> {
        let mut results = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let prefix = self.generate_prefix(whole_doc, chunk).await?;
            results.push(prefix);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_prefix_mock() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
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
                    "response": "Dies ist ein Kontext-Präfix."
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
        let prefixer = ContextPrefixer::new(client, "llama3");
        let prefix = prefixer
            .generate_prefix("Gesamtdokument Inhalt", "Chunk Inhalt")
            .await
            .unwrap();
        assert_eq!(prefix, "Dies ist ein Kontext-Präfix.");
    }

    #[tokio::test]
    async fn test_generate_batch_mock() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "response": "Präfix"
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
        let prefixer = ContextPrefixer::new(client, "llama3");
        let chunks = vec!["Chunk 1".to_string(), "Chunk 2".to_string()];
        let prefixes = prefixer
            .generate_batch("Dokument", &chunks)
            .await
            .unwrap();
        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0], "Präfix");
        assert_eq!(prefixes[1], "Präfix");
    }
}
