#![forbid(unsafe_code)]

//! Cryptography module for MemFuse.
//!
//! # Architektur
//! Bietet Verschlüsselung und Integritätsschutz für WAL und SSTables.
//! Nutzt AES-256-GCM für Datenverschlüsselung und HMAC-SHA256 für Integrität.
//!
//! # Invarianten
//! - Pro Datei wird ein eindeutiger Schlüssel abgeleitet (Nonce-Reuse Mitigation).
//! - Passwörter werden via HKDF in Keys expandiert.

pub mod crypto;
pub mod wal_crypto;
