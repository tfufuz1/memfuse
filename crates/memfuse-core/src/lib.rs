// ANCHOR:ARCH:CORE-001 — Triebwerk-Fundament: Alle anderen Crates hängen von memfuse-core ab.
// Änderungen an public API-Signaturen hier brechen ALLES downstream.
// Vor jeder Änderung: `cargo check -p memfuse-db` um Downstream-Bruch zu erkennen.
// INVARIANTE: Kein I/O, kein async, kein Netzwerk — reine Datentypen + Traits.
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
