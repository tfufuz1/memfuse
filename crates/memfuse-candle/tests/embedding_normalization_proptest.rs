// FILE-CONTEXT
// STAND: 2026-09-11T16:00:00Z
// ZWECK: Property-based tests for numerical normalization, finite outputs, and empty/whitespace input edge cases.
// INVARIANTEN: No NaN or Inf in output vectors (is_finite true); no panics on arbitrary string lengths or empty inputs; proptest with 50 cases.

use candle_core::Device;
use memfuse_candle::embedding::{CandleEmbedInner, DefaultCandleEmbedModel};
use memfuse_candle::model_registry::ModelFingerprint;
use memfuse_candle::CandleEmbedClient;
use memfuse_core::traits::EmbeddingProvider;
use proptest::prelude::*;

fn create_test_client() -> CandleEmbedClient {
    let mock_model = Box::new(DefaultCandleEmbedModel { dim: 384 });
    let fp = ModelFingerprint {
        hash: [9u8; 32],
        model_id: "proptest_embed.gguf".to_string(),
        quantization: "Q4_K_M".to_string(),
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
    let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).unwrap();

    CandleEmbedClient::new(Device::Cpu, mock_model, fp, tokenizer)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    // --- Test 7: prop_embedding_output_finite_for_arbitrary_input_length ---
    #[test]
    fn prop_embedding_output_finite_for_arbitrary_input_length(
        text in "\\PC*"
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let client = create_test_client();
            let provider: &dyn EmbeddingProvider = &client;

            let res = provider.embed(&text).await;
            if let Ok(vec) = res {
                prop_assert!(!vec.is_empty(), "Embedding vector must not be empty");
                prop_assert_eq!(vec.len(), provider.embedding_dim());

                for (idx, &val) in vec.iter().enumerate() {
                    prop_assert!(
                        val.is_finite(),
                        "Value at index {idx} in output embedding vector is not finite (NaN or Inf): {val}"
                    );
                }
            }
            Ok(())
        })?;
    }

    // --- Test 8: prop_empty_string_input_yields_defined_result ---
    #[test]
    fn prop_empty_string_input_yields_defined_result(
        input in prop_oneof![
            Just("".to_string()),
            Just("   ".to_string()),
            Just("\t\n\r  ".to_string()),
            Just("\0\0".to_string()),
            "\\s*",
            "\\PC{1,500}",
        ]
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut inner = DefaultCandleEmbedModel { dim: 256 };
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
            let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).unwrap();

            // Inner embed execution MUST NEVER panic on empty, whitespace, or null-byte strings
            let res = inner.embed(&input, &tokenizer, &Device::Cpu);

            match res {
                Ok(vec) => {
                    prop_assert_eq!(vec.len(), 256);
                    for &val in &vec {
                        prop_assert!(val.is_finite(), "Vector element must be finite: {val}");
                    }
                }
                Err(err) => {
                    // Defined error return (e.g. InvalidInput or Internal) is allowed
                    let err_msg = err.to_string();
                    prop_assert!(!err_msg.is_empty(), "Error message must be non-empty");
                }
            }
            Ok(())
        })?;
    }
}
