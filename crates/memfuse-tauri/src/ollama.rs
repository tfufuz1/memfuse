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

#[async_trait::async_trait]
impl memfuse_db::QueryRewriter for OllamaBridge {
    async fn rewrite(
        &self,
        original_query: &str,
        current_results: &[memfuse_db::SearchResult],
    ) -> Result<Vec<String>> {
        let prompt = format!(
            "Die ursprüngliche Suchanfrage war: \"{}\".\n\
             Bisher wurden {} Ergebnisse gefunden, die möglicherweise unvollständig sind.\n\
             Generiere bis zu 2 alternative, präzisere Suchbegriffe oder Teilfragen.\n\
             Gib NUR die Suchanfragen zurück, jeweils eine pro Zeile, ohne Aufzählungszeichen.",
            original_query,
            current_results.len()
        );

        match self.client.generate_text(&self.model, &prompt).await {
            Ok(response) => {
                let sub_queries: Vec<String> = response
                    .lines()
                    .map(|l| {
                        l.trim()
                            .trim_start_matches(|c: char| {
                                c.is_ascii_digit() || c == '.' || c == '-' || c == '*'
                            })
                            .trim()
                            .to_string()
                    })
                    .filter(|l| !l.is_empty() && l != original_query)
                    .take(2)
                    .collect();
                Ok(sub_queries)
            }
            Err(e) => Err(e),
        }
    }
}
