use crate::client::{OllamaClient, OllamaConfig, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL};
use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, TextEmbeddingEngine};

/// Implementation of `TextEmbeddingEngine` using Ollama's HTTP API.
#[derive(Clone, Debug)]
pub struct OllamaEmbedder {
    client: OllamaClient,
    model: String,
    concurrency: usize,
    expected_dimension: Option<usize>,
}

impl OllamaEmbedder {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let base_url_str = base_url.into();
        let model_str = model.into();
        let config = OllamaConfig {
            base_url: base_url_str,
            model: model_str.clone(),
            ..Default::default()
        };
        Self {
            client: OllamaClient::with_config(config),
            model: model_str,
            concurrency: 8,
            expected_dimension: None,
        }
    }

    pub fn with_config(config: OllamaConfig) -> Self {
        let model = config.model.clone();
        Self {
            client: OllamaClient::with_config(config),
            model,
            concurrency: 8,
            expected_dimension: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL)
    }

    pub fn config(&self) -> &OllamaConfig {
        self.client.config()
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    pub fn with_expected_dimension(mut self, dim: usize) -> Self {
        self.expected_dimension = Some(dim);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl TextEmbeddingEngine for OllamaEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let vec = self.client.embed(&self.model, text).await?;
        if let Some(expected_dim) = self.expected_dimension {
            if vec.len() != expected_dim {
                return Err(MemFuseError::Index(format!(
                    "Ollama returned embedding of dimension {} but expected {}. Model '{}' may have changed. Rebuild the HNSW index.",
                    vec.len(),
                    expected_dim,
                    self.model
                )));
            }
        }
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Nutze nativen Batch-Endpunkt (mit Fallback im Client)
        let output = self.client.embed_batch(&self.model, texts).await?;

        if let Some(expected_dim) = self.expected_dimension {
            for vec in &output {
                if vec.len() != expected_dim {
                    return Err(MemFuseError::Index(format!(
                        "Ollama returned embedding of dimension {} but expected {}. Model '{}' may have changed. Rebuild the HNSW index.",
                        vec.len(),
                        expected_dim,
                        self.model
                    )));
                }
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embed_batch_empty() {
        // OllamaClient mit nicht-erreichbarer URL
        let client = OllamaClient::new("http://127.0.0.1:1"); // Closed port
                                                              // embed_batch([]) soll sofort Ok(vec![]) zurückgeben ohne Netzwerk-Call
        let embedder = OllamaEmbedder {
            client,
            model: "test".into(),
            concurrency: 4,
            expected_dimension: None,
        };
        let result = embedder.embed_batch(&[]).await.unwrap(); // unwrap allowed
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_dimension_validation_mismatch_returns_index_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "embedding": [0.1, 0.2, 0.3] // 3 dimensions
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

        let embedder =
            OllamaEmbedder::new(server_url, "nomic-embed-text").with_expected_dimension(768);

        let result = embedder.embed("test text").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            MemFuseError::Index(msg) => {
                assert!(msg.contains("Ollama returned embedding of dimension 3 but expected 768"));
                assert!(msg.contains("Model 'nomic-embed-text' may have changed"));
            }
            _ => panic!("Expected MemFuseError::Index, got {:?}", err),
        }
    }
}
