// memfuse-embed/src/reranker.rs
//! ONNX Cross-Encoder Reranker für Post-RRF Filtering.

#[cfg(feature = "onnx")]
use std::path::Path;
#[cfg(feature = "onnx")]
use std::sync::Arc;

#[cfg(feature = "onnx")]
use memfuse_core::{MemFuseError, Result};
#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;

#[cfg(feature = "onnx")]
use crate::{SessionGuard, SessionPool};

/// Scored pair: (original_index, cross-encoder-score)
pub type RankedCandidate = (usize, f32);

/// ONNX Cross-Encoder Reranker.
///
/// Lädt ein Cross-Encoder-Modell (z. B. `bge-reranker-base`) und bewertet
/// (Query, Passage)-Paare für Post-RRF Reranking.
///
/// # Feature-Flag
/// Nur aktiv mit `features = ["onnx"]`.
#[cfg(feature = "onnx")]
pub struct OnnxCrossEncoderReranker {
    pool: Arc<SessionPool>,
    tokenizer: Arc<Tokenizer>,
    max_sequence_length: usize,
}

#[cfg(feature = "onnx")]
impl OnnxCrossEncoderReranker {
    /// Lädt Cross-Encoder aus Modell-Verzeichnis.
    /// Erwartet: `model.onnx` + `tokenizer.json` (Cross-Encoder-Format).
    pub fn load(model_dir: impl AsRef<Path>, pool_size: usize) -> Result<Self> {
        let path = model_dir.as_ref();
        if !path.join("tokenizer.json").exists() {
            return Err(MemFuseError::InvalidInput(
                "tokenizer.json not found".into(),
            ));
        }
        if !path.join("model.onnx").exists() {
            return Err(MemFuseError::InvalidInput("model.onnx not found".into()));
        }

        let tokenizer = Tokenizer::from_file(path.join("tokenizer.json"))
            .map_err(|e| MemFuseError::Internal(format!("CrossEncoder tokenizer: {e}")))?;

        let mut sessions = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let session = ort::session::Session::builder()
                .map_err(|e| MemFuseError::Internal(format!("Session builder: {e}")))?
                .commit_from_file(path.join("model.onnx"))
                .map_err(|e| MemFuseError::Internal(format!("Model load: {e}")))?;
            sessions.push(session);
        }

        Ok(Self {
            pool: Arc::new(SessionPool::new(sessions)),
            tokenizer: Arc::new(tokenizer),
            max_sequence_length: 512,
        })
    }

    /// Bewertet Kandidaten und gibt sortierte Liste zurück (Index, Score).
    /// Blockiert intern via `spawn_blocking` – safe für async-Kontext.
    pub async fn rerank(
        &self,
        query: &str,
        candidates: Vec<String>,
    ) -> Result<Vec<RankedCandidate>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let pool = Arc::clone(&self.pool);
        let tokenizer = Arc::clone(&self.tokenizer);
        let max_len = self.max_sequence_length;
        let query = query.to_string();

        tokio::task::spawn_blocking(move || {
            let mut session_guard = SessionGuard::new(pool)?;
            let mut scored = Vec::with_capacity(candidates.len());

            for (idx, candidate) in candidates.iter().enumerate() {
                let score =
                    Self::score_pair(&mut session_guard, &tokenizer, &query, candidate, max_len)?;
                scored.push((idx, score));
            }

            // Absteigend sortieren
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            Ok::<_, MemFuseError>(scored)
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("spawn_blocking: {e}")))?
    }

    fn score_pair(
        session: &mut ort::session::Session,
        tokenizer: &Tokenizer,
        query: &str,
        passage: &str,
        max_len: usize,
    ) -> Result<f32> {
        use ort::value::Value;

        // Cross-Encoder: Query und Passage werden als Paar tokenisiert
        let encoding = tokenizer
            .encode((query, passage), true)
            .map_err(|e| MemFuseError::Internal(format!("Tokenization: {e}")))?;

        let input_ids: Vec<i64> = encoding
            .get_ids()
            .iter()
            .map(|&id| id as i64)
            .take(max_len)
            .collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .take(max_len)
            .collect();

        let seq_len = input_ids.len();

        let ids_tensor = Value::from_array(([1usize, seq_len], input_ids))
            .map_err(|e| MemFuseError::Internal(format!("Tensor: {e}")))?;
        let mask_tensor = Value::from_array(([1usize, seq_len], attention_mask))
            .map_err(|e| MemFuseError::Internal(format!("Tensor: {e}")))?;

        let outputs = session
            .run(ort::inputs!["input_ids" => ids_tensor, "attention_mask" => mask_tensor])
            .map_err(|e| MemFuseError::Internal(format!("Inference: {e}")))?;

        // Logit aus Output extrahieren (Cross-Encoder gibt [batch=1, 1] oder [batch=1, 2])
        let output_value = outputs
            .iter()
            .next()
            .ok_or_else(|| MemFuseError::Internal("No output".into()))?
            .1;

        let (_, data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| MemFuseError::Internal(format!("Extract: {e}")))?;

        // Sigmoid für Binary-Cross-Encoder (1 Logit) oder Softmax-Index 1 für 2-class
        let score = if data.len() >= 2 {
            // 2-class: relevance score = softmax(logits)[1]
            let e0 = data[0].exp();
            let e1 = data[1].exp();
            e1 / (e0 + e1)
        } else if !data.is_empty() {
            // Binary: sigmoid(logit)
            1.0 / (1.0 + (-data[0]).exp())
        } else {
            0.0
        };

        Ok(score)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "onnx")]
    use super::*;
    #[cfg(feature = "onnx")]
    use std::fs::File;
    #[cfg(feature = "onnx")]
    use tempfile::tempdir;

    #[cfg(feature = "onnx")]
    #[test]
    fn test_reranker_load_missing_files() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;

        let res = OnnxCrossEncoderReranker::load(dir.path(), 1);
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert!(matches!(err, MemFuseError::InvalidInput(_)));

        File::create(dir.path().join("tokenizer.json"))?;
        let res = OnnxCrossEncoderReranker::load(dir.path(), 1);
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert!(matches!(err, MemFuseError::InvalidInput(_)));

        Ok(())
    }
}
