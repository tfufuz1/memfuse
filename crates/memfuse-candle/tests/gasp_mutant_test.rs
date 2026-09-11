// FILE-CONTEXT
// STAND: 2026-09-09T12:44:49Z (SESSION: c74a1828)
// ZWECK: Additional unit tests targeting mutants in GaspValidator and GaspConfig.

use memfuse_candle::gasp::{GaspConfig, GaspValidator};
use memfuse_core::traits::GroundingValidator;
use memfuse_core::{ContextChunk, DocId, MemFuseError};

fn chunk(id: u64, content: &str) -> ContextChunk {
    ContextChunk {
        doc_id: DocId::new(id),
        content: content.to_string(),
        relevance: 1.0,
        token_count: 10,
        metadata: None,
        contextual_prefix: None,
        links: Vec::new(),
    }
}

#[test]
fn test_number_filtering_edge_cases() {
    let validator = GaspValidator::new();
    let chunks = vec![chunk(
        1,
        "Punkt 1 und Punkt 2 sind wichtig. 1. Thema, 2. Thema, 3. Thema.",
    )];

    // Numbers 1, 2, 3 should be ignored as bullet points when single digit
    let score = validator
        .compute_raw_grounding_score("1. Thema, 2. Thema, 3. Thema.", &chunks)
        .unwrap();
    assert!(score > 0.0);

    // Number 12 is 2 digits and should be evaluated against context
    let score_num12 = validator
        .compute_raw_grounding_score("Punkt 12 ist relevant.", &chunks)
        .unwrap();
    // 12 is not in context, so number_score = 0.0, reducing overall raw_score
    assert!(score_num12 < 0.5);
}

#[test]
fn test_word_length_filtering() {
    let validator = GaspValidator::new();
    // Words <= 3 chars (e.g. "der", "die", "das") are ignored in word overlap
    let chunks = vec![chunk(1, "Der Hund bellt laut im Garten.")];
    let score = validator
        .compute_raw_grounding_score("Der Hund bellt.", &chunks)
        .unwrap();
    assert_eq!(score, 1.0);

    // Unmatched words > 3 chars ("Katze", "miaut") lower word score
    let score_unmatched = validator
        .compute_raw_grounding_score("Die Katze miaut sehr laut Draußen.", &chunks)
        .unwrap();
    assert!(score_unmatched < 0.7);
}

#[test]
fn test_gasp_config_default_and_custom() {
    let default_cfg = GaspConfig::default();
    assert_eq!(default_cfg.threshold, 0.70);
    assert_eq!(default_cfg.warmup_required, 10);
    assert_eq!(default_cfg.max_observations, 2000);

    let validator_default = GaspValidator::default();
    assert_eq!(validator_default.threshold(), 0.70);
}

#[tokio::test]
async fn test_gasp_exact_threshold_boundary() {
    let mut validator = GaspValidator::new();
    validator.set_threshold(0.50);

    let chunks = vec![chunk(1, "Alpha Beta Gamma Delta.")];
    // A response yielding high score
    let res = validator
        .validate_grounding("Alpha Beta Gamma Delta.", &chunks)
        .await;
    assert!(res.is_ok());

    validator.set_threshold(0.99);
    // Response with unmatched words yields raw score < 0.99, triggering threshold failure
    let res_high = validator
        .validate_grounding("Alpha Beta Delta FremdWort.", &chunks)
        .await;
    assert!(res_high.is_err());
    match res_high.unwrap_err() {
        MemFuseError::PolicyViolation(msg) => {
            assert!(msg.contains("LowConfidenceGrounding"));
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}
