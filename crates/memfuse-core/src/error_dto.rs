//! Serializable DTO representation of MemFuseError for IPC and FFI boundaries (ADR-028).

use crate::error::MemFuseError;
use serde::{Deserialize, Serialize};

/// Serializable data transfer object representing a [`MemFuseError`].
///
/// Used across IPC and API boundaries (e.g. Tauri frontend IPC) to preserve structured
/// error kinds and optional JSON detail fields without losing error typing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemFuseErrorDto {
    /// Stable string identifier for the error variant (e.g. `"NotFound"`, `"PolicyViolation"`).
    pub kind: String,
    /// Human-readable error message.
    pub message: String,
    /// Structured detail payload for complex error variants (e.g. offset/reason for WAL corruption).
    pub details: Option<serde_json::Value>,
}

impl MemFuseErrorDto {
    /// Creates a new `MemFuseErrorDto` with custom kind and message.
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Creates a new `MemFuseErrorDto` with custom kind, message, and details payload.
    pub fn with_details(
        kind: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            details: Some(details),
        }
    }
}

impl std::fmt::Display for MemFuseErrorDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]: {}", self.kind, self.message)
    }
}

impl std::error::Error for MemFuseErrorDto {}

impl From<MemFuseError> for MemFuseErrorDto {
    fn from(err: MemFuseError) -> Self {
        Self::from(&err)
    }
}

impl From<String> for MemFuseErrorDto {
    fn from(msg: String) -> Self {
        Self {
            kind: "InvalidInput".to_string(),
            message: msg,
            details: None,
        }
    }
}

impl From<&str> for MemFuseErrorDto {
    fn from(msg: &str) -> Self {
        Self {
            kind: "InvalidInput".to_string(),
            message: msg.to_string(),
            details: None,
        }
    }
}

impl From<&MemFuseError> for MemFuseErrorDto {
    fn from(err: &MemFuseError) -> Self {
        // NOTE: Strictly no catch-all `_ => ...` wildcard arm in this match expression.
        // Every single variant of MemFuseError must be explicitly listed below.
        // If a new variant is added to MemFuseError, Rust compilation will fail here
        // until From<&MemFuseError> is deliberately updated.
        match err {
            MemFuseError::Internal(msg) => Self {
                kind: "Internal".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::InvalidInput(msg) => Self {
                kind: "InvalidInput".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::NotFound(msg) => Self {
                kind: "NotFound".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::PolicyViolation(msg) => Self {
                kind: "PolicyViolation".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::NamespaceViolation(msg) => Self {
                kind: "NamespaceViolation".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::Storage(msg) => Self {
                kind: "Storage".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::Io(io_err) => Self {
                kind: "Io".to_string(),
                message: io_err.to_string(),
                details: None,
            },
            MemFuseError::WalCorruption { offset, reason } => Self {
                kind: "WalCorruption".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "offset": offset,
                    "reason": reason,
                })),
            },
            MemFuseError::ChecksumMismatch { path, block_id } => Self {
                kind: "ChecksumMismatch".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "path": path,
                    "block_id": block_id,
                })),
            },
            MemFuseError::Transaction(msg) => Self {
                kind: "Transaction".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::TransactionTimeout { tx_id, elapsed_ms } => Self {
                kind: "TransactionTimeout".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "tx_id": tx_id,
                    "elapsed_ms": elapsed_ms,
                })),
            },
            MemFuseError::Conflict(msg) => Self {
                kind: "Conflict".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::InvalidSequenceNumber(seq_no) => Self {
                kind: "InvalidSequenceNumber".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "seq_no": seq_no,
                })),
            },
            MemFuseError::Index(msg) => Self {
                kind: "Index".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::HnswConnectivityDegraded { deleted_ratio } => Self {
                kind: "HnswConnectivityDegraded".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "deleted_ratio": deleted_ratio,
                })),
            },
            MemFuseError::Text(msg) => Self {
                kind: "Text".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::MemoryBudgetExceeded { used_mb, limit_mb } => Self {
                kind: "MemoryBudgetExceeded".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "used_mb": used_mb,
                    "limit_mb": limit_mb,
                })),
            },
            MemFuseError::Sandbox(msg) => Self {
                kind: "Sandbox".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::MemoryLimitExceeded(msg) => Self {
                kind: "MemoryLimitExceeded".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::SandboxTimeout(msg) => Self {
                kind: "SandboxTimeout".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::Serialization(msg) => Self {
                kind: "Serialization".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::Json(json_err) => Self {
                kind: "Json".to_string(),
                message: json_err.to_string(),
                details: None,
            },
            MemFuseError::Crypto(msg) => Self {
                kind: "Crypto".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::CheckpointNotFound => Self {
                kind: "CheckpointNotFound".to_string(),
                message: "Checkpoint not found".to_string(),
                details: None,
            },
            MemFuseError::Cluster(msg) => Self {
                kind: "Cluster".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::ParseError(msg) => Self {
                kind: "ParseError".to_string(),
                message: msg.clone(),
                details: None,
            },
            MemFuseError::Bincode(bincode_err) => Self {
                kind: "Bincode".to_string(),
                message: bincode_err.to_string(),
                details: None,
            },
            MemFuseError::CapabilityUnsupported { capability, reason } => Self {
                kind: "CapabilityUnsupported".to_string(),
                message: err.to_string(),
                details: Some(serde_json::json!({
                    "capability": capability,
                    "reason": reason,
                })),
            },
            MemFuseError::ResourceExhausted(msg) => Self {
                kind: "ResourceExhausted".to_string(),
                message: msg.clone(),
                details: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dto_exhaustive_match_coverage() {
        let variants = vec![
            (MemFuseError::Internal("test".into()), "Internal"),
            (MemFuseError::InvalidInput("test".into()), "InvalidInput"),
            (MemFuseError::NotFound("test".into()), "NotFound"),
            (
                MemFuseError::PolicyViolation("test".into()),
                "PolicyViolation",
            ),
            (
                MemFuseError::NamespaceViolation("test".into()),
                "NamespaceViolation",
            ),
            (MemFuseError::Storage("test".into()), "Storage"),
            (
                MemFuseError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                )),
                "Io",
            ),
            (
                MemFuseError::WalCorruption {
                    offset: 100,
                    reason: "bad header".into(),
                },
                "WalCorruption",
            ),
            (
                MemFuseError::ChecksumMismatch {
                    path: "/tmp/a".into(),
                    block_id: 42,
                },
                "ChecksumMismatch",
            ),
            (MemFuseError::Transaction("test".into()), "Transaction"),
            (
                MemFuseError::TransactionTimeout {
                    tx_id: 1,
                    elapsed_ms: 500,
                },
                "TransactionTimeout",
            ),
            (MemFuseError::Conflict("test".into()), "Conflict"),
            (
                MemFuseError::InvalidSequenceNumber(10),
                "InvalidSequenceNumber",
            ),
            (MemFuseError::Index("test".into()), "Index"),
            (
                MemFuseError::HnswConnectivityDegraded { deleted_ratio: 0.2 },
                "HnswConnectivityDegraded",
            ),
            (MemFuseError::Text("test".into()), "Text"),
            (
                MemFuseError::MemoryBudgetExceeded {
                    used_mb: 200,
                    limit_mb: 100,
                },
                "MemoryBudgetExceeded",
            ),
            (MemFuseError::Sandbox("test".into()), "Sandbox"),
            (
                MemFuseError::MemoryLimitExceeded("test".into()),
                "MemoryLimitExceeded",
            ),
            (
                MemFuseError::SandboxTimeout("test".into()),
                "SandboxTimeout",
            ),
            (MemFuseError::Serialization("test".into()), "Serialization"),
            (
                MemFuseError::Json(serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err()),
                "Json",
            ),
            (MemFuseError::Crypto("test".into()), "Crypto"),
            (MemFuseError::CheckpointNotFound, "CheckpointNotFound"),
            (MemFuseError::Cluster("test".into()), "Cluster"),
            (MemFuseError::ParseError("test".into()), "ParseError"),
            (
                MemFuseError::Bincode(Box::new(bincode::ErrorKind::Custom("err".into()))),
                "Bincode",
            ),
            (
                MemFuseError::CapabilityUnsupported {
                    capability: "cap".into(),
                    reason: "reason".into(),
                },
                "CapabilityUnsupported",
            ),
        ];

        for (err, expected_kind) in variants {
            let dto = MemFuseErrorDto::from(&err);
            assert_eq!(dto.kind, expected_kind);
            assert!(!dto.message.is_empty());
        }
    }

    #[test]
    fn test_dto_details_serialization() {
        let err = MemFuseError::WalCorruption {
            offset: 4096,
            reason: "corrupted block header".to_string(),
        };
        let dto = MemFuseErrorDto::from(&err);
        assert_eq!(dto.kind, "WalCorruption");
        let details = dto.details.as_ref().expect("details should exist"); // unwrap allowed
        assert_eq!(details["offset"], 4096);
        assert_eq!(details["reason"], "corrupted block header");

        let json_str = serde_json::to_string(&dto).expect("serde serialize"); // unwrap allowed
        let deser_dto: MemFuseErrorDto =
            serde_json::from_str(&json_str).expect("serde deserialize"); // unwrap allowed
        assert_eq!(dto, deser_dto);
    }
}
