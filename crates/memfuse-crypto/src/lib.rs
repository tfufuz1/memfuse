//! # MemFuse Crypto
//!
//! This crate provides cryptographic primitives and utilities for MemFuse,
//! including encryption at rest for the storage engine and HMAC-based
//! integrity verification for the Write-Ahead Log (WAL).

pub mod crypto;
pub mod wal_crypto;
