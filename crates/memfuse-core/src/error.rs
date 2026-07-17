//! Error types for MemFuse.

// INVARIANT: Einzige Error-Enum für den gesamten Workspace.
// Neue Varianten nur ANHÄNGEN (niemals umsortieren) → binäre Kompatibilität.
// DOWNSTREAM: memfuse-store, memfuse-index, memfuse-db konvertieren via `?` und `From`.

use thiserror::Error;

/// Convenience alias for `Result<T, MemFuseError>`.
pub type Result<T> = std::result::Result<T, MemFuseError>;

/// Unified error type for all MemFuse operations.
#[derive(Error, Debug)]
pub enum MemFuseError {
    // ═══ Core & Logic ═══
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    // ═══ Storage Engine ═══
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WAL corruption detected at offset {offset}: {reason}")]
    WalCorruption { offset: u64, reason: String },

    #[error("Checksum mismatch: file={path}, block={block_id}")]
    ChecksumMismatch { path: String, block_id: u64 },

    // ═══ Transactions & Consistency ═══
    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Transaction {tx_id} timed out after {elapsed_ms}ms")]
    TransactionTimeout { tx_id: u64, elapsed_ms: u64 },

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid sequence number: {0}")]
    InvalidSequenceNumber(u64),

    // ═══ Index & Search ═══
    #[error("Index error: {0}")]
    Index(String),

    #[error("HNSW graph connectivity degraded: {deleted_ratio:.1}% deleted nodes")]
    HnswConnectivityDegraded { deleted_ratio: f64 },

    #[error("Text engine error: {0}")]
    Text(String),

    // ═══ Resources & Sandbox ═══
    #[error("Memory budget exceeded: {used_mb}MB / {limit_mb}MB")]
    MemoryBudgetExceeded { used_mb: u64, limit_mb: u64 },

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Memory limit exceeded in sandbox: {0}")]
    MemoryLimitExceeded(String),

    #[error("Timeout exceeded in sandbox: {0}")]
    SandboxTimeout(String),

    // ═══ Infrastructure ═══
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Checkpoint not found")]
    CheckpointNotFound,

    #[error("Cluster error: {0}")]
    Cluster(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl MemFuseError {
    /// Creates an `InvalidInput` error from any displayable value.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
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
    fn test_invalid_input_helper() {
        let err = MemFuseError::invalid_input("bad param");
        match err {
            MemFuseError::InvalidInput(msg) => assert_eq!(msg, "bad param"),
            _ => panic!("Expected InvalidInput, got {:?}", err),
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
            MemFuseError::HnswConnectivityDegraded {
                deleted_ratio: 0.25
            }
            .to_string(),
            "HNSW graph connectivity degraded: 0.2% deleted nodes"
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
    }

    #[test]
    fn test_from_conversions() {
        // Io
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let m_err: MemFuseError = io_err.into();
        assert!(matches!(m_err, MemFuseError::Io(_)));

        // Json
        let json_str = "{ invalid }";
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let m_err2: MemFuseError = json_err.into();
        assert!(matches!(m_err2, MemFuseError::Json(_)));
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
}
