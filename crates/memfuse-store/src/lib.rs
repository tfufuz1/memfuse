//! `MemFuse` Store — LSM-Tree based storage engine.
//!
//! Provides persistent key-value storage with WAL, `MemTable`,
//! `SSTable`, and background compaction.
//!
//! # Checkpoint-Architektur
//! `memfuse-store` enthält ein lokales, crate-internes Checkpointing (`pub(crate) mod checkpoint`).
//! Dieses dient ausschließlich als internes MVCC-Snapshot-Pinning (gekoppelt an `SnapshotRegistry`)
//! und darf niemals von außerhalb dieses Crates verwendet werden.
//! Die öffentliche, benannte Checkpoint-API gemäß ADR-011 ("Consolidated Checkpoint Subsystem Architecture")
//! befindet sich im Crate `memfuse-checkpoint`.

// INVARIANT: LSM-Tree Storage Engine (Triebwerk — Layer 1).
// DATEN-PFAD: Client → TxBuffer → WAL → MemTable → SSTable → Compaction
// INVARIANTE: tokio::fs für Metadaten/Lifecycle, std::fs::File ausschließlich innerhalb spawn_blocking für Block-Level Random-Access.
// ANCHOR[INTEGRATION:STO-001] STATUS:RESOLVED (TS:2026-08-24T00:00:00Z)
// MODUL-HIERARCHIE: lsm.rs orchestriert, memtable/wal/sstable sind Bausteine.

// INTENT: deny unsafe_code except Windows Win32 API calls for file permissions
// BEGRÜNDUNG: Sovereign Core Doctrine mandates zero unsafe outside `memfuse-index`,
// except Windows ACL security programming (`SetNamedSecurityInfoW`, etc.).
#![deny(unsafe_code)]

pub(crate) mod checkpoint;
pub mod compaction;
pub mod lsm;
pub mod memtable;
pub mod sstable;
pub(crate) mod util;
pub mod wal;

pub use compaction::{CompactionConfig, CompactionEngine};
pub use lsm::{LsmConfig, LsmStorage};
