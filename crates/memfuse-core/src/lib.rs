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

// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs for REVIEW components (WAL/LSM).
// INVARIANT: Triebwerk-Fundament: Alle anderen Crates hängen von memfuse-core ab.
// INVARIANTE: Kein I/O, kein async, kein Netzwerk — reine Datentypen + Traits.
// Vor jeder Änderung: `cargo check -p memfuse-db` um Downstream-Bruch zu erkennen.

#![deny(unsafe_code)]

pub mod error;
pub mod ipc;
pub mod snapshot;
pub mod traits;
pub mod tx_buffer;
pub mod types;

pub use error::{MemFuseError, Result};
pub use snapshot::{SnapshotGuard, SnapshotRegistry};
pub use traits::*;
pub use tx_buffer::{IndexOp, TxBuffer};
pub use types::*;
