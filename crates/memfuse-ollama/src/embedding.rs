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
}

impl OllamaEmbedder {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: OllamaClient::new(base_url.into()),
            model: model.into(),
            concurrency: 8,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL)
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl TextEmbeddingEngine for OllamaEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.client.embed(&self.model, text).await
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let owned_texts: Vec<String> = texts.iter().map(|&s| s.to_string()).collect();
        let client = self.client.clone();
        let model = self.model.clone();

        let results: Vec<Result<Vec<f32>>> = stream::iter(owned_texts)
            .map(move |text| {
                let client = client.clone();
                let model = model.clone();
                async move { client.embed(&model, &text).await }
            })
            .buffer_unordered(self.concurrency)
            .collect()
            .await;

        let mut output = Vec::with_capacity(results.len());
        for res in results {
            output.push(res?);
        }
        Ok(output)
    }
}
