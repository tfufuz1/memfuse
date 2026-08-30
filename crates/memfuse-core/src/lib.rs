//! `MemFuse` Core — Types, traits, and error handling.
//!
//! This crate provides the foundational building blocks for the `MemFuse`
//! embedded hybrid-search library.
//!
//! # Architecture Role (Triebwerk — Layer 0)
//!
//! This is the **dependency root** of the entire workspace. Every other crate
//! depends on `memfuse-core`. It provides:
//! - **Type IDs**: [`DocId`], [`EntityId`], [`TxId`] — all `#[repr(transparent)]` u64 newtypes
//! - **Traits**: [`StorageEngine`], [`VectorIndex`] — async interfaces for subsystems
//! - **Error**: [`MemFuseError`] — unified error enum, zero-panic via `?` propagation
//! - **`TxBuffer`**: Sharded transaction staging with orphan reaper
//! - **Snapshots**: MVCC read isolation via [`SnapshotRegistry`]

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Core types, traits, and error handling for MemFuse.
// INVARIANTEN: Triebwerk-Fundament: Alle anderen Crates hängen von memfuse-core ab. Kein I/O, kein async, kein Netzwerk in types.
// HOTSPOTS: 1-45
// NICHT-OFFENSICHTLICH: TxId allocation base ranges separate system and collection transactions.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md (ADR-028)
// AGENT-NOTIZ: Demonstrating second-precision TS, SESSION hash, hash-based ID and REVIEW-PASS grammar.

// ANCHOR[DEBT:CORE-INLINE-001] STATUS:DONE (ID: AGT-CORE-a3f29c1d) (TS:2026-08-29T09:14:07Z) (SESSION: a3f29c1d)
// AUFGABE : Inline-Kontextsystem demonstrieren und absichern
// GATE    : cargo test -p memfuse-core
// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T10:00:00Z) (SESSION: b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Tag-Grammatik und SESSION-Identität verifiziert.
// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T11:00:00Z) (SESSION: c9f5e2b3)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Unabhängiges Zweit-Review auf frischem Kontextschnitt durchgeführt.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod error_dto;
// ANCHOR[BUILD:XTASK-001] STATUS:DONE (ID: AGT-XTASK-P01) (TS:2026-08-30T00:00:00Z) (SESSION:jules-p01) - xtask compilation blocker fixed (P0-1)
pub mod ipc;
pub mod seq_log;
pub mod snapshot;
pub mod traits;
pub mod tx_buffer;
pub mod types;

pub use error::{MemFuseError, Result};
pub use error_dto::MemFuseErrorDto;
pub use seq_log::{SeqLogEntry, SequenceLog};
pub use snapshot::{SnapshotGuard, SnapshotRegistry};
pub use traits::*;
pub use tx_buffer::{IndexOp, TxBuffer};
pub use types::*;
