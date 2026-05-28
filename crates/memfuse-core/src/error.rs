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
}

impl MemFuseError {
    /// Creates an `InvalidInput` error from any displayable value.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }
}
