use futures_util::StreamExt;
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Standard Ollama base URL
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Standard embedding model for SME usage (multilingual support for German)
pub const DEFAULT_EMBED_MODEL: &str = "nomic-embed-text";

/// HTTP client for interacting with a local Ollama instance.
#[derive(Clone, Debug)]
pub struct OllamaClient {
    base_url: String,
    pub(crate) client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Clone, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatStreamChunk {
    message: Option<ChatMessageResponse>,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

/// Batch-Embedding Request für `/api/embed` (Ollama ≥ 0.3.9).
#[derive(Serialize)]
struct BatchEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

/// Batch-Embedding Response.
#[derive(Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        // AI-NOTE: reqwest::Client::builder().build() only fails on invalid TLS config,
        // which would be a compile-time/system-level issue. The unwrap_or_else fallback
        // ensures we never panic, and timeout-less operation is still better than no client.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to build HTTP client with timeouts: {e}, falling back to default"
                );
                reqwest::Client::new()
            });
        Self {
            base_url: base_url.into(),
            client,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// List available models in Ollama via GET /api/tags
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            MemFuseError::Internal(format!(
                "Ollama not reachable at {}: {e}. Is Ollama running?",
                self.base_url
            ))
        })?;

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelTagInfo>,
        }
        #[derive(Deserialize)]
        struct ModelTagInfo {
            name: String,
        }

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Invalid Ollama tags response: {e}")))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Generates vector embedding with retry logic for transient failures.
    ///
    /// Retries up to 3 times with exponential backoff (500ms, 1s, 2s).
    /// This handles the common case where Ollama blocks briefly on first call
    /// while loading the model into RAM.
    pub async fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>> {
        const MAX_RETRIES: u32 = 3;
        let mut last_err = None;

        for attempt in 0..MAX_RETRIES {
            match self.try_embed(model, text).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt < MAX_RETRIES - 1 {
                        tracing::warn!(
                            attempt = attempt + 1,
                            max = MAX_RETRIES,
                            "Ollama embed attempt failed, retrying: {e}"
                        );
                        let backoff_ms = 500u64 * (1u64 << attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            MemFuseError::Internal("Embed retries exhausted with no error captured".into())
        }))
    }

    /// Single embed attempt via POST /api/embeddings (no retry).
    async fn try_embed(&self, model: &str, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.base_url);
        let request = EmbedRequest {
            model,
            prompt: text,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama embedding request failed: {e}")))?;

        let parsed: EmbedResponse = response.json().await.map_err(|e| {
            MemFuseError::Internal(format!("Invalid Ollama embedding response: {e}"))
        })?;

        Ok(parsed.embedding)
    }

    /// Generiert Embeddings für mehrere Texte in einem einzelnen HTTP-Request.
    ///
    /// Nutzt den `/api/embed`-Endpunkt (Ollama ≥ 0.3.9).
    /// Fällt automatisch auf sequentielle Einzelrequests zurück, wenn der
    /// Batch-Endpunkt nicht verfügbar ist (404 oder Connection Error).
    pub async fn embed_batch(&self, model: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Versuche Batch-Endpunkt zuerst
        match self.try_embed_batch(model, texts).await {
            Ok(embeddings) => {
                if embeddings.len() == texts.len() {
                    return Ok(embeddings);
                }
                // Längen-Mismatch — Fallback
                tracing::warn!(
                    expected = texts.len(),
                    got = embeddings.len(),
                    "Ollama batch embed returned wrong count, falling back to sequential"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Ollama /api/embed not available, falling back to sequential"
                );
            }
        }

        // Fallback: sequentiell mit bestehender retry-fähiger embed()
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(model, text).await?);
        }
        Ok(results)
    }

    async fn try_embed_batch(&self, model: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embed", self.base_url);
        let request = BatchEmbedRequest {
            model,
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Batch embed request: {e}")))?;

        if !response.status().is_success() {
            return Err(MemFuseError::Internal(format!(
                "Batch embed HTTP {}",
                response.status()
            )));
        }

        let parsed: BatchEmbedResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Batch embed response parse: {e}")))?;

        Ok(parsed.embeddings)
    }

    /// Streams RAG chat response token by token via POST /api/chat
    pub async fn chat_with_rag_streaming(
        &self,
        model: &str,
        user_query: &str,
        context: &str,
        mut on_token: impl FnMut(String) + Send,
    ) -> Result<String> {
        let system_prompt = format!(
            "Du bist ein hilfreicher Unternehmensassistent. Beantworte Fragen \
             ausschließlich auf Basis des folgenden Kontexts aus internen \
             Firmendokumenten. Antworte auf Deutsch. Wenn die Antwort im \
             Kontext nicht zu finden ist, sage ehrlich: \
             'Diese Information liegt mir nicht vor.'\n\nKontext:\n{context}"
        );

        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_query.to_string(),
                },
            ],
            stream: true,
        };

        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama chat request failed: {e}")))?;

        let mut stream = response.bytes_stream();
        let mut full_response = String::new();

        'outer: while let Some(chunk_result) = stream.next().await {
            let bytes =
                chunk_result.map_err(|e| MemFuseError::Internal(format!("Stream error: {e}")))?;
            for line in bytes.split(|&b| b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(chunk) = serde_json::from_slice::<ChatStreamChunk>(line) {
                    if let Some(msg) = chunk.message {
                        on_token(msg.content.clone());
                        full_response.push_str(&msg.content);
                    }
                    if chunk.done {
                        break 'outer;
                    }
                }
            }
        }

        Ok(full_response)
    }
}

#[cfg(test)]
mod tests {
    use crate::embedding::OllamaEmbedder;
    use memfuse_core::TextEmbeddingEngine;

    #[tokio::test]
    async fn test_embed_batch_empty() {
        // OllamaClient mit nicht-erreichbarer URL
        // embed_batch([]) soll sofort Ok(vec![]) zurückgeben ohne Netzwerk-Call
        let embedder = OllamaEmbedder::new("http://127.0.0.1:1", "test");
        let result = embedder.embed_batch(&[]).await.unwrap();
        assert!(result.is_empty());
    }
}
