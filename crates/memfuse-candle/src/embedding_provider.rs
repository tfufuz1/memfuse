// FILE-CONTEXT
// STAND: 2026-09-09T00:00:00Z (SESSION: CANDLE-EMBEDDING-PROVIDER)
// ZWECK: Trait-based EmbeddingProvider implementation for CandleEmbedClient.
// INVARIANTEN: No block-on in async context; spawn_blocking for CPU inference; embedding_dim strictly matches model dimension.

use memfuse_core::traits::embedding::EmbeddingError;
use memfuse_core::traits::{BoxFuture, EmbeddingProvider};
use std::sync::Arc;

use crate::embedding::CandleEmbedClient;

/// Maximum batch size for Candle GGUF embeddings.
///
/// Set to 256 (lower than ONNX's 512) because GGUF tensor activations and intermediate FP32/FP16 buffers
/// consume higher peak VRAM/RAM per batch item during Candle model forward passes.
pub const MAX_CANDLE_EMBED_BATCH_SIZE: usize = 256;

impl EmbeddingProvider for CandleEmbedClient {
    fn provider_name(&self) -> &str {
        "candle"
    }

    fn embedding_dim(&self) -> usize {
        // Derived dynamically from model/fingerprint metadata stored in self.dim
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

    fn embed_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, std::result::Result<Vec<Vec<f32>>, EmbeddingError>> {
        Box::pin(async move {
            let limit = MAX_CANDLE_EMBED_BATCH_SIZE;
            if texts.len() > limit {
                return Err(EmbeddingError::Unavailable(format!(
                    "Batch size {} exceeds Candle max_batch_size {limit}. Split into smaller batches.",
                    texts.len()
                )));
            }

            let model = Arc::clone(&self.model);
            let tokenizer = self.tokenizer.clone();
            let device = self.device.clone();
            let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

            tokio::task::spawn_blocking(move || {
                let mut guard = model.blocking_lock();
                let mut results = Vec::with_capacity(texts_owned.len());
                for text in &texts_owned {
                    let vec = guard
                        .embed(text, &tokenizer, &device)
                        .map_err(|e| EmbeddingError::ComputationFailed(e.to_string()))?;
                    results.push(vec);
                }
                Ok(results)
            })
            .await
            .map_err(|e| {
                EmbeddingError::ComputationFailed(format!("Candle task join error: {e}"))
            })?
        })
    }
}
