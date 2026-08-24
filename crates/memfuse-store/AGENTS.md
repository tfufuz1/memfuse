# memfuse-store — Crate-Level Agent Rules

## Critical Invariants

### fsync Error Propagation
EVERY `sync_all()` and `sync_data()` call MUST propagate errors with `?`.
NEVER use `let _ = dir.sync_all()` — this silently drops WAL durability guarantees.

**Known violations (AI-TAG[SMELL][CRITICAL]):**
- `wal.rs:338` — `let _ = dir.sync_all().await;` (AGT-AUDIT-006)
- `wal.rs:422` — `let _ = dir.sync_all().await;`
- `wal.rs:471` — `let _ = dir.sync_all().await;`
- `lsm.rs:125` — `let _ = dir.sync_all().await;`

### last_committed_tx — Single Load Rule
In `get_at_seq()` and `scan_prefix_at()`: load `last_committed_tx` ONCE at the
start into a local variable. Do NOT re-read during iteration — this breaks
snapshot isolation under concurrent writes.

### WAL HMAC Key Sourcing
ALWAYS use `load_or_create_integrity_key()` to obtain the HMAC key.
NEVER hardcode key material. The key is derived via HKDF from the master key.

### I/O Pattern
- `tokio::fs` for metadata and lifecycle operations
- `std::fs::File` ONLY inside `spawn_blocking` for block-level random-access (ADR-012)
