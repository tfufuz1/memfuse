use crate::model_registry::ModelFingerprint;
use candle_core::Device;
use memfuse_core::traits::embedding::EmbeddingError;
use memfuse_core::traits::{BoxFuture, EmbeddingProvider};
use memfuse_core::Result;
use std::sync::Arc;

/// Inner trait abstracting low-level Candle forward execution for vector embeddings.
///
/// Enables mock-based unit testing without loading full ONNX or GGUF files in CI.
pub trait CandleEmbedInner: Send {
    /// Generates an embedding vector for the provided input text.
    fn embed(
        &mut self,
        text: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
    ) -> Result<Vec<f32>>;

    /// Returns the vector dimension produced by this embedding model.
    fn dim(&self) -> usize;
}

/// Text embedding client powered by Candle ML backend.
pub struct CandleEmbedClient {
    /// Hardware device (CPU, CUDA, Metal).
    pub device: Device,
    /// Thread-safe mutex wrapping the inner embedding model.
    pub model: Arc<tokio::sync::Mutex<Box<dyn CandleEmbedInner + Send>>>,
    /// Unique fingerprint identifying the embedding model weights and quantization.
    pub fingerprint: ModelFingerprint,
    /// Tokenizer for converting text to token ID tensors.
    pub tokenizer: tokenizers::Tokenizer,
    /// Vector dimension produced by this model.
    pub dim: usize,
}

impl CandleEmbedClient {
    /// Creates a new `CandleEmbedClient`.
    pub fn new(
        device: Device,
        model: Box<dyn CandleEmbedInner + Send>,
        fingerprint: ModelFingerprint,
        tokenizer: tokenizers::Tokenizer,
    ) -> Self {
        let dim = model.dim();
        Self {
            device,
            model: Arc::new(tokio::sync::Mutex::new(model)),
            fingerprint,
            tokenizer,
            dim,
        }
    }

    /// Returns a reference to the model's fingerprint.
    pub fn fingerprint(&self) -> &ModelFingerprint {
        &self.fingerprint
    }
}

impl EmbeddingProvider for CandleEmbedClient {
    fn provider_name(&self) -> &str {
        "candle"
    }

    fn embedding_dim(&self) -> usize {
        self.dim
    }

    fn embed<'a>(
        &'a self,
        text: &'a str,
    ) -> BoxFuture<'a, std::result::Result<Vec<f32>, EmbeddingError>> {
        let model = Arc::clone(&self.model);
        let tokenizer = self.tokenizer.clone();
        let device = self.device.clone();
        let text_owned = text.to_string();

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut guard = model.blocking_lock();
                guard
                    .embed(&text_owned, &tokenizer, &device)
                    .map_err(|e| EmbeddingError::ComputationFailed(e.to_string()))
            })
            .await
            .map_err(|e| {
                EmbeddingError::ComputationFailed(format!("Candle task join error: {e}"))
            })?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::traits::TextEmbeddingEngine;

    struct MockEmbedModel {
        dim: usize,
    }

    impl CandleEmbedInner for MockEmbedModel {
        fn embed(
            &mut self,
            _text: &str,
            _tokenizer: &tokenizers::Tokenizer,
            _device: &Device,
        ) -> Result<Vec<f32>> {
            Ok(vec![0.5f32; self.dim])
        }

        fn dim(&self) -> usize {
            self.dim
        }
    }

    #[tokio::test]
    async fn test_candle_embed_client_provider_and_engine() {
        let mock_model = Box::new(MockEmbedModel { dim: 4 });
        let fingerprint = ModelFingerprint {
            hash: [2u8; 32],
            model_id: "embed_model.gguf".to_string(),
            quantization: "Q8_0".to_string(),
        };
        let tokenizer_bytes = r#"{
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": null,
            "post_processor": null,
            "decoder": null,
            "model": { "type": "BPE", "dropout": null, "unk_token": null, "continuing_subword_prefix": null, "end_of_word_suffix": null, "fuse_unk": false, "vocab": {}, "merges": [] }
        }"#;
        let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes())
            .map_err(|e| e.to_string())
            .unwrap();

        let client =
            CandleEmbedClient::new(Device::Cpu, mock_model, fingerprint.clone(), tokenizer);

        assert_eq!(client.provider_name(), "candle");
        assert_eq!(client.embedding_dim(), 4);
        assert_eq!(client.fingerprint(), &fingerprint);

        let vec = EmbeddingProvider::embed(&client, "test sentence")
            .await
            .unwrap();
        assert_eq!(vec, vec![0.5f32, 0.5f32, 0.5f32, 0.5f32]);

        // Test blanket TextEmbeddingEngine
        let engine: &dyn TextEmbeddingEngine = &client;
        let vec_engine = engine.embed("another test").await.unwrap();
        assert_eq!(vec_engine, vec![0.5f32; 4]);
    }
}
