# Audit-Report: `memfuse-py` (Layer 3 — Python PyO3 Bindings)

**Datum/Zeit:** 2026-09-11T10:30:00Z
**Session:** `1a5d121f`
**Crate:** `memfuse-py`
**Rolle:** Senior Rust FFI-Engineer — PyO3, GIL, Zero-Panic-Boundary

---

## Executive Summary

`memfuse-py` serves as the Layer 3 Python bridge for the MemFuse embedded vector engine. It exposes high-performance hybrid search, CRUD, and graph relationship methods to Python via PyO3, NumPy, and FlatBuffers.

The current audit verified:
1. **Zero-Panic-Boundary**: All blocking FFI executions wrap underlying Tokio async operations via `run_blocking_ffi`, using `std::panic::catch_unwind` and `AssertUnwindSafe`. Panics in Rust core are caught and translated to clean `PyRuntimeError` exceptions rather than aborting/crashing the Python process.
2. **GIL Management**: `run_blocking_ffi` uses `py.allow_threads()` during async Tokio `block_on` execution, releasing the Python GIL to allow concurrent Python threads to run unimpeded.
3. **Error Mapping Completeness**: `memfuse_err` maps all `memfuse_core::MemFuseError` variants via `MemFuseErrorDto` into PyO3 exception types (`PyKeyError`, `PyValueError`, `PyPermissionError`, `MemFuseIOError`, `MemFuseIndexError`, `MemFuseCryptoError`, etc.) while attaching `kind`, `message`, and `details` attributes onto the Python exception instances.
4. **Boundary & Input Validation**: Hard input boundary guards validate document IDs (non-empty, max 1024 bytes), collection names (non-empty, max 64 bytes), query text, batch sizes (max 10,000 items), and vectors (non-empty, finite f32 without NaN or Inf).
5. **Sub-Interpreter Safety**: CPython sub-interpreter imports are cleanly rejected with explicit `ImportError` ("does not support loading in subinterpreters"), preventing shared process state corruption or OnceLock runtime double-initialization.

---

## Key Invariants & Safeguards

- **#[forbid(unsafe_code)]**: `memfuse-py` maintains a strict `#![forbid(unsafe_code)]` directive.
- **Shared Tokio Runtime**: A multi-thread Tokio runtime (`memfuse-py-worker`) is lazily initialized attached to per-interpreter module state in `get_runtime()`.

---

## Audit Verification & Test Delta (Session `1a5d121f`, TS: 2026-09-11T10:30:00Z)

- **Inventory Reality Check**: Confirmed `crates/memfuse-py/src/lib.rs` (1639 lines) matches actual repo inventory with 0 drift.
- **Rust Checks**: `cargo check --manifest-path crates/memfuse-py/Cargo.toml --all-features` (0 errors, 0 warnings).
- **Clippy Analysis**: `cargo clippy --manifest-path crates/memfuse-py/Cargo.toml -- -D warnings` (0 findings).
- **Formatting**: `cargo fmt --check --manifest-path crates/memfuse-py/Cargo.toml` (0 diffs).
- **Tier 1 Concurrency Verification**: 5 consecutive runs of `cargo test --manifest-path crates/memfuse-py/Cargo.toml --all-features -- --test-threads=8` completed with 0 failures and 0 panics.
- **Python Integration Suite**: Built release extension wheel via `maturin develop --release` in virtualenv and executed full `pytest` suite (52 test cases passed, 100% pass rate).

---

## Tiefen-Audit 2026-09-11

### Summary of Tier 1 Verification
- **Rust Unit & Sanity Checks**: `cargo check --manifest-path crates/memfuse-py/Cargo.toml --all-features` (0 errors, 0 warnings).
- **Clippy Analysis**: `cargo clippy --manifest-path crates/memfuse-py/Cargo.toml -- -D warnings` (0 findings).
- **Rust Integration Test Suite**: `cargo test --manifest-path crates/memfuse-py/Cargo.toml --all-features` (100% passed).
- **Tier 1 Concurrency Verification**: 5 consecutive runs of `cargo test --manifest-path crates/memfuse-py/Cargo.toml --all-features -- --test-threads=8` executed with 0 failures and 0 panics.
- **Python FFI / Integration Suite**: `pytest` executed 52 test cases with 100% pass rate in virtualenv mode (including GIL release, sub-interpreter rejection, panic containment, MCP stub/real, and WAL recovery).

---

## Chaos-Engineering-Audit 2026-09-11

| Szenario | Ergebnis | Recovery-Verhalten | Befund |
|---|---|---|---|
| Crash mid-write | OK | WAL-Flushing & Atomic Storage recover on restart | — |
| Disk-Full ENOSPC | OK | Storage error maps to MemFuseIOError, no panic | — |
| OOM / Backpressure | OK | MAX_BATCH_SIZE (10,000) & thread pool bounds enforced | — |
| SIGBUS mmap-truncate | N/A | No direct mmap usage in memfuse-py | — |
| SIGKILL recovery | OK | LSM/WAL state clean on reopen | — |
