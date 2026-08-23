use crate::client::{OllamaClient, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL};
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use memfuse_core::{Result, TextEmbeddingEngine};

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
        Self {
            client: OllamaClient::new(base_url.into()),
            model: model.into(),
            concurrency: 8,
            expected_dimension: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL)
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
        if let Some(dim) = self.expected_dimension {
            if vec.len() != dim {
                return Err(memfuse_core::MemFuseError::invalid_input(format!(
                    "Ollama embedding dimension mismatch: expected {}, got {}",
                    dim,
                    vec.len()
                )));
            }
        }
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let owned_texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let results: Vec<Result<Vec<f32>>> = stream::iter(owned_texts)
            .map(|text| async move { self.client.embed(&self.model, &text).await })
            .buffer_unordered(self.concurrency)
            .collect()
            .await;

        let mut output = Vec::with_capacity(results.len());
        for res in results {
            let vec = res?;
            if let Some(dim) = self.expected_dimension {
                if vec.len() != dim {
                    return Err(memfuse_core::MemFuseError::invalid_input(format!(
                        "Ollama embedding dimension mismatch: expected {}, got {}",
                        dim,
                        vec.len()
                    )));
                }
            }
            output.push(vec);
        }
        Ok(output)
    }
}
