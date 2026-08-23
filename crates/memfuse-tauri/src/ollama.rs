use futures_util::StreamExt;
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Bridge zu einer lokal laufenden Ollama-Instanz.
pub struct OllamaBridge {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Clone)]
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

impl OllamaBridge {
    /// Erstellt eine neue Bridge. Standard-Port von Ollama: 11434.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn localhost() -> Self {
        Self::new("http://localhost:11434")
    }

    /// Prüft, ob Ollama erreichbar ist und listet verfügbare Modelle.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            MemFuseError::Internal(format!(
                "Ollama nicht erreichbar unter {}: {e}. Ist Ollama gestartet?",
                self.base_url
            ))
        })?;

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Antwort ungültig: {e}")))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Führt einen RAG-Chat aus: Systemkontext (aus MemFuse-Suchergebnissen)
    /// wird vor die Nutzerfrage gesetzt, Antwort wird gestreamt.
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
            .map_err(|e| {
                MemFuseError::Internal(format!("Ollama-Chat-Anfrage fehlgeschlagen: {e}"))
            })?;

        let mut stream = response.bytes_stream();
        let mut full_response = String::new();

        while let Some(chunk_result) = stream.next().await {
            let bytes =
                chunk_result.map_err(|e| MemFuseError::Internal(format!("Stream-Fehler: {e}")))?;
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
                        break;
                    }
                }
            }
        }

        Ok(full_response)
    }
}

#[async_trait::async_trait]
impl crate::ingestion::pipeline::EmbeddingProvider for OllamaBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            prompt: &'a str,
        }
        #[derive(Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        let url = format!("{}/api/embeddings", self.base_url);
        let request = EmbedRequest {
            model: "nomic-embed-text",
            prompt: text,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Embedding fehlgeschlagen: {e}")))?;

        let parsed: EmbedResponse = response.json().await.map_err(|e| {
            MemFuseError::Internal(format!("Ollama-Embedding-Antwort ungültig: {e}"))
        })?;

        Ok(parsed.embedding)
    }
}
