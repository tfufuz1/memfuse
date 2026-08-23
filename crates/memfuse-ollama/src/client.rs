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
    client: reqwest::Client,
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

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
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

    /// Generates vector embedding for a single text prompt via POST /api/embeddings
    pub async fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>> {
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

        while let Some(chunk_result) = stream.next().await {
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
                        break;
                    }
                }
            }
        }

        Ok(full_response)
    }
}
