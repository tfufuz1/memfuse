// ANCHOR:ARCH:STORE-001 — LSM-Tree Storage Engine (Triebwerk — Layer 1).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DATEN-PFAD: Client → TxBuffer → WAL → MemTable → SSTable → Compaction
// INVARIANTE: Alle Disk-I/O via tokio::fs (zero std::fs imports).
// MODUL-HIERARCHIE: lsm.rs orchestriert, memtable/wal/sstable sind Bausteine.

// ANCHOR:ARCH:GATE-FV — Formal Verification Gate
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:00 DATE:2026-05-11 STATUS:OPEN
// DONE: Alle krypto- oder parallelitäts-relevanten LSM-Änderungen sind formal verifiziert (Kani/TLA+).
// WATCHDOG: Initializing Gate as OPEN until formal proofs are integrated.

//! MemFuse Store — LSM-Tree based storage engine.
//!
//! Provides persistent key-value storage with WAL, MemTable,
//! SSTable, and background compaction.

#![forbid(unsafe_code)]

pub mod compaction;
pub mod lsm;
pub mod memtable;
pub mod sstable;
pub mod wal;

pub use compaction::{CompactionConfig, CompactionEngine};
pub use lsm::{LsmConfig, LsmStorage};
