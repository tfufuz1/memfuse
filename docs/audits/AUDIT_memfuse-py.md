# Audit-Report: `memfuse-py` (Layer 3 — Python PyO3 Bindings)

**Datum/Zeit:** 2026-09-03T19:29:58Z
**Session:** `94a6a82c`
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
- **Shared Tokio Runtime**: A multi-thread Tokio runtime (`memfuse-py-worker`) is lazily initialized via `OnceLock<Runtime>` in `get_runtime()`.
- **Zero-Copy Serialization**: High-performance FlatBuffer search responses (`search_fb`, `hybrid_search_fb`) build raw zero-copy bytes returned as PyBytes.

---

## Audit Verification & Test Delta

- **Rust Unit & Sanity Checks**: `cargo check -p memfuse-py --all-features` (0 errors, 0 warnings).
- **Clippy Analysis**: `cargo clippy -p memfuse-py --no-deps -- -D warnings` (0 findings).
- **Rust Integration Test Suite**: `cargo test -p memfuse-py --all-features` (100% passed).
- **Tier 1 Concurrency Verification**: 5 consecutive runs of `cargo test -p memfuse-py --all-features -- --test-threads=8` executed with 0 failures and 0 panics.

---

## Audit Findings in Session `94a6a82c` (TS: 2026-09-03T19:29:58Z)

| ID | Kategorie | Severity | Datei | Zeile | Beschreibung |
|---|---|---|---|---|---|
| `AGT-PY-ff475c8e` | BUG | MAJOR | `crates/memfuse-py/src/lib.rs` | 1435 | `_trigger_panic_for_test` returns `PyRuntimeError` directly instead of panicking inside `run_blocking_ffi` |

### Detailed Analysis (`AGT-PY-ff475c8e`)
- **Befund:** `_trigger_panic_for_test` constructs and returns `PyRuntimeError` directly instead of causing an actual Rust panic wrapped in `run_blocking_ffi`.
- **Risiko:** FFI panic boundary catching (`catch_unwind` in `run_blocking_ffi`) is not exercised by pytest tests, causing `tests/test_panic_isolation.py` to fail and masking panic boundary regressions.
- **Empfehlung:** Update `_trigger_panic_for_test` to invoke `run_blocking_ffi(py, || panic!("{}", msg))`.

---

## Chaos-Engineering-Audit 2026-09-03

| Szenario | Ergebnis | Recovery-Verhalten | Befund |
|---|---|---|---|
| Crash mid-write | OK | WAL-Flushing & Atomic Storage recover on restart | — |
| Disk-Full ENOSPC | OK | Storage error maps to MemFuseIOError, no panic | — |
| OOM / Backpressure | OK | MAX_BATCH_SIZE (10,000) & thread pool bounds enforced | — |
| SIGBUS mmap-truncate | N/A | No direct mmap usage in memfuse-py | — |
| SIGKILL recovery | OK | LSM/WAL state clean on reopen | — |

---

## Historical Session Audits

### Changes Implemented in Session `9cd9a63a`

1. **`crates/memfuse-py/src/lib.rs`**:
   - Enhanced `validate_id` to enforce `id.len() <= MAX_ID_LENGTH` (1024 bytes), returning `MemFuseValueError` on violation.
   - Enhanced `validate_collection_name` to enforce `name.len() <= 64` bytes, returning `MemFuseValueError` on violation.
2. **`crates/memfuse-py/tests/test_errors.py`**:
   - Added `test_long_id_validation` verifying oversized document ID handling.
   - Added `test_long_collection_name_validation` verifying oversized collection name handling.

### Changes Implemented in Session `8e159fc9` (TS: 2026-09-02T08:30:27Z)

1. **`crates/memfuse-py/src/lib.rs`**:
   - Standardized `validate_collection_name`, `validate_db_path`, and `validate_query_text` to return `MemFuseValueError` instead of standard `PyValueError`.
2. **`crates/memfuse-py/tests/test_errors.py`**:
   - Added `test_empty_collection_name_and_query_validation` testing `MemFuseValueError` raising behavior on empty collection names and empty hybrid search queries.
3. **`crates/memfuse-py/tests/test_bindings.py` & `crates/memfuse-py/AGENTS.md`**:
   - Annotated `test_open_and_close` and `test_hybrid_search` with `ANCHOR[TEST:PY-001]` and `ANCHOR[TEST:PY-002]` tags updating review status to `IN-PROGRESS (REVIEW-PASS 1/2)`.

---

## Tiefen-Audit & Tier 1 FFI Domain Audit 2026-09-04

**Datum/Zeit:** 2026-09-04T13:38:02Z
**Session:** `0415c0ba`
**Crate:** `memfuse-py` (Layer 3 — Python PyO3 Bindings)
**Rolle:** Senior Rust FFI-Engineer — PyO3, GIL, Zero-Panic-Boundary

### Executive Summary & Scope

A comprehensive Tier 1 and FFI domain audit was conducted on `memfuse-py` (`crates/memfuse-py/src/lib.rs`). Inventory check confirmed exact alignment with single-file layout (`lib.rs`).

Key Verifications Completed:
1. **Zero-Panic-Boundary (APM-9)**: Verified `run_blocking_ffi` traps all core Rust panics via `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))` and converts them to structured Python `PyRuntimeError` exceptions, preventing Python interpreter crashes across FFI.
2. **GIL Release & Concurrency**: Verified `py.allow_threads` releases the Python GIL during blocking async Tokio calls, ensuring non-blocking Python thread execution under high concurrency.
3. **Sub-Interpreter Isolation Guard**: Verified `check_subinterpreter_guard` inspects `_xxsubinterpreters` / `_interpreters` to deterministically reject imports in CPython sub-interpreters (interpreter ID != 0) with `PyImportError`, preventing multi-interpreter state corruption.
4. **Boundary Validation & Sanitization**: Verified strict input validation functions (`validate_id`, `validate_collection_name`, `validate_db_path`, `validate_query_text`, `validate_batch_size`, `validate_vector`, `validate_id_obj`) rejecting null bytes, empty strings, oversized IDs (>1024 chars), oversized batch sizes (>10,000), negative integer IDs, and NaN/Inf vector floats.
5. **FlatBuffer IPC Response Serialization (APM-34)**: Verified `search_fb` and `hybrid_search_fb` assemble search results into FlatBuffers IPC payloads using `#![forbid(unsafe_code)]` compliant `PyBytes` copying, eliminating use-after-free and dangling pointer risks over FFI.
6. **Error Mapping Completeness (APM-35)**: Verified `memfuse_err` maps all `MemFuseError` variants via `MemFuseErrorDto` into specific Python exception types (`PyKeyError`, `PyValueError`, `PyPermissionError`, `MemFuseIOError`, `MemFuseIndexError`, `MemFuseCryptoError`, etc.) while dynamically populating `kind`, `message`, and `details` attributes.
7. **Refutation/Status of Prior Findings**: Finding `AGT-PY-ff475c8e` (testing helper panic invocation) was re-verified at `crates/memfuse-py/src/lib.rs:1431` and confirmed fully resolved (`RESOLVED`). No open findings exist in `memfuse-py`.

### Audit Findings Table (Session `0415c0ba`, TS: 2026-09-04T13:38:02Z)

| ID | Kategorie | Severity | Datei | Zeile | Beschreibung | Status |
|---|---|---|---|---|---|---|
| `AGT-PY-ff475c8e` | BUG | MAJOR | `crates/memfuse-py/src/lib.rs` | 1431 | `_trigger_panic_for_test` FFI panic invocation | RESOLVED |

### Tier 1 & FFI Concurrency Sampling
- **Concurrency Stress Test**: 5 consecutive runs of `cargo test -p memfuse-py --lib --all-features -- --test-threads=8` executed with 0 failures and 0 panics.
- **Python Integration Tests**: Executed `maturin develop --release` + `pytest` suite across 7 test files (`test_bindings.py`, `test_errors.py`, `test_gil_concurrency.py`, `test_mcp_real.py`, `test_panic_isolation.py`, `test_recovery.py`, `test_subinterpreter.py`), 100% passing.
- **Sub-Interpreter Guard Verification**: Confirmed deterministic `PyImportError` raising when imported inside sub-interpreters.
