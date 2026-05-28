//! Cryptographic primitives for MemFuse.
//!
//! Provides encryption and hashing utilities for protecting data at rest,
//! including WAL (Write-Ahead-Log) protection.
//!
// ANCHOR:DOC AGENT:08 STATUS:DONE

pub mod crypto;
pub mod wal_crypto;
