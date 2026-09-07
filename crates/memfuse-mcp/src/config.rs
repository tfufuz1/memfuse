use memfuse_core::{EmbeddingProvider, LlmTextGenerator, MemFuseError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Embedding provider configuration settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingConfig {
    /// Provider type ("ollama", "onnx", "candle", or "mock").
    pub provider: String,
    /// Base URL for Ollama HTTP API.
    pub ollama_url: String,
    /// Model identifier for Ollama embeddings.
    pub embed_model: String,
    /// Optional path to ONNX model file or directory.
    pub onnx_model_path: Option<PathBuf>,
    /// Optional path to Candle model directory.
    pub candle_model_dir: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            ollama_url: memfuse_ollama::DEFAULT_BASE_URL.to_string(),
            embed_model: memfuse_ollama::DEFAULT_EMBED_MODEL.to_string(),
            onnx_model_path: None,
            candle_model_dir: None,
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

        let candle_model_dir = std::env::var("MEMFUSE_CANDLE_MODEL_DIR")
            .ok()
            .map(PathBuf::from);

        Self {
            provider,
            ollama_url,
            embed_model,
            onnx_model_path,
            candle_model_dir,
        }
    }

    /// Instantiates the configured `EmbeddingProvider` as an `Arc<dyn EmbeddingProvider>`.
    pub fn build_provider(&self) -> Result<Arc<dyn EmbeddingProvider>, MemFuseError> {
        create_embedding_provider(
            &self.provider,
            &self.ollama_url,
            &self.embed_model,
            self.onnx_model_path.as_deref(),
            self.candle_model_dir.as_deref(),
        )
    }
}

/// Dynamically constructs an `EmbeddingProvider` implementation based on provider identifier.
pub fn create_embedding_provider(
    provider_type: &str,
    ollama_url: &str,
    embed_model: &str,
    onnx_model_path: Option<&Path>,
    candle_model_dir: Option<&Path>,
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
        #[cfg(feature = "candle")]
        "candle" => {
            let _ = (ollama_url, embed_model, onnx_model_path);
            let model_dir = candle_model_dir.ok_or_else(|| {
                MemFuseError::InvalidInput(
                    "candle_model_dir is required when embedding provider is 'candle'".to_string(),
                )
            })?;
            let quantization = memfuse_candle::model_registry::CandleQuantization::Q4KM;
            let embedder = memfuse_candle::CandleEmbedClient::from_dir(model_dir, quantization)
                .map_err(|e| MemFuseError::Internal(format!("Failed to load Candle embed model: {e}")))?;
            Ok(Arc::new(embedder))
        }
        #[cfg(not(feature = "candle"))]
        "candle" => {
            let _ = (ollama_url, embed_model, onnx_model_path, candle_model_dir);
            Err(MemFuseError::CapabilityUnsupported {
                capability: "candle embedding backend".to_string(),
                reason: "memfuse-mcp was built without the 'candle' feature".to_string(),
            })
        }
        "mock" => {
            let embedder = memfuse_core::MockEmbedder::new(768);
            Ok(Arc::new(embedder))
        }
        other => Err(MemFuseError::InvalidInput(format!(
            "Unknown embedding provider '{other}'. Expected 'ollama', 'onnx', 'candle', or 'mock'."
        ))),
    }
}

/// LLM provider configuration settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmConfig {
    /// Provider type ("ollama", "candle", or "mock").
    pub provider: String,
    /// Base URL for Ollama HTTP API.
    pub ollama_url: String,
    /// Model identifier for Ollama LLM.
    pub llm_model: String,
    /// Optional path to Candle model directory.
    pub candle_model_dir: Option<PathBuf>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            ollama_url: memfuse_ollama::DEFAULT_BASE_URL.to_string(),
            llm_model: "llama3.2:3b".to_string(),
            candle_model_dir: None,
        }
    }
}

impl LlmConfig {
    /// Loads configuration from environment variables with fallbacks.
    pub fn from_env() -> Self {
        let provider = std::env::var("MEMFUSE_LLM_PROVIDER")
            .or_else(|_| std::env::var("LLM_PROVIDER"))
            .unwrap_or_else(|_| "ollama".to_string());

        let ollama_url = std::env::var("MEMFUSE_OLLAMA_URL")
            .unwrap_or_else(|_| memfuse_ollama::DEFAULT_BASE_URL.to_string());

        let llm_model = std::env::var("MEMFUSE_LLM_MODEL")
            .unwrap_or_else(|_| "llama3.2:3b".to_string());

        let candle_model_dir = std::env::var("MEMFUSE_CANDLE_MODEL_DIR")
            .ok()
            .map(PathBuf::from);

        Self {
            provider,
            ollama_url,
            llm_model,
            candle_model_dir,
        }
    }

    /// Instantiates the configured `LlmTextGenerator` as an `Arc<dyn LlmTextGenerator>`.
    pub fn build_generator(&self) -> Result<Arc<dyn LlmTextGenerator>, MemFuseError> {
        create_llm_text_generator(
            &self.provider,
            &self.ollama_url,
            &self.llm_model,
            self.candle_model_dir.as_deref(),
        )
    }
}

/// Dynamically constructs an `LlmTextGenerator` implementation based on provider identifier.
pub fn create_llm_text_generator(
    provider_type: &str,
    ollama_url: &str,
    llm_model: &str,
    candle_model_dir: Option<&Path>,
) -> Result<Arc<dyn LlmTextGenerator>, MemFuseError> {
    match provider_type.to_lowercase().trim() {
        "ollama" => {
            let mut config = memfuse_ollama::OllamaConfig::default();
            config.base_url = ollama_url.to_string();
            config.model = llm_model.to_string();
            let client = memfuse_ollama::OllamaClient::with_config(config);
            Ok(Arc::new(client))
        }
        #[cfg(feature = "candle")]
        "candle" => {
            let _ = (ollama_url, llm_model);
            let model_dir = candle_model_dir.ok_or_else(|| {
                MemFuseError::InvalidInput(
                    "candle_model_dir is required when LLM provider is 'candle'".to_string(),
                )
            })?;
            let quantization = memfuse_candle::model_registry::CandleQuantization::Q4KM;
            let generator = memfuse_candle::CandleLlmClient::from_dir(model_dir, quantization)
                .map_err(|e| MemFuseError::Internal(format!("Failed to load Candle LLM model: {e}")))?;
            Ok(Arc::new(generator))
        }
        #[cfg(not(feature = "candle"))]
        "candle" => {
            let _ = (ollama_url, llm_model, candle_model_dir);
            Err(MemFuseError::CapabilityUnsupported {
                capability: "candle LLM backend".to_string(),
                reason: "memfuse-mcp was built without the 'candle' feature".to_string(),
            })
        }
        "mock" => {
            let generator = MockLlmGenerator;
            Ok(Arc::new(generator))
        }
        other => Err(MemFuseError::InvalidInput(format!(
            "Unknown LLM provider '{other}'. Expected 'ollama', 'candle', or 'mock'."
        ))),
    }
}

/// Fallback Mock LLM Text Generator.
#[derive(Debug, Clone, Default)]
pub struct MockLlmGenerator;

impl LlmTextGenerator for MockLlmGenerator {
    fn generate<'a>(
        &'a self,
        prompt: &'a str,
    ) -> memfuse_core::traits::BoxFuture<'a, Result<String, MemFuseError>> {
        let response = format!("[Mock LLM response for: {prompt}]");
        Box::pin(async move { Ok(response) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.ollama_url, memfuse_ollama::DEFAULT_BASE_URL);
        assert_eq!(config.embed_model, memfuse_ollama::DEFAULT_EMBED_MODEL);
        assert!(config.onnx_model_path.is_none());
        assert!(config.candle_model_dir.is_none());
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.ollama_url, memfuse_ollama::DEFAULT_BASE_URL);
        assert_eq!(config.llm_model, "llama3.2:3b");
        assert!(config.candle_model_dir.is_none());
    }

    #[test]
    fn test_create_embedding_provider_mock() {
        let provider = create_embedding_provider("mock", "http://localhost:11434", "nomic-embed-text", None, None).unwrap();
        assert_eq!(provider.provider_name(), "mock");
        assert_eq!(provider.embedding_dim(), 768);
    }

    #[test]
    fn test_create_embedding_provider_ollama() {
        let provider = create_embedding_provider("ollama", "http://localhost:11434", "nomic-embed-text", None, None).unwrap();
        assert_eq!(provider.provider_name(), "ollama");
    }

    #[test]
    fn test_create_embedding_provider_unknown_error() {
        let res = create_embedding_provider("invalid_provider", "http://localhost:11434", "nomic-embed-text", None, None);
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[test]
    fn test_create_llm_generator_mock() {
        let generator = create_llm_text_generator("mock", "http://localhost:11434", "llama3.2:3b", None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let response = rt.block_on(generator.generate("hello")).unwrap();
        assert!(response.contains("[Mock LLM response for: hello]"));
    }

    #[test]
    fn test_create_llm_generator_ollama() {
        let generator = create_llm_text_generator("ollama", "http://localhost:11434", "llama3.2:3b", None);
        assert!(generator.is_ok());
    }

    #[test]
    fn test_create_llm_generator_unknown_error() {
        let res = create_llm_text_generator("invalid_provider", "http://localhost:11434", "llama3.2:3b", None);
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[cfg(feature = "candle")]
    #[test]
    fn test_create_embedding_provider_candle_success() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = create_embedding_provider("candle", "http://localhost:11434", "embed_model", None, Some(tmp.path())).unwrap();
        assert_eq!(provider.provider_name(), "candle");
    }

    #[cfg(feature = "candle")]
    #[test]
    fn test_create_embedding_provider_candle_missing_dir_error() {
        let res = create_embedding_provider("candle", "http://localhost:11434", "embed_model", None, None);
        match res {
            Err(err) => {
                assert!(matches!(err, MemFuseError::InvalidInput(_)));
                assert!(err.to_string().contains("candle_model_dir is required"));
            }
            Ok(_) => panic!("Expected error for missing candle_model_dir"),
        }
    }

    #[cfg(not(feature = "candle"))]
    #[test]
    fn test_create_embedding_provider_candle_unsupported_error() {
        let res = create_embedding_provider("candle", "http://localhost:11434", "embed_model", None, None);
        assert!(matches!(res, Err(MemFuseError::CapabilityUnsupported { .. })));
    }

    #[cfg(feature = "candle")]
    #[test]
    fn test_create_llm_generator_candle_success() {
        let tmp = tempfile::tempdir().unwrap();
        let generator = create_llm_text_generator("candle", "http://localhost:11434", "llama3.2:3b", Some(tmp.path())).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let response = rt.block_on(generator.generate("hello")).unwrap();
        assert!(response.contains("[Candle] Response for prompt: hello"));
    }

    #[cfg(feature = "candle")]
    #[test]
    fn test_create_llm_generator_candle_missing_dir_error() {
        let res = create_llm_text_generator("candle", "http://localhost:11434", "llama3.2:3b", None);
        match res {
            Err(err) => {
                assert!(matches!(err, MemFuseError::InvalidInput(_)));
                assert!(err.to_string().contains("candle_model_dir is required"));
            }
            Ok(_) => panic!("Expected error for missing candle_model_dir"),
        }
    }

    #[cfg(not(feature = "candle"))]
    #[test]
    fn test_create_llm_generator_candle_unsupported_error() {
        let res = create_llm_text_generator("candle", "http://localhost:11434", "llama3.2:3b", None);
        assert!(matches!(res, Err(MemFuseError::CapabilityUnsupported { .. })));
    }
}
