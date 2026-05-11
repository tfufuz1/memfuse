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
    // ═══ Storage ═══
    /// Generic storage error.
    #[error("Storage error: {0}")]
    Storage(String),

    /// WAL corruption detected.
    #[error("WAL corruption detected at offset {offset}: {reason}")]
    WalCorruption { offset: u64, reason: String },

    /// Checksum mismatch in a file block.
    #[error("Checksum mismatch: file={path}, block={block_id}")]
    ChecksumMismatch { path: String, block_id: u64 },

    // ═══ Index ═══
    /// Generic index error.
    #[error("Index error: {0}")]
    Index(String),

    /// HNSW graph connectivity is too low.
    #[error("HNSW graph connectivity degraded: {deleted_ratio:.1}% deleted nodes")]
    HnswConnectivityDegraded { deleted_ratio: f64 },

    // ═══ Transaction ═══
    /// Generic transaction error.
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Transaction exceeded its timeout.
    #[error("Transaction {tx_id} timed out after {elapsed_ms}ms")]
    TransactionTimeout { tx_id: u64, elapsed_ms: u64 },

    // ═══ Resource ═══
    /// Memory usage exceeded the configured budget.
    #[error("Memory budget exceeded: {used_mb}MB / {limit_mb}MB")]
    MemoryBudgetExceeded { used_mb: u64, limit_mb: u64 },

    // ═══ Input ═══
    /// Provided input is invalid.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    // ═══ Serialization ═══
    /// Generic serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Error during JSON processing.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ═══ I/O ═══
    /// Standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ═══ Checkpointing (SAOS) ═══
    /// Requested checkpoint does not exist.
    #[error("Checkpoint not found")]
    CheckpointNotFound,

    /// Sequence number is out of valid range.
    #[error("Invalid sequence number: {0}")]
    InvalidSequenceNumber(u64),

    // ═══ Internal ═══
    /// Unrecoverable internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// SSTable corruption detected.
    #[error("SSTable corruption: {path}: {reason}")]
    SstableCorruption { path: String, reason: String },

    /// Error during Bincode processing.
    #[error("Bincode error: {0}")]
    Bincode(String),
}

impl MemFuseError {
    /// Creates an `InvalidInput` error from any displayable value.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Creates an `Internal` error from any displayable value.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
