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
    /// Retrieves model info via POST /api/show
    pub async fn show_model(&self, model: &str) -> Result<ModelInfo> {
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

    #[test]
    fn test_known_dimension_nomic() {
        assert_eq!(known_dimension("nomic-embed-text"), Some(768));
        assert_eq!(known_dimension("nomic-embed-text:latest"), Some(768));
        assert_eq!(known_dimension("unknown-model-xyz"), None);
    }
}
