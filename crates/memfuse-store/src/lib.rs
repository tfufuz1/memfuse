//! MemFuse Store — LSM-Tree based storage engine.
//!
//! Provides persistent key-value storage with WAL, MemTable,
//! SSTable, and background compaction.

// INVARIANT: LSM-Tree Storage Engine (Triebwerk — Layer 1).
// DATEN-PFAD: Client → TxBuffer → WAL → MemTable → SSTable → Compaction
// INVARIANTE: Alle Disk-I/O via tokio::fs (zero std::fs imports).
// TODO[STABILIZE][memfuse-store][MAJOR][INVARIANT-DRIFT]
// PROBLEM: SSTable reader and builder use std::fs::File inside spawn_blocking, violating the documented invariant.
// BEWEIS: SstableReader::open_with_key_manager uses std::fs::File::open inside tokio::task::spawn_blocking.
// URSACHE: Async random-access file read is not supported directly by tokio without wrapper overhead, making std::fs::File inside spawn_blocking a necessity for performance.
// LÖSUNG: Update the documented invariant in lib.rs to explicitly allow std::fs usage inside spawn_blocking for block-level access, or refactor to use tokio::fs completely (with performance trade-offs).
// VERIFIKATION: Compile and run benchmarks.
// ABHÄNGIGKEIT: None
// ANCHOR:INTEGRATION STATUS:REVIEW AGENT:02 DATE:2026-05-16
// ANCHOR:INTEGRATION STATUS:REVIEW AGENT:02 DATE:2026-05-16
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
