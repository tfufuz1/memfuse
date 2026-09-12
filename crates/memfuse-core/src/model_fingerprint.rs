// FILE-CONTEXT
// STAND: 2026-09-12T00:00:00Z
// ZWECK: Canonical ModelFingerprint type definition shared across workspace crates.
// INVARIANTEN: SHA-256 over weight blob concatenated with quantization string; distinct quant tiers yield distinct fingerprints.

//! Canonical `ModelFingerprint` definition for MemFuse.

use serde::{Deserialize, Serialize};

/// Uniquely identifies a model weight file and its quantization tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFingerprint {
    /// SHA-256 hash digest over model weight blob and quantization tier string.
    pub hash: [u8; 32],
    /// Model identifier or filename (e.g. "llama-3.2-3b-instruct").
    pub model_id: String,
    /// String representation of quantization tier (e.g. "Q4_K_M", "Q8_0", "F16").
    pub quantization: String,
}

impl ModelFingerprint {
    /// Creates a new `ModelFingerprint`.
    pub fn new(
        hash: [u8; 32],
        model_id: impl Into<String>,
        quantization: impl Into<String>,
    ) -> Self {
        Self {
            hash,
            model_id: model_id.into(),
            quantization: quantization.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_fingerprint_new_and_serde() {
        let fp = ModelFingerprint::new([0xab; 32], "test-model", "Q4_K_M");
        assert_eq!(fp.hash, [0xab; 32]);
        assert_eq!(fp.model_id, "test-model");
        assert_eq!(fp.quantization, "Q4_K_M");

        let json = serde_json::to_string(&fp).expect("serialization failed");
        let deserialized: ModelFingerprint =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(fp, deserialized);
    }
}
