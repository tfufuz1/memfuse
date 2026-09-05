//! Deterministic Error Matrix Mapper for MemFuse (SPO Framework)
//!
//! Maps every `MemFuseError` and `MemFuseErrorDto` to a deterministic error class
//! and specifies the exact automated reaction for Autonomous Agents, Tauri IPC,
//! and PyO3 Python bindings.

use serde::{Deserialize, Serialize};

/// High-level error classification according to the SPO Specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorClass {
    /// Temporary issues: Retry with exponential backoff.
    Transient,
    /// Invalid input or logical issue: Mutation/correction required.
    Logical,
    /// Physical or unrecoverable error: Abort and escalate immediately.
    Fatal,
    /// System invariant or architecture violation: Quarantine and halt.
    Architectural,
}

/// Prescribed agent action based on the error classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrescribedAction {
    /// Retry with exponential backoff (e.g. 500ms, 1000ms, 2000ms, max 3 attempts).
    RetryExponentialBackoff { max_attempts: u32, base_ms: u64 },
    /// Abort current tactic, reload specification, mutate prompt strategy.
    AbortCurrentApproach { mutation_required: bool },
    /// Rollback local state, create incident report, escalate to human gate.
    AbortAndEscalate,
    /// Quarantine output, freeze all dependent tasks, notify orchestrator.
    HaltAllDependentAgents,
}

/// Enriched error metadata for deterministic IPC / FFI handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPolicy {
    pub class: ErrorClass,
    pub action: PrescribedAction,
    pub http_status: u16,
    pub is_retryable: bool,
}

impl ErrorPolicy {
    /// Resolves the canonical error policy for any given error kind string.
    pub fn for_error_kind(kind: &str) -> Self {
        match kind {
            // TRANSIENT
            "TransactionTimeout" | "SandboxTimeout" | "Conflict" => Self {
                class: ErrorClass::Transient,
                action: PrescribedAction::RetryExponentialBackoff {
                    max_attempts: 3,
                    base_ms: 500,
                },
                http_status: 429,
                is_retryable: true,
            },

            // LOGICAL
            "InvalidInput"
            | "NotFound"
            | "EmbeddingDimensionMismatch"
            | "ParseError"
            | "CapabilityUnsupported"
            | "StaleRead" => Self {
                class: ErrorClass::Logical,
                action: PrescribedAction::AbortCurrentApproach {
                    mutation_required: true,
                },
                http_status: 400,
                is_retryable: false,
            },

            // ARCHITECTURAL / INVARIANT VIOLATION
            "PolicyViolation"
            | "MemoryBudgetExceeded"
            | "MemoryLimitExceeded"
            | "HnswConnectivityDegraded"
            | "InvalidSequenceNumber"
            | "CheckpointNotFound" => Self {
                class: ErrorClass::Architectural,
                action: PrescribedAction::HaltAllDependentAgents,
                http_status: 507,
                is_retryable: false,
            },

            // FATAL (Default for internal, storage, corruption, crypto, IO)
            _ => Self {
                class: ErrorClass::Fatal,
                action: PrescribedAction::AbortAndEscalate,
                http_status: 500,
                is_retryable: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_policy_mapping() {
        let p1 = ErrorPolicy::for_error_kind("TransactionTimeout");
        assert_eq!(p1.class, ErrorClass::Transient);
        assert!(p1.is_retryable);

        let p2 = ErrorPolicy::for_error_kind("InvalidInput");
        assert_eq!(p2.class, ErrorClass::Logical);
        assert!(!p2.is_retryable);

        let p3 = ErrorPolicy::for_error_kind("MemoryBudgetExceeded");
        assert_eq!(p3.class, ErrorClass::Architectural);

        let p4 = ErrorPolicy::for_error_kind("WalCorruption");
        assert_eq!(p4.class, ErrorClass::Fatal);
    }
}
