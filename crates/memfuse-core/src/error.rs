//! Error types for `MemFuse`.

// FILE-CONTEXT
// STAND: 2026-08-30T21:51:46Z (SESSION: a43b7682)
// ZWECK: Kanonische unified MemFuseError Enum für den gesamten Workspace.
// INVARIANTEN: Zero-Panic via ? propagation; #[non_exhaustive] für binäre Abwärtskompatibilität. KEINE neue Error-Enum in anderen Crates anlegen.
// HOTSPOTS: 20-180
// NICHT-OFFENSICHTLICH: Neue Fehler-Varianten NUR unten anhängen; Downstream-Crates brauchen Wildcard-Match arm.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md

// INVARIANT: Einzige Error-Enum für den gesamten Workspace.
// Neue Varianten nur ANHÄNGEN (niemals umsortieren) → binäre Kompatibilität.
// DOWNSTREAM: memfuse-store, memfuse-index, memfuse-db konvertieren via `?` und `From`.

use thiserror::Error;

/// Convenience alias for `Result<T, MemFuseError>`.
pub type Result<T> = std::result::Result<T, MemFuseError>;

/// Unified error type for all `MemFuse` operations across the entire workspace.
///
/// # Non-Exhaustive Variant Guarantee
/// This enum is marked `#[non_exhaustive]` to allow appending new error variants
/// in future minor releases without breaking downstream `match` statements across crate boundaries
/// (such as `memfuse-py` and `memfuse-mcp`).
///
/// Downstream callers matching on `MemFuseError` must include a wildcard arm (`_ => ...`).
/// New variants are appended strictly to the bottom of the enum to preserve binary and FFI compatibility.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MemFuseError {
    // ═══ Core & Logic ═══
    /// Internal engine logic error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Invalid user or API argument input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Resource, key, or document not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Security or execution policy violation.
    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    // ═══ Storage Engine ═══
    /// Storage layer failure.
    #[error("Storage error: {0}")]
    Storage(String),

    /// Standard I/O error wrapper.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Data integrity corruption detected in Write-Ahead Log.
    #[error("WAL corruption detected at offset {offset}: {reason}")]
    #[non_exhaustive]
    WalCorruption {
        /// Byte offset in WAL file where corruption occurred.
        offset: u64,
        /// Detail text describing corruption cause.
        reason: String,
    },

    /// Block checksum validation failure.
    #[error("Checksum mismatch: file={path}, block={block_id}")]
    #[non_exhaustive]
    ChecksumMismatch {
        /// File path of corrupted storage block.
        path: String,
        /// Block identifier.
        block_id: u64,
    },

    // ═══ Transactions & Consistency ═══
    /// Transaction lifecycle or execution failure.
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Transaction execution timeout exceeded.
    #[error("Transaction {tx_id} timed out after {elapsed_ms}ms")]
    TransactionTimeout {
        /// Identifier of timed out transaction.
        tx_id: u64,
        /// Elapsed time in milliseconds before timeout.
        elapsed_ms: u64,
    },

    /// Data conflict during commit or mutation.
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Out-of-order or invalid sequence number.
    #[error("Invalid sequence number: {0}")]
    InvalidSequenceNumber(u64),

    // ═══ Index & Search ═══
    /// General index operation failure.
    #[error("Index error: {0}")]
    Index(String),

    /// Vector or embedding dimension mismatch.
    #[error("Embedding dimension mismatch: expected {expected}, got {got}")]
    EmbeddingDimensionMismatch {
        /// Expected vector dimension.
        expected: usize,
        /// Actual vector dimension.
        got: usize,
    },

    /// HNSW graph degradation warning threshold reached.
    #[error("HNSW graph connectivity degraded: {deleted_ratio:.1}% deleted nodes")]
    HnswConnectivityDegraded {
        /// Ratio of deleted tombstone nodes in graph.
        deleted_ratio: f64,
    },

    /// Text search engine operation failure.
    #[error("Text engine error: {0}")]
    Text(String),

    // ═══ Resources & Sandbox ═══
    /// Configured memory allocation budget exceeded.
    #[error("Memory budget exceeded: {used_mb}MB / {limit_mb}MB")]
    MemoryBudgetExceeded {
        /// Currently used memory in MB.
        used_mb: u64,
        /// Configured memory limit in MB.
        limit_mb: u64,
    },

    /// Code or query sandbox execution error.
    #[error("Sandbox error: {0}")]
    Sandbox(String),

    /// Sandbox memory limit breach.
    #[error("Memory limit exceeded in sandbox: {0}")]
    MemoryLimitExceeded(String),

    /// Sandbox execution timeout breach.
    #[error("Timeout exceeded in sandbox: {0}")]
    SandboxTimeout(String),

    /// General operation execution timeout breach.
    #[error("Operation timed out: {operation} (limit: {timeout_ms}ms)")]
    Timeout {
        /// Identifier of the timed out operation.
        operation: String,
        /// Configured timeout limit in milliseconds.
        timeout_ms: u64,
    },

    // ═══ Infrastructure ═══
    /// Data serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// JSON processing error wrapper.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Cryptographic operation error.
    #[error("Crypto error: {0}")]
    Crypto(String),

    /// Requested checkpoint missing or deleted.
    #[error("Checkpoint not found")]
    CheckpointNotFound,

    /// Distributed cluster operation error.
    #[error("Cluster error: {0}")]
    Cluster(String),

    /// Data parsing error.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Bincode serialization or deserialization error wrapper.
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    /// Capability requested is not supported by this engine or implementation.
    #[error("Capability unsupported: {capability} - {reason}")]
    CapabilityUnsupported {
        /// Unique capability identifier.
        capability: String,
        /// Detail text describing why capability is unsupported.
        reason: String,
    },

    /// Optimistic concurrency control stale read or version conflict.
    #[error("Stale read / OCC conflict: {0}")]
    StaleRead(String),
}

impl MemFuseError {
    /// Creates a `CapabilityUnsupported` error.
    pub fn capability_unsupported(
        capability: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::CapabilityUnsupported {
            capability: capability.into(),
            reason: reason.into(),
        }
    }
    /// Creates an `InvalidInput` error from any displayable value.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Creates a `WalCorruption` error.
    pub fn wal_corruption(offset: u64, reason: impl Into<String>) -> Self {
        Self::WalCorruption {
            offset,
            reason: reason.into(),
        }
    }

    /// Creates a `ChecksumMismatch` error.
    pub fn checksum_mismatch(path: impl Into<String>, block_id: u64) -> Self {
        Self::ChecksumMismatch {
            path: path.into(),
            block_id,
        }
    }
}

impl From<std::array::TryFromSliceError> for MemFuseError {
    fn from(e: std::array::TryFromSliceError) -> Self {
        Self::ParseError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memfuse_error_display_no_panic() {
        let variants = [
            MemFuseError::Internal("test".into()),
            MemFuseError::InvalidInput("test".into()),
            MemFuseError::NotFound("test".into()),
            MemFuseError::Storage("test".into()),
            MemFuseError::PolicyViolation("test".into()),
            MemFuseError::WalCorruption {
                offset: 10,
                reason: "test".into(),
            },
            MemFuseError::ChecksumMismatch {
                path: "file".into(),
                block_id: 1,
            },
            MemFuseError::Transaction("test".into()),
            MemFuseError::TransactionTimeout {
                tx_id: 1,
                elapsed_ms: 100,
            },
            MemFuseError::Conflict("test".into()),
            MemFuseError::InvalidSequenceNumber(1),
            MemFuseError::Index("test".into()),
            MemFuseError::EmbeddingDimensionMismatch {
                expected: 1536,
                got: 768,
            },
            MemFuseError::HnswConnectivityDegraded { deleted_ratio: 0.1 },
            MemFuseError::Text("test".into()),
            MemFuseError::MemoryBudgetExceeded {
                used_mb: 10,
                limit_mb: 5,
            },
            MemFuseError::Sandbox("test".into()),
            MemFuseError::MemoryLimitExceeded("test".into()),
            MemFuseError::SandboxTimeout("test".into()),
            MemFuseError::Serialization("test".into()),
            MemFuseError::Crypto("test".into()),
            MemFuseError::CheckpointNotFound,
            MemFuseError::Cluster("test".into()),
            MemFuseError::ParseError("test".into()),
        ];
        for v in &variants {
            let _ = format!("{v}");
            let _ = format!("{v:?}");
        }
    }

    #[test]
    fn test_invalid_input_helper() {
        let err = MemFuseError::invalid_input("bad param");
        match err {
            MemFuseError::InvalidInput(msg) => assert_eq!(msg, "bad param"),
            _ => panic!("Expected InvalidInput, got {:?}", err),
        }
    }

    #[test]
    fn test_capability_unsupported_helper() {
        let err = MemFuseError::capability_unsupported("snapshot_read_at", "ADR-024");
        assert_eq!(
            err.to_string(),
            "Capability unsupported: snapshot_read_at - ADR-024"
        );
        match err {
            MemFuseError::CapabilityUnsupported { capability, reason } => {
                assert_eq!(capability, "snapshot_read_at");
                assert_eq!(reason, "ADR-024");
            }
            _ => panic!("Expected CapabilityUnsupported, got {:?}", err),
        }
    }

    #[test]
    fn test_from_try_from_slice_error() {
        let slice: &[u8] = &[1, 2, 3];
        let try_from_res: std::result::Result<[u8; 4], _> = slice.try_into();
        assert!(try_from_res.is_err());

        let parse_err: MemFuseError = try_from_res.unwrap_err().into();
        match parse_err {
            MemFuseError::ParseError(msg) => {
                assert!(msg.contains("could not convert slice to array") || msg.contains("slice"));
            }
            _ => panic!("Expected ParseError, got {:?}", parse_err),
        }
    }

    #[test]
    fn test_error_display_all_variants() {
        // Core & Logic
        assert_eq!(
            MemFuseError::Internal("test".into()).to_string(),
            "Internal error: test"
        );
        assert_eq!(
            MemFuseError::InvalidInput("test".into()).to_string(),
            "Invalid input: test"
        );
        assert_eq!(
            MemFuseError::NotFound("doc_1".into()).to_string(),
            "Not found: doc_1"
        );
        assert_eq!(
            MemFuseError::PolicyViolation("test".into()).to_string(),
            "Policy violation: test"
        );

        // Storage Engine
        assert_eq!(
            MemFuseError::Storage("test".into()).to_string(),
            "Storage error: test"
        );
        assert_eq!(
            MemFuseError::WalCorruption {
                offset: 1024,
                reason: "invalid header".into()
            }
            .to_string(),
            "WAL corruption detected at offset 1024: invalid header"
        );
        assert_eq!(
            MemFuseError::ChecksumMismatch {
                path: "f".into(),
                block_id: 1
            }
            .to_string(),
            "Checksum mismatch: file=f, block=1"
        );

        // Transactions & Consistency
        assert_eq!(
            MemFuseError::Transaction("test".into()).to_string(),
            "Transaction error: test"
        );
        assert_eq!(
            MemFuseError::TransactionTimeout {
                tx_id: 1,
                elapsed_ms: 50
            }
            .to_string(),
            "Transaction 1 timed out after 50ms"
        );
        assert_eq!(
            MemFuseError::Conflict("test".into()).to_string(),
            "Conflict: test"
        );
        assert_eq!(
            MemFuseError::InvalidSequenceNumber(42).to_string(),
            "Invalid sequence number: 42"
        );

        // Index & Search
        assert_eq!(
            MemFuseError::Index("test".into()).to_string(),
            "Index error: test"
        );
        assert_eq!(
            MemFuseError::EmbeddingDimensionMismatch {
                expected: 1536,
                got: 768
            }
            .to_string(),
            "Embedding dimension mismatch: expected 1536, got 768"
        );
        assert_eq!(
            MemFuseError::HnswConnectivityDegraded {
                deleted_ratio: 25.0
            }
            .to_string(),
            "HNSW graph connectivity degraded: 25.0% deleted nodes"
        );
        assert_eq!(
            MemFuseError::Text("test".into()).to_string(),
            "Text engine error: test"
        );

        // Resources & Sandbox
        assert_eq!(
            MemFuseError::MemoryBudgetExceeded {
                used_mb: 100,
                limit_mb: 50
            }
            .to_string(),
            "Memory budget exceeded: 100MB / 50MB"
        );
        assert_eq!(
            MemFuseError::Sandbox("test".into()).to_string(),
            "Sandbox error: test"
        );
        assert_eq!(
            MemFuseError::MemoryLimitExceeded("test".into()).to_string(),
            "Memory limit exceeded in sandbox: test"
        );
        assert_eq!(
            MemFuseError::SandboxTimeout("test".into()).to_string(),
            "Timeout exceeded in sandbox: test"
        );

        // Infrastructure
        assert_eq!(
            MemFuseError::Serialization("test".into()).to_string(),
            "Serialization error: test"
        );
        assert_eq!(
            MemFuseError::Crypto("test".into()).to_string(),
            "Crypto error: test"
        );
        assert_eq!(
            MemFuseError::CheckpointNotFound.to_string(),
            "Checkpoint not found"
        );
        assert_eq!(
            MemFuseError::Cluster("test".into()).to_string(),
            "Cluster error: test"
        );
        assert_eq!(
            MemFuseError::ParseError("test".into()).to_string(),
            "Parse error: test"
        );
        let bincode_err: bincode::Error = Box::new(bincode::ErrorKind::Custom("test".into()));
        assert_eq!(
            MemFuseError::Bincode(bincode_err).to_string(),
            "Bincode error: test"
        );
    }

    #[test]
    fn test_from_conversions() {
        use std::error::Error;

        // Io
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let m_err: MemFuseError = io_err.into();
        assert!(matches!(m_err, MemFuseError::Io(_)));
        assert!(m_err.source().is_some());

        // Json
        let json_str = "{ invalid }";
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let m_err2: MemFuseError = json_err.into();
        assert!(matches!(m_err2, MemFuseError::Json(_)));
        assert!(m_err2.source().is_some());

        // Bincode
        let bincode_err: bincode::Error =
            Box::new(bincode::ErrorKind::Custom("test bincode".into()));
        let m_err_bc: MemFuseError = bincode_err.into();
        assert!(matches!(m_err_bc, MemFuseError::Bincode(_)));
        assert!(m_err_bc.source().is_some());

        // Other variants should return None for source()
        let m_err3 = MemFuseError::Internal("test".into());
        assert!(m_err3.source().is_none());
    }

    #[test]
    fn test_result_alias() {
        fn fail() -> Result<()> {
            Err(MemFuseError::Internal("failed".to_string()))
        }
        let res = fail();
        assert!(res.is_err());
    }

    /// Mutation-robustness test: verifies that `From<std::io::Error>` preserves the
    /// original I/O error message in the Display output.
    ///
    /// # Anti-Mirroring
    /// Expected string `"I/O error: "` is a hand-written prefix, independent of the format
    /// impl string `"I/O error: {0}"`. If the format string changed (e.g. prefix dropped),
    /// this test would catch it.
    ///
    /// # Mutation robustness
    /// Removing the `#[from]` attribute would break this test (Io variant would stop matching).
    /// Changing the prefix from "I/O error" to anything else would break the `starts_with` assert.
    #[test]
    fn test_io_error_message_preserved() {
        let sentinel = "unique_sentinel_message_for_mutation_test_42";
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, sentinel);
        let mf_err: MemFuseError = io_err.into();

        // Must be the Io variant — not Internal or Storage
        assert!(
            matches!(mf_err, MemFuseError::Io(_)),
            "Expected MemFuseError::Io, got: {:?}",
            mf_err
        );

        // The original message must survive in the Display output (no message loss)
        let display = mf_err.to_string();
        assert!(
            display.contains(sentinel),
            "io::Error message must be preserved. Display was: {:?}",
            display
        );
        // The MemFuseError prefix must also be present (structural check)
        assert!(
            display.starts_with("I/O error:"),
            "Expected 'I/O error:' prefix, got: {:?}",
            display
        );
    }

    /// Mutation-robustness test: verifies `From<serde_json::Error>` carries the parse error info.
    ///
    /// # Anti-Mirroring
    /// `"JSON error:"` is a hand-written known-stable string; not derived from the format macro.
    #[test]
    fn test_json_error_message_preserved() {
        let sentinel_json = "{\"key\": }"; // Valid JSON prefix, invalid tail
        let json_err = serde_json::from_str::<serde_json::Value>(sentinel_json).unwrap_err();
        let original_msg = json_err.to_string();
        let mf_err: MemFuseError = json_err.into();

        assert!(matches!(mf_err, MemFuseError::Json(_)));

        let display = mf_err.to_string();
        // Message from the JSON parser must survive in the output
        assert!(
            display.contains(&original_msg) || display.contains("expected value"),
            "Json error detail must be preserved. Display: {:?}, original: {:?}",
            display,
            original_msg
        );
        assert!(
            display.starts_with("JSON error:"),
            "Expected 'JSON error:' prefix, got: {:?}",
            display
        );
    }

    /// Mutation-robustness test: WalCorruption fields must not be transposed.
    ///
    /// # Invariant
    /// Swapping `offset` and `reason` in the struct definition would break this test.
    /// Using the same value for both fields (lazy test) would not — this test uses distinct
    /// types to make field-swapping impossible, and distinct values to catch display-level bugs.
    #[test]
    fn test_wal_corruption_fields_not_transposed() {
        let err = MemFuseError::WalCorruption {
            offset: 98765,
            reason: "corrupted hmac chain".to_string(),
        };
        let display = err.to_string();

        // The numeric offset must appear in the display — not be silently replaced by the reason
        assert!(
            display.contains("98765"),
            "WalCorruption offset must appear in display: {:?}",
            display
        );
        // The reason text must also appear
        assert!(
            display.contains("corrupted hmac chain"),
            "WalCorruption reason must appear in display: {:?}",
            display
        );
        // The offset must NOT accidentally appear where the reason should be
        // (catches field-transposition bug in format string)
        assert!(
            !display.starts_with("WAL corruption detected at offset corrupted"),
            "Offset and reason must not be transposed: {:?}",
            display
        );
    }

    /// Mutation-robustness test: ChecksumMismatch fields must not be transposed.
    #[test]
    fn test_checksum_mismatch_fields_not_transposed() {
        let err = MemFuseError::ChecksumMismatch {
            path: "data_segment_003.sst".to_string(),
            block_id: 887766,
        };
        let display = err.to_string();

        // Check path and block_id are present
        assert!(
            display.contains("data_segment_003.sst"),
            "ChecksumMismatch path must appear in display: {:?}",
            display
        );
        assert!(
            display.contains("887766"),
            "ChecksumMismatch block_id must appear in display: {:?}",
            display
        );

        // Verify correct field positions (path shouldn't be formatted into block position)
        assert!(
            display.contains("file=data_segment_003.sst"),
            "Path formatted incorrectly: {:?}",
            display
        );
        assert!(
            display.contains("block=887766"),
            "Block ID formatted incorrectly: {:?}",
            display
        );
    }

    /// Mutation-robustness test: TransactionTimeout fields must not be transposed.
    #[test]
    fn test_transaction_timeout_fields_not_transposed() {
        let err = MemFuseError::TransactionTimeout {
            tx_id: 112233,
            elapsed_ms: 998877,
        };
        let display = err.to_string();

        assert!(
            display.contains("112233"),
            "TransactionTimeout tx_id must appear in display: {:?}",
            display
        );
        assert!(
            display.contains("998877"),
            "TransactionTimeout elapsed_ms must appear in display: {:?}",
            display
        );

        // Position checks
        assert!(
            display.starts_with("Transaction 112233 timed out"),
            "Transaction ID formatted in wrong place: {:?}",
            display
        );
        assert!(
            display.contains("after 998877ms"),
            "Elapsed ms formatted in wrong place: {:?}",
            display
        );
    }

    /// Mutation-robustness test: MemoryBudgetExceeded fields must not be transposed.
    #[test]
    fn test_memory_budget_exceeded_fields_not_transposed() {
        let err = MemFuseError::MemoryBudgetExceeded {
            used_mb: 4096,
            limit_mb: 8192,
        };
        let display = err.to_string();

        assert!(
            display.contains("4096"),
            "MemoryBudgetExceeded used_mb must appear in display: {:?}",
            display
        );
        assert!(
            display.contains("8192"),
            "MemoryBudgetExceeded limit_mb must appear in display: {:?}",
            display
        );

        // Position checks
        assert!(
            display.contains("4096MB / 8192MB"),
            "Used and limit MB fields are transposed or misformatted: {:?}",
            display
        );
    }

    /// Mutation-robustness test: asserts HnswConnectivityDegraded deleted_ratio is preserved as-is.
    ///
    /// Checks that the deleted_ratio is preserved without scaling (not divided/multiplied by 100)
    /// and that the formatting accurately reflects the ratio.
    #[test]
    fn test_hnsw_connectivity_degraded_preserves_ratio() {
        let sentinel_ratio = 37.42;
        let err = MemFuseError::HnswConnectivityDegraded {
            deleted_ratio: sentinel_ratio,
        };

        if let MemFuseError::HnswConnectivityDegraded { deleted_ratio } = err {
            assert!(
                (deleted_ratio - sentinel_ratio).abs() < f64::EPSILON,
                "deleted_ratio must be preserved as-is: expected {}, got {}",
                sentinel_ratio,
                deleted_ratio
            );
        } else {
            panic!("Expected MemFuseError::HnswConnectivityDegraded");
        }

        let display = err.to_string();
        assert!(
            display.contains("37.4%"),
            "Display formatting must contain '37.4%': got {:?}",
            display
        );
    }

    #[test]
    fn test_error_constructor_helpers() {
        let err_cap = MemFuseError::capability_unsupported("vector_search", "no_gpu");
        assert!(matches!(
            err_cap,
            MemFuseError::CapabilityUnsupported {
                ref capability,
                ref reason
            } if capability == "vector_search" && reason == "no_gpu"
        ));

        let err_inv = MemFuseError::invalid_input("key cannot be empty");
        assert!(
            matches!(err_inv, MemFuseError::InvalidInput(ref msg) if msg == "key cannot be empty")
        );

        let err_wal = MemFuseError::wal_corruption(1024, "bad crc");
        assert!(matches!(
            err_wal,
            MemFuseError::WalCorruption { offset: 1024, ref reason } if reason == "bad crc"
        ));

        let err_chk = MemFuseError::checksum_mismatch("/var/data.sst", 7);
        assert!(matches!(
            err_chk,
            MemFuseError::ChecksumMismatch { ref path, block_id: 7 } if path == "/var/data.sst"
        ));
    }

    #[test]
    fn test_dto_with_details_override() {
        use crate::error_dto::MemFuseErrorDto;
        let dto = MemFuseErrorDto::with_details(
            "CustomKind",
            "Custom message",
            serde_json::json!({"trace_id": "12345"}),
        );
        assert_eq!(dto.kind, "CustomKind");
        assert_eq!(dto.message, "Custom message");
        assert_eq!(dto.details.expect("details present")["trace_id"], "12345"); // expect
    }
}
