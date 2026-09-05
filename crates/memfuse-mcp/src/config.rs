use memfuse_core::{EmbeddingProvider, MemFuseError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Embedding provider configuration settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingConfig {
    /// Provider type ("ollama", "onnx", or "mock").
    pub provider: String,
    /// Base URL for Ollama HTTP API.
    pub ollama_url: String,
    /// Model identifier for Ollama embeddings.
    pub embed_model: String,
    /// Optional path to ONNX model file or directory.
    pub onnx_model_path: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            ollama_url: memfuse_ollama::DEFAULT_BASE_URL.to_string(),
            embed_model: memfuse_ollama::DEFAULT_EMBED_MODEL.to_string(),
            onnx_model_path: None,
        }
    }
}

impl EmbeddingConfig {
    /// Loads configuration from environment variables with fallbacks.
    pub fn from_env() -> Self {
        let provider = std::env::var("MEMFUSE_EMBEDDING_PROVIDER")
            .or_else(|_| std::env::var("EMBEDDING_PROVIDER"))
            .unwrap_or_else(|_| "ollama".to_string());

        let ollama_url = std::env::var("MEMFUSE_OLLAMA_URL")
            .unwrap_or_else(|_| memfuse_ollama::DEFAULT_BASE_URL.to_string());

        let embed_model = std::env::var("MEMFUSE_EMBED_MODEL")
            .unwrap_or_else(|_| memfuse_ollama::DEFAULT_EMBED_MODEL.to_string());

        let onnx_model_path = std::env::var("MEMFUSE_ONNX_MODEL_PATH")
            .ok()
            .map(PathBuf::from);

        Self {
            provider,
            ollama_url,
            embed_model,
            onnx_model_path,
        }
    }

    /// Instantiates the configured `EmbeddingProvider` as an `Arc<dyn EmbeddingProvider>`.
    pub fn build_provider(&self) -> Result<Arc<dyn EmbeddingProvider>, MemFuseError> {
        create_embedding_provider(
            &self.provider,
            &self.ollama_url,
            &self.embed_model,
            self.onnx_model_path.as_deref(),
        )
    }
}

/// Dynamically constructs an `EmbeddingProvider` implementation based on provider identifier.
pub fn create_embedding_provider(
    provider_type: &str,
    ollama_url: &str,
    embed_model: &str,
    onnx_model_path: Option<&Path>,
) -> Result<Arc<dyn EmbeddingProvider>, MemFuseError> {
    match provider_type.to_lowercase().trim() {
        "ollama" => {
            let embedder = memfuse_ollama::OllamaEmbedder::new(ollama_url, embed_model);
            Ok(Arc::new(embedder))
        }
        #[cfg(feature = "onnx")]
        "onnx" => {
            let path = onnx_model_path.ok_or_else(|| {
                MemFuseError::InvalidInput(
                    "onnx_model_path is required when embedding provider is 'onnx'".to_string(),
                )
            })?;
            let embedder = memfuse_embed::OnnxEmbedder::from_path(path)?;
            Ok(Arc::new(embedder))
        }
        #[cfg(not(feature = "onnx"))]
        "onnx" => {
            let _ = onnx_model_path;
            Err(MemFuseError::CapabilityUnsupported {
                capability: "onnx".to_string(),
                reason:
                    "ONNX support is disabled in this build. Recompile with feature flag 'onnx'."
                        .to_string(),
            })
        }
        "mock" => {
            let embedder = memfuse_core::MockEmbedder::new(768);
            Ok(Arc::new(embedder))
        }
        other => Err(MemFuseError::InvalidInput(format!(
            "Unknown embedding provider '{other}'. Expected 'ollama', 'onnx', or 'mock'."
        ))),
    }
}
