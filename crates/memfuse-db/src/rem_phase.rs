//! REM Phase (Rapid Eye Movement) — Generative Memory Synthesis.
//!
//! ARCHITEKTUR-HINWEIS: Diese Phase ist BEWUSST von der NREM-Phase getrennt (eigenes Modul),
//! da sie LLM-API-Abhängigkeiten hat (memfuse-ollama), die in der NREM-Phase verboten sind.
//! NREM → statisch, deterministisch, kein LLM
//! REM  → generativ, LLM-abhängig, stochastisch
//!
//! SEGMENT-LEVEL (nicht Turn-Level): Jeder REM-Synthesized-Chunk abstrahiert ein ganzes
//! TurnSegment aus der NREM-Phase. Dies entspricht LycheeMemory V2 (arXiv:2608.12990).

use crate::sleep_cycle::TurnSegment;
use memfuse_core::{BoxFuture, DocId, Result};

/// Ergebnis der REM-Konsolidierungsphase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemPhaseResult {
    /// Neu synthetisierte Chunks (einer pro konsolidiertem Segment).
    pub synthesized_chunks: Vec<SynthesizedChunk>,
    /// Anzahl der Segmente, für die keine Synthese möglich war (LLM-Fehler / zu kurz).
    pub skipped_segments: usize,
}

/// Ein generativ synthetisierter Wissens-Chunk mit Provenienz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesizedChunk {
    /// Der neue, abstrakte Inhalt.
    pub content: String,
    /// DocIds der Quell-Turns aus dem ursprünglichen Segment.
    pub source_turn_ids: Vec<DocId>,
    /// Modell, das zur Synthese verwendet wurde.
    pub model_id: String,
}

/// Trait-Abstraktion für den LLM-Synthesizer (testbar via Mock).
pub use memfuse_core::SegmentSynthesizer;

/// Führt die REM-Phase aus: synthetisiert pro Segment einen abstrakten Chunk.
///
/// # Fehlerverhalten
/// Einzelne Segment-Fehler werden übersprungen (`skipped_segments` inkrementiert),
/// nie als Err propagiert. Globale Fehler (Trait-Fehler im Setup) propagieren als Err.
pub async fn run_rem_phase(
    segments: &[TurnSegment],
    segment_texts: &[Vec<String>], // Texte der Turns pro Segment
    synthesizer: &dyn SegmentSynthesizer,
    min_turns_for_rem: usize,      // Default: 3 — kurze Segmente überspringen
) -> RemPhaseResult {
    let mut synthesized_chunks = Vec::new();
    let mut skipped_segments = 0;

    for (i, segment) in segments.iter().enumerate() {
        if segment.turn_ids.len() < min_turns_for_rem {
            skipped_segments += 1;
            continue;
        }

        let texts = match segment_texts.get(i) {
            Some(t) => t,
            None => {
                skipped_segments += 1;
                continue;
            }
        };

        if texts.is_empty() {
            skipped_segments += 1;
            continue;
        }

        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        match synthesizer.synthesize_segment(&text_refs).await {
            Ok(content) => {
                synthesized_chunks.push(SynthesizedChunk {
                    content,
                    source_turn_ids: segment.turn_ids.clone(),
                    model_id: synthesizer.model_id().to_string(),
                });
            }
            Err(e) => {
                tracing::warn!(
                    segment_idx = i,
                    error = %e,
                    "REM phase segment synthesis failed; skipping segment"
                );
                skipped_segments += 1;
            }
        }
    }

    RemPhaseResult {
        synthesized_chunks,
        skipped_segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockSynthesizer {
        model: String,
        should_fail: AtomicBool,
    }

    impl MockSynthesizer {
        fn new(model: &str, should_fail: bool) -> Self {
            Self {
                model: model.to_string(),
                should_fail: AtomicBool::new(should_fail),
            }
        }
    }

    impl SegmentSynthesizer for MockSynthesizer {
        fn synthesize_segment<'a>(&'a self, segment_texts: &'a [&'a str]) -> BoxFuture<'a, Result<String>> {
            Box::pin(async move {
                if self.should_fail.load(Ordering::SeqCst) {
                    Err(memfuse_core::MemFuseError::Internal(
                        "Mock LLM synthesis error".into(),
                    ))
                } else {
                    Ok(format!("Synthesized: {}", segment_texts.join(" + ")))
                }
            })
        }

        fn model_id(&self) -> &str {
            &self.model
        }
    }

    #[tokio::test]
    async fn test_rem_phase_empty_segments_returns_empty() {
        let synthesizer = MockSynthesizer::new("test-model", false);
        let res = run_rem_phase(&[], &[], &synthesizer, 3).await;

        assert_eq!(res.synthesized_chunks.len(), 0);
        assert_eq!(res.skipped_segments, 0);
    }

    #[tokio::test]
    async fn test_rem_phase_too_short_segments_skipped() {
        let synthesizer = MockSynthesizer::new("test-model", false);
        let segment = TurnSegment {
            turn_ids: vec![DocId::new(1), DocId::new(2)], // 2 turns < min_turns_for_rem=3
            representative_embedding: vec![1.0, 0.0],
        };
        let texts = vec![vec!["text 1".to_string(), "text 2".to_string()]];

        let res = run_rem_phase(&[segment], &texts, &synthesizer, 3).await;

        assert_eq!(res.synthesized_chunks.len(), 0);
        assert_eq!(res.skipped_segments, 1);
    }

    #[tokio::test]
    async fn test_rem_phase_llm_error_increments_skipped_not_err() {
        let synthesizer = MockSynthesizer::new("test-model", true); // should_fail = true
        let segment = TurnSegment {
            turn_ids: vec![DocId::new(1), DocId::new(2), DocId::new(3)],
            representative_embedding: vec![1.0, 0.0],
        };
        let texts = vec![vec![
            "text 1".to_string(),
            "text 2".to_string(),
            "text 3".to_string(),
        ]];

        let res = run_rem_phase(&[segment], &texts, &synthesizer, 3).await;

        assert_eq!(res.synthesized_chunks.len(), 0);
        assert_eq!(res.skipped_segments, 1);
    }

    #[tokio::test]
    async fn test_rem_phase_synthesized_chunk_has_correct_source_ids() {
        let synthesizer = MockSynthesizer::new("test-model", false);
        let segment = TurnSegment {
            turn_ids: vec![DocId::new(10), DocId::new(20), DocId::new(30)],
            representative_embedding: vec![1.0, 0.0],
        };
        let texts = vec![vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
        ]];

        let res = run_rem_phase(&[segment], &texts, &synthesizer, 3).await;

        assert_eq!(res.synthesized_chunks.len(), 1);
        assert_eq!(res.skipped_segments, 0);

        let chunk = &res.synthesized_chunks[0];
        assert_eq!(chunk.content, "Synthesized: A + B + C");
        assert_eq!(
            chunk.source_turn_ids,
            vec![DocId::new(10), DocId::new(20), DocId::new(30)]
        );
        assert_eq!(chunk.model_id, "test-model");
    }
}
