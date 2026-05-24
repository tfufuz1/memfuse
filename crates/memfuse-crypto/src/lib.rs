//! # MemFuse Crypto
//!
//! Provides cryptographic primitives and utilities for MemFuse, including
//! encryption at rest for the storage engine and cryptographic verification
//! of the Write-Ahead Log (WAL).
// ANCHOR:DOC:CRYPTO-LIB-001 — Module documentation added
// AGENT:08 STATUS:DONE

pub mod crypto;
pub mod wal_crypto;
