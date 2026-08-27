//! Bekannte Ollama-Modell-Dimensionen für Embedding-Modelle.

use crate::client::OllamaClient;
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub modelfile: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

impl OllamaClient {
    /// Validates that the model exists in Ollama via GET /api/tags before invoking embeddings/chat.
    pub async fn validate_model_available(&self, model: &str) -> Result<()> {
        crate::client::validate_model_name(model)?;
        if !self.is_model_available(model).await {
            return Err(MemFuseError::InvalidInput(format!(
                "Ollama model '{}' not found. Run: ollama pull {}",
                model, model
            )));
        }
        Ok(())
    }

    /// Retrieves model info via POST /api/show
    pub async fn show_model(&self, model: &str) -> Result<ModelInfo> {
        crate::client::validate_model_name(model)?;

        #[derive(Serialize)]
        struct ShowRequest<'a> {
            name: &'a str,
        }
        #[derive(Deserialize)]
        struct ShowResponse {
            modelfile: Option<String>,
            details: Option<ModelDetails>,
        }
        #[derive(Deserialize)]
        struct ModelDetails {
            parameter_size: Option<String>,
            quantization_level: Option<String>,
        }

        let url = format!("{}/api/show", self.base_url());
        let req = ShowRequest { name: model };

        let response = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama /api/show request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unreadable>".into());
            let lower = body.to_lowercase();
            if lower.contains("model") && lower.contains("not found")
                || status == reqwest::StatusCode::NOT_FOUND
            {
                return Err(MemFuseError::NotFound(format!(
                    "Ollama model '{model}' not found. Run: ollama pull {model}"
                )));
            }
            return Err(MemFuseError::Storage(format!(
                "Ollama show_model HTTP {status}: {body}"
            )));
        }

        let parsed: ShowResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Invalid Ollama show response: {e}")))?;

        let (param_size, quant_level) = match parsed.details {
            Some(d) => (d.parameter_size, d.quantization_level),
            None => (None, None),
        };

        Ok(ModelInfo {
            modelfile: parsed.modelfile,
            parameter_size: param_size,
            quantization_level: quant_level,
        })
    }
}

/// Gibt die Embedding-Dimension für bekannte Modelle zurück.
/// Gibt `None` zurück wenn das Modell unbekannt ist.
pub fn known_dimension(model: &str) -> Option<usize> {
    // Normalisiere: Kleinbuchstaben, Strip Tag (z.B. ":latest")
    let base = model.split(':').next().unwrap_or(model).to_lowercase();
    match base.as_str() {
        "nomic-embed-text" => Some(768),
        "mxbai-embed-large" => Some(1024),
        "all-minilm" => Some(384),
        "snowflake-arctic-embed" => Some(1024),
        "text-embedding-ada-002" => Some(1536), // OpenAI-kompatibel
        "text-embedding-3-small" => Some(1536),
        "text-embedding-3-large" => Some(3072),
        "bge-large-en-v1.5" => Some(1024),
        "bge-m3" => Some(1024),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_model_available() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "models": [
                        { "name": "nomic-embed-text:latest" }
                    ]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        assert!(client
            .validate_model_available("nomic-embed-text")
            .await
            .is_ok());

        let err = client
            .validate_model_available("missing-model")
            .await
            .unwrap_err();
        match err {
            MemFuseError::InvalidInput(msg) => {
                assert!(msg.contains("Ollama model 'missing-model' not found"));
                assert!(msg.contains("Run: ollama pull missing-model"));
            }
            _ => panic!("Expected MemFuseError::InvalidInput, got {:?}", err),
        }
    }

    #[test]
    fn test_known_dimension_nomic() {
        assert_eq!(known_dimension("nomic-embed-text"), Some(768));
        assert_eq!(known_dimension("nomic-embed-text:latest"), Some(768));
        assert_eq!(known_dimension("unknown-model-xyz"), None);
    }

    #[test]
    fn model_info_deserializes_correctly() {
        let json = r#"{"modelfile":"FROM nomic-embed-text","parameter_size":"137M","quantization_level":"Q4_0"}"#;
        let info: ModelInfo = serde_json::from_str(json).unwrap(); // unwrap
        assert_eq!(info.modelfile.as_deref(), Some("FROM nomic-embed-text"));
        assert_eq!(info.parameter_size.as_deref(), Some("137M"));
        assert_eq!(info.quantization_level.as_deref(), Some("Q4_0"));
    }
}
