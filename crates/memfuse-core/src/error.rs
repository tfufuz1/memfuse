//! Error types for MemFuse.

// ANCHOR:ARCH:ERR-001 — Einzige Error-Enum für den gesamten Workspace.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// Neue Varianten nur ANHÄNGEN (niemals umsortieren) → binäre Kompatibilität.
// DOWNSTREAM: memfuse-store, memfuse-index, memfuse-db konvertieren via `?` und `From`.

// ANCHOR:DOC STATUS:DONE AGENT:01 PRIO:3 — Missing variant documentation.
// ANCHOR:DEBT STATUS:DONE AGENT:01 PRIO:3 — Missing specific variants for SSTable and Bincode.

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
    WalCorruption {
        /// Offset in the WAL file where corruption was detected.
        offset: u64,
        /// Reason for the corruption.
        reason: String,
    },

    /// Checksum mismatch for a block or file.
    #[error("Checksum mismatch: file={path}, block={block_id}")]
    ChecksumMismatch {
        /// Path to the file.
        path: String,
        /// Block identifier.
        block_id: u64,
    },

    // ═══ Index ═══
    /// Generic index error.
    #[error("Index error: {0}")]
    Index(String),

    /// HNSW graph connectivity degraded beyond acceptable threshold.
    #[error("HNSW graph connectivity degraded: {deleted_ratio:.1}% deleted nodes")]
    HnswConnectivityDegraded {
        /// Ratio of deleted nodes.
        deleted_ratio: f64,
    },

    // ═══ Transaction ═══
    /// Generic transaction error.
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Transaction timeout.
    #[error("Transaction {tx_id} timed out after {elapsed_ms}ms")]
    TransactionTimeout {
        /// Transaction ID.
        tx_id: u64,
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
    },

    // ═══ Resource ═══
    /// Resource budget exceeded (e.g., memory).
    #[error("Memory budget exceeded: {used_mb}MB / {limit_mb}MB")]
    MemoryBudgetExceeded {
        /// Currently used memory in MB.
        used_mb: u64,
        /// Configured memory limit in MB.
        limit_mb: u64,
    },

    // ═══ Input ═══
    /// Invalid input provided by the user.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Requested resource or entity not found.
    #[error("Not found: {0}")]
    NotFound(String),

    // ═══ Serialization ═══
    /// Generic serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ═══ I/O ═══
    /// Standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ═══ Checkpointing (SAOS) ═══
    /// Checkpoint was not found.
    #[error("Checkpoint not found")]
    CheckpointNotFound,

    /// Invalid sequence number provided.
    #[error("Invalid sequence number: {0}")]
    InvalidSequenceNumber(u64),

    // ═══ Internal ═══
    /// Unrecoverable internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// SSTable data or index is corrupted.
    #[error("SSTable corruption at {path}: {reason}")]
    SstableCorruption {
        /// Path to the corrupted file.
        path: String,
        /// Reason for corruption.
        reason: String,
    },

    /// Bincode serialization or deserialization error.
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
