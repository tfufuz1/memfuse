use memfuse_core::Result;
use memfuse_ollama::OllamaClient;

/// Bridge zu einer lokal laufenden Ollama-Instanz (nutzt memfuse-ollama).
pub struct OllamaBridge {
    client: OllamaClient,
    model: String,
}

impl OllamaBridge {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: OllamaClient::new(base_url.into()),
            model: memfuse_ollama::DEFAULT_EMBED_MODEL.to_string(),
        }
    }

    pub fn with_model(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: OllamaClient::new(base_url.into()),
            model: model.into(),
        }
    }

    pub fn localhost() -> Self {
        Self::new(memfuse_ollama::DEFAULT_BASE_URL)
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        self.client.list_models().await
    }

    pub async fn chat_with_rag_streaming(
        &self,
        model: &str,
        user_query: &str,
        context: &str,
        on_token: impl FnMut(String) + Send,
    ) -> Result<String> {
        self.client
            .chat_with_rag_streaming(model, user_query, context, on_token)
            .await
    }
}

#[async_trait::async_trait]
impl memfuse_core::TextEmbeddingEngine for OllamaBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.client.embed(&self.model, text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_bridge_initialization() {
        let bridge1 = OllamaBridge::new("http://127.0.0.1:11434");
        assert_eq!(bridge1.model, memfuse_ollama::DEFAULT_EMBED_MODEL);

        let bridge2 = OllamaBridge::with_model("http://127.0.0.1:11434", "custom-model");
        assert_eq!(bridge2.model, "custom-model");

        let bridge3 = OllamaBridge::localhost();
        assert_eq!(bridge3.model, memfuse_ollama::DEFAULT_EMBED_MODEL);
    }
}
