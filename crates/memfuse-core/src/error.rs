//! Error types for MemFuse.

// ANCHOR:ARCH:ERR-001 — Einzige Error-Enum für den gesamten Workspace.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
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
    fn test_error_display() {
        let err = MemFuseError::NotFound("doc_1".to_string());
        assert_eq!(err.to_string(), "Not found: doc_1");

        let io_err = MemFuseError::Io(std::io::Error::other("disk failure"));
        assert!(io_err.to_string().contains("I/O error: disk failure"));

        let wal_err = MemFuseError::WalCorruption {
            offset: 1024,
            reason: "invalid header".to_string(),
        };
        assert_eq!(wal_err.to_string(), "WAL corruption detected at offset 1024: invalid header");
    }

    #[test]
    fn test_from_conversions() {
        // Io
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let m_err: MemFuseError = io_err.into();
        assert!(matches!(m_err, MemFuseError::Io(_)));

        // Json
        let json_str = "{ invalid }";
        let json_err: serde_json::Error = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
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
}
