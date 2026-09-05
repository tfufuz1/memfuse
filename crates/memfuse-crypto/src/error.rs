//! Error types for `memfuse-crypto`.

use thiserror::Error;

/// Result type alias for cryptographic operations in `memfuse-crypto`.
pub type Result<T> = std::result::Result<T, CryptoError>;

/// Standalone error type for all operations within `memfuse-crypto`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CryptoError {
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {0}")]
    Decryption(String),

    #[error("invalid nonce or key length: {0}")]
    InvalidLength(String),

    #[error("integrity check failed: authentication tag mismatch")]
    IntegrityViolation,

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("WAL corruption detected at offset {offset}: {reason}")]
    WalCorruption {
        /// Byte offset in WAL file where corruption occurred.
        offset: u64,
        /// Detail text describing corruption cause.
        reason: String,
    },
}

impl CryptoError {
    /// Helper to construct a `WalCorruption` error.
    pub fn wal_corruption(offset: u64, reason: impl Into<String>) -> Self {
        Self::WalCorruption {
            offset,
            reason: reason.into(),
        }
    }
}
