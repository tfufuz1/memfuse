//! MemFuse Store — LSM-Tree based storage engine.
//!
//! Provides persistent key-value storage with WAL, MemTable,
//! SSTable, and background compaction.

// INVARIANT: LSM-Tree Storage Engine (Triebwerk — Layer 1).
// DATEN-PFAD: Client → TxBuffer → WAL → MemTable → SSTable → Compaction
// INVARIANTE: tokio::fs für Metadaten/Lifecycle, std::fs::File ausschließlich innerhalb spawn_blocking für Block-Level Random-Access.
// ANCHOR:INTEGRATION STATUS:RESOLVED AGENT:04 DATE:2026-08-24
// MODUL-HIERARCHIE: lsm.rs orchestriert, memtable/wal/sstable sind Bausteine.

// INTENT: strictly forbid unsafe_code
// BEGRÜNDUNG: Sovereign Core Doctrine mandates zero unsafe outside `memfuse-index`
#![forbid(unsafe_code)]

pub mod checkpoint;
pub mod compaction;
pub mod lsm;
pub mod memtable;
pub mod sstable;
pub mod wal;

pub use compaction::{CompactionConfig, CompactionEngine};
pub use lsm::{LsmConfig, LsmStorage};
