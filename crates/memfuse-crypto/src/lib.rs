#![forbid(unsafe_code)]
//! ANCHOR:DEBT:CRYPTO-HARDENING (AGENT:13 STATUS:DONE DATE:2026-06-01)
//! Cryptographic Utilities for MemFuse.
//!
//! Provides AES-256-GCM encryption for WAL and SSTables,
//! and HMAC-based integrity chains for the Write-Ahead Log.

pub mod crypto;
pub mod wal_crypto;
