//! MemFuse Core — Types, traits, and error handling.
//!
//! This crate provides the foundational building blocks for the MemFuse
//! embedded hybrid-search library.
//!
//! # Architecture Role (Triebwerk — Layer 0)
//!
//! This is the **dependency root** of the entire workspace. Every other crate
//! depends on `memfuse-core`. It provides:
//! - **Type IDs**: [`DocId`], [`EntityId`], [`TxId`] — all `#[repr(transparent)]` u64 newtypes
//! - **Traits**: [`StorageEngine`], [`VectorIndex`] — async interfaces for subsystems
//! - **Error**: [`MemFuseError`] — unified error enum, zero-panic via `?` propagation
//! - **TxBuffer**: Sharded transaction staging with orphan reaper
//! - **Snapshots**: MVCC read isolation via [`SnapshotRegistry`]

// ANCHOR:INTEGRATION STATUS:DONE AGENT:01
// ANCHOR:ARCH:GATE-FV STATUS:OPEN
// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs for REVIEW components (WAL/LSM/Encryption). Run: 2026-05-17
// ANCHOR:ARCH:CORE-001 — Triebwerk-Fundament: Alle anderen Crates hängen von memfuse-core ab.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// INVARIANTE: Kein I/O, kein async, kein Netzwerk — reine Datentypen + Traits.
// Vor jeder Änderung: `cargo check -p memfuse-db` um Downstream-Bruch zu erkennen.

#![forbid(unsafe_code)]

pub mod error;
pub mod snapshot;
pub mod traits;
pub mod tx_buffer;
pub mod types;

pub use error::{MemFuseError, Result};
pub use snapshot::{SnapshotGuard, SnapshotRegistry};
pub use traits::*;
pub use tx_buffer::{IndexOp, TxBuffer};
pub use types::*;
