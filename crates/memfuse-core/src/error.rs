// ANCHOR:ARCH:ERR-001 — Einzige Error-Enum für den gesamten Workspace.
// Neue Varianten nur ANHÄNGEN (niemals umsortieren) → binäre Kompatibilität.
// Jede neue Variante braucht strukturierte Felder für Observability (kein reines String).
// DOWNSTREAM: memfuse-store, memfuse-index, memfuse-db konvertieren via `?` und `From`.
//! Error types for MemFuse.

use thiserror::Error;

/// Convenience alias for `Result<T, MemFuseError>`.
pub type Result<T> = std::result::Result<T, MemFuseError>;

/// Unified error type for all MemFuse operations.
#[derive(Error, Debug)]
pub enum MemFuseError {
    // ═══ Storage ═══
    /// Error during storage operations (LSM, SSTable).
    #[error("Storage error: {0}")]
    Storage(String),

    /// WAL corruption detected during replay or read.
    #[error("WAL corruption detected at offset {offset}: {reason}")]
    WalCorruption {
        /// File offset where corruption was detected.
        offset: u64,
        /// Reason/details of the corruption.
        reason: String,
    },

    /// Checksum verification failed.
    #[error("Checksum mismatch: file={path}, block={block_id}")]
    ChecksumMismatch {
        /// Path to the file.
        path: String,
        /// ID of the corrupted block.
        block_id: u64,
    },

    // ═══ Index ═══
    /// Error during vector index operations (HNSW).
    #[error("Index error: {0}")]
    Index(String),

    /// HNSW connectivity has dropped below threshold.
    #[error("HNSW graph connectivity degraded: {deleted_ratio:.1}% deleted nodes")]
    HnswConnectivityDegraded {
        /// Ratio of deleted nodes in the index.
        deleted_ratio: f64,
    },

    // ═══ Transaction ═══
    /// Error related to transaction management.
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Transaction has exceeded its configured timeout.
    #[error("Transaction {tx_id} timed out after {elapsed_ms}ms")]
    TransactionTimeout {
        /// ID of the timed-out transaction.
        tx_id: u64,
        /// Time elapsed since transaction start.
        elapsed_ms: u64,
    },

    // ═══ Resource ═══
    /// Memory usage has exceeded the configured budget.
    #[error("Memory budget exceeded: {used_mb}MB / {limit_mb}MB")]
    MemoryBudgetExceeded {
        /// Current memory usage in megabytes.
        used_mb: u64,
        /// Configured limit in megabytes.
        limit_mb: u64,
    },

    // ═══ Input ═══
    /// Provided input (e.g. vector dimensions) is invalid.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Requested item was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    // ═══ Serialization ═══
    /// Error during Bincode or Serde serialization.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Error during JSON parsing or serialization.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ═══ I/O ═══
    /// Underlying filesystem IO error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ═══ Checkpointing (SAOS) ═══
    /// Requested state checkpoint does not exist.
    #[error("Checkpoint not found")]
    CheckpointNotFound,

    /// Provided sequence number is out of valid bounds.
    #[error("Invalid sequence number: {0}")]
    InvalidSequenceNumber(u64),

    // ═══ Internal ═══
    /// Unrecoverable internal consistency error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl MemFuseError {
    /// Creates an `InvalidInput` error from any displayable value.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }
}
