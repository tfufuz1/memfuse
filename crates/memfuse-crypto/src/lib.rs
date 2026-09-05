// FILE-CONTEXT
// ZWECK: MemFuse cryptographic kernel - Layer 1 crate for AEAD block encryption, HKDF key derivation, and WAL integrity.
// INVARIANTEN: Cryptographic primitives strictly isolated to this crate. All key operations zeroized on drop. Lock-free & I/O-free in src/.
// NICHT-OFFENSICHTLICH: KeyManager uses AES-256-GCM-SIV with OsRng 8-byte random suffix + 4-byte prefix to prevent nonce-reuse key leakage.
// HOTSPOTS: [1-25]
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

#![cfg_attr(not(test), forbid(unsafe_code))]

//! Cryptography module for MemFuse.
//!
//! # Architektur
//! Bietet Verschlüsselung und Integritätsschutz für WAL und SSTables.
//! Nutzt AES-256-GCM-SIV für Datenverschlüsselung und HMAC-SHA256 für Integrität.
//!
//! # Invarianten
//! - Pro Datei wird ein eindeutiger Schlüssel abgeleitet (Nonce-Reuse Mitigation).
//! - Passwörter werden via HKDF in Keys expandiert.
//! - Absolut lock-frei und frei von synchronen/asynchronen I/O-Operationen.

pub mod anti_tamper;
pub mod crypto;
pub mod error;
pub mod wal_crypto;

pub use crypto::KeyManager as CryptoKey;
pub use error::{CryptoError, Result};
