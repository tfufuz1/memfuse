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

impl From<CryptoError> for memfuse_core::MemFuseError {
    fn from(e: CryptoError) -> Self {
        match e {
            CryptoError::WalCorruption { offset, reason } => Self::wal_corruption(offset, reason),
            CryptoError::InvalidInput(msg) => Self::InvalidInput(msg),
            other => Self::Crypto(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::MemFuseError;

    #[test]
    fn test_from_crypto_error() {
        let crypto_err = CryptoError::WalCorruption {
            offset: 42,
            reason: "bad hmac".to_string(),
        };
        let mf_err: MemFuseError = crypto_err.into();
        assert!(matches!(
            mf_err,
            MemFuseError::WalCorruption {
                offset: 42,
                ref reason,
                ..
            } if reason == "bad hmac"
        ));

        let invalid_input = CryptoError::InvalidInput("bad key".to_string());
        let mf_err_inv: MemFuseError = invalid_input.into();
        assert!(matches!(
            mf_err_inv,
            MemFuseError::InvalidInput(ref msg) if msg == "bad key"
        ));

        let gen_crypto = CryptoError::Encryption("failed".to_string());
        let mf_err_gen: MemFuseError = gen_crypto.into();
        assert!(matches!(
            mf_err_gen,
            MemFuseError::Crypto(ref msg) if msg.contains("encryption failed")
        ));
    }
}
