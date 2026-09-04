// FILE-CONTEXT Header (Format v3)
// ZWECK: Trait-Abstraktion für Ollama LLM / Embedding Operationen.
// INVARIANTEN: OllamaApi ist Send + Sync + 'static; embed_batch delegiert standardmäßig sequentiell auf embed.
// STAND: TS:2026-09-04T13:30:00Z

use async_trait::async_trait;
use memfuse_core::Result;

/// Abstract interface for Ollama LLM operations.
/// Implement `MockOllamaClient` for tests.
#[async_trait]
pub trait OllamaApi: Send + Sync + 'static {
    /// Generates an embedding vector for the given text.
    async fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>>;

    /// Generates a batch of embedding vectors.
    async fn embed_batch(&self, model: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(model, text).await?);
        }
        Ok(results)
    }

    /// Sends a chat prompt and returns the response text.
    async fn chat(&self, model: &str, prompt: &str) -> Result<String>;
}
