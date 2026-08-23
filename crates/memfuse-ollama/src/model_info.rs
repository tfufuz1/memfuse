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
