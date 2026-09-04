// FILE-CONTEXT Header (Format v3)
// ZWECK: Mock-Client für OllamaApi für deterministische Tests ohne echten Ollama-Server.
// INVARIANTEN: Inkludiert unter #[cfg(any(test, feature = "test-utils"))].
// STAND: TS:2026-09-04T13:30:00Z

#[cfg(any(test, feature = "test-utils"))]
use async_trait::async_trait;
#[cfg(any(test, feature = "test-utils"))]
use crate::api::OllamaApi;
#[cfg(any(test, feature = "test-utils"))]
use memfuse_core::Result;

/// Mock implementation of `OllamaApi` for unit and integration testing.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone, Default)]
pub struct MockOllamaClient {
    pub embed_response: Vec<f32>,
    pub chat_response: String,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockOllamaClient {
    /// Creates a new `MockOllamaClient` with preconfigured responses.
    pub fn new(embed_response: Vec<f32>, chat_response: impl Into<String>) -> Self {
        Self {
            embed_response,
            chat_response: chat_response.into(),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait]
impl OllamaApi for MockOllamaClient {
    async fn embed(&self, _model: &str, _text: &str) -> Result<Vec<f32>> {
        Ok(self.embed_response.clone())
    }

    async fn chat(&self, _model: &str, _prompt: &str) -> Result<String> {
        Ok(self.chat_response.clone())
    }
}
