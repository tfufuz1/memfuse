// FILE-CONTEXT
// ZWECK: KV-Cache-Bridge Sicherheitsschicht (KvSegment, Tenant-Isolation, Eviction-Worker, Segment-Verschlüsselung).
// STAND: TS:2026-09-09T12:43:43Z (SESSION: 76e16dcf)

//! # KV-Cache-Bridge Sicherheitsschicht
//!
//! Implementiert die Sicherheitsinfrastruktur für den KV-Cache gemäß Gesamtspezifikation v7.0:
//! - **P9 (Kein Klartext-Sensitivspeicher)**: `ZeroizeOnDrop` für alle Tensordaten.
//! - **K14 Increment 2 (Abgeschlossen)**: Optional feature-gated Segment-Verschlüsselung (`#[cfg(feature = "kv-encryption")]`)
//!   mittels `KvSegmentCipher` (AES-256-GCM-SIV, per-tenant + per-model_fingerprint Sub-Key-Derivation via HKDF-SHA256).
//! - **RoPE-Offset (`rope_offset: Option<usize>`)**: Strukturell in `KvSegment` verankert; wo der Aufrufer (`memfuse-mcp`)
//!   diesen noch nicht liefert, wird `None` übergeben und als offener Folgepunkt dokumentiert.

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod eviction_worker;
pub mod segment;
pub mod store;

pub use eviction_worker::{emergency_wipe, EvictionWorker};
pub use segment::KvSegment;
pub use store::TenantIsolatedKvStore;
