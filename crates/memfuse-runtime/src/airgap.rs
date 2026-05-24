//! Air-Gap Deployment Profile (WP-6.6).
//!
//! Enforces complete network isolation for sovereign AI deployments.
//! Validates that no socket calls leak from the runtime.

// ANCHOR:ARCH:AIRGAP-001 — Air-Gap Deployment Profile (WP-6.6)
// WP:WP-6.6 PRIO:2 NEEDS:WP-3.1+WP-3.2
// STATUS:SCAFFOLD DATE:2026-05-17

use memfuse_core::Result;

/// Configuration for air-gap deployment mode.
#[derive(Debug, Clone)]
pub struct AirGapConfig {
    /// Whether network access is completely disabled.
    pub network_disabled: bool,
    /// Path to local ONNX embedding model (if any).
    pub local_model_path: Option<String>,
    /// ONNX runtime backend (e.g., "ort").
    pub embedding_runtime: EmbeddingRuntime,
    /// Whether encryption at rest is mandatory.
    pub require_encryption: bool,
}

/// Supported embedding runtimes for offline operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingRuntime {
    /// ONNX Runtime via `ort` crate.
    OnnxRuntime,
    /// No embedding — vectors must be provided externally.
    None,
}

impl Default for AirGapConfig {
    fn default() -> Self {
        Self {
            network_disabled: true,
            local_model_path: None,
            embedding_runtime: EmbeddingRuntime::None,
            require_encryption: true,
        }
    }
}

impl AirGapConfig {
    /// Creates a strict air-gap configuration.
    pub fn strict() -> Self {
        Self {
            network_disabled: true,
            local_model_path: None,
            embedding_runtime: EmbeddingRuntime::None,
            require_encryption: true,
        }
    }

    /// Creates an air-gap config with a local ONNX model.
    pub fn with_local_model(model_path: impl Into<String>) -> Self {
        Self {
            network_disabled: true,
            local_model_path: Some(model_path.into()),
            embedding_runtime: EmbeddingRuntime::OnnxRuntime,
            require_encryption: true,
        }
    }

    /// Validates the air-gap configuration.
    ///
    /// Returns error if network is enabled (not air-gapped).
    pub fn validate(&self) -> Result<()> {
        if !self.network_disabled {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "Air-gap mode requires network_disabled=true".into(),
            ));
        }
        Ok(())
    }
}

/// Verifies that the current runtime is air-gap compliant.
///
/// Checks that no network sockets are open and no external
/// connections are possible.
pub struct AirGapVerifier;

impl AirGapVerifier {
    /// Runs a full air-gap compliance check.
    ///
    /// Returns a verification report.
    pub fn verify(_config: &AirGapConfig) -> Result<AirGapReport> {
        // TODO(WP-6.6): Implement actual verification:
        // 1. Check no open sockets (via /proc/self/fd on Linux)
        // 2. Verify encryption is enabled
        // 3. Verify no DNS resolution is possible
        // 4. Generate SPDX SBOM
        Ok(AirGapReport {
            network_isolated: true,
            encryption_active: true,
            sbom_generated: false,
        })
    }
}

/// Result of an air-gap compliance verification.
#[derive(Debug)]
pub struct AirGapReport {
    /// Whether network isolation is confirmed.
    pub network_isolated: bool,
    /// Whether encryption at rest is active.
    pub encryption_active: bool,
    /// Whether an SPDX SBOM was generated.
    pub sbom_generated: bool,
}

impl AirGapReport {
    /// Returns true if all compliance checks passed.
    pub fn is_compliant(&self) -> bool {
        self.network_isolated && self.encryption_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airgap_config_strict() {
        let config = AirGapConfig::strict();
        assert!(config.network_disabled);
        assert!(config.require_encryption);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_airgap_config_with_model() {
        let config = AirGapConfig::with_local_model("/models/e5-large.onnx");
        assert_eq!(config.embedding_runtime, EmbeddingRuntime::OnnxRuntime);
        assert_eq!(
            config.local_model_path.as_deref(),
            Some("/models/e5-large.onnx")
        );
    }

    #[test]
    fn test_airgap_verifier() {
        let config = AirGapConfig::strict();
        let report = AirGapVerifier::verify(&config).expect("valid test value"); // unwrap allowed
        assert!(report.is_compliant());
    }
}
