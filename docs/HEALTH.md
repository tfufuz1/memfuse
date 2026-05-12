# System Health & Verification Gates

// ANCHOR:ARCH:GATE-FV — Formal Verification Gate
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:00 DATE:2026-05-12 STATUS:OPEN
// WATCHDOG: Monitoring Kani/TLA+ proofs for LSM and Crypto components.

## FV Violations (Missing Kani/TLA+)
- crates/memfuse-store/src/lsm.rs
- crates/memfuse-store/src/wal.rs
- crates/memfuse-store/src/sstable.rs
- crates/memfuse-core/src/tx_buffer.rs
- crates/memfuse-core/src/types.rs
