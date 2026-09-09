// FILE-CONTEXT
// STAND: 2026-09-09T12:44:49Z (SESSION: c74a1828)
// ZWECK: Property-based tests for memfuse-candle components (fingerprinting, GaspValidator, and clients).
// INVARIANTEN: Property tests must cover arbitrary inputs without panicking or producing illegal confidence/grounding scores.

use memfuse_candle::gasp::{GaspConfig, GaspValidator};
use memfuse_candle::model_registry::{compute_fingerprint, CandleQuantization};
use memfuse_core::ContextChunk;
use memfuse_core::DocId;
use proptest::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_proptest_fingerprint_file_content_hash(
        data1 in prop::collection::vec(any::<u8>(), 0..1024),
        data2 in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let mut file1 = NamedTempFile::new().unwrap();
        file1.write_all(&data1).unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(&data2).unwrap();

        let fp1 = compute_fingerprint(file1.path(), &CandleQuantization::Q4KM).unwrap();
        let fp2 = compute_fingerprint(file2.path(), &CandleQuantization::Q4KM).unwrap();

        if data1 == data2 {
            prop_assert_eq!(fp1.hash, fp2.hash);
        } else {
            prop_assert_ne!(fp1.hash, fp2.hash);
        }
    }

    #[test]
    fn test_proptest_gasp_raw_score_bounds(
        response in "\\PC*",
        chunk_content in "\\PC*",
    ) {
        let validator = GaspValidator::new();
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: chunk_content,
            relevance: 1.0,
            token_count: 10,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        if response.trim().is_empty() || chunk.content.trim().is_empty() {
            let res = validator.compute_raw_grounding_score(&response, &[chunk]);
            // Should fail gracefully on empty input/context
            prop_assert!(res.is_err() || (res.is_ok() && (0.0..=1.0).contains(&res.unwrap())));
        } else {
            let res = validator.compute_raw_grounding_score(&response, &[chunk]);
            if let Ok(score) = res {
                prop_assert!(score >= 0.0 && score <= 1.0, "Score {} must be in [0.0, 1.0]", score);
            }
        }
    }

    #[test]
    fn test_proptest_gasp_threshold_configuration(
        threshold in 0.0f32..1.0f32,
    ) {
        let config = GaspConfig {
            threshold,
            ..GaspConfig::default()
        };
        let validator = GaspValidator::with_config(config);
        prop_assert_eq!(validator.threshold(), threshold);
    }
}
