# Atomic Spec: FIND-STO-001 — Compaction Loop Yielding & Cancellation

## 1. Problem Statement
The LSM compaction engine in `memfuse-store` runs a heavy merge operation in a tight loop. This blocks the Tokio executor thread, leading to CPU starvation for other tasks (e.g., concurrent inserts or searches). Additionally, the background `run_loop` lacks a mechanism for graceful shutdown.

## 2. Requirements
- **RQ-1:** The `merge_sstables` function must periodically yield execution back to the Tokio scheduler.
- **RQ-2:** The `run_loop` must support a cancellation mechanism to allow graceful shutdown of the storage engine.
- **RQ-3:** Performance impact of yielding should be minimized by yielding only after a configurable number of processed entries.

## 3. Proposed Changes

### 3.1 `memfuse-store` / `compaction.rs`
- Add `yield_threshold: usize` to `CompactionConfig`. Default: 1000.
- Update `CompactionEngine::run_loop` to accept a cancellation signal. 
  - *Decision:* Use `tokio::sync::watch::Receiver<bool>` as a shutdown signal to avoid adding new dependencies if `tokio-util` is not preferred, OR use `tokio_util::sync::CancellationToken` if available. Since it's not in `Cargo.toml`, I'll use a simple `watch` channel or an `Arc<AtomicBool>`. Actually, a `watch` channel is better for async.
- Update `CompactionEngine::merge_sstables` to:
  - Track the number of entries processed.
  - Call `tokio::task::yield_now().await` every `yield_threshold` entries.
  - Periodically check the cancellation signal (if passed through).

### 3.2 `memfuse-store` / `lsm.rs`
- Pass the shutdown signal from `LsmStorage` to the compaction task.

## 4. Verification Plan
- **Test-1 (Yielding):** unit test that verifies `merge_sstables` completes and doesn't block indefinitely (hard to prove yield with unit tests, but can verify it runs).
- **Test-2 (Cancellation):** unit test that starts a long-running compaction and cancels it, verifying it stops.

## 5. Sovereign Core Invariants
- Zero panics.
- No `unwrap()`.
- Error propagation via `MemFuseError`.
