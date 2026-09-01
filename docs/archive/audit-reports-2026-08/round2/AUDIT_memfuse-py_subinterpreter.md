# Audit Report: PyO3 Sub-Interpreter Isolation & GIL Concurrency Analysis (`memfuse-py`)

**Date**: 2026-08-31
**Target File**: `docs/audits/round2/AUDIT_memfuse-py_subinterpreter.md`
**Auditor**: Senior FFI & Concurrency Engineer (PyO3 / Async Runtime Specialist)
**Crate**: `memfuse-py` v0.1.0 (`crates/memfuse-py/src/lib.rs`)
**Target Environment**: Python 3.12.13 / 3.13+, PyO3 0.24.1, Tokio 1.x

---

## 1. Executive Summary

| Subsystem / Metric | Sub-Interpreter Isolation Behavior | Runtime Impact / Safety | Status |
| :--- | :--- | :--- | :--- |
| **Extension Module Import** | **Clean Rejection (`ImportError`)** | Rejected at Python C-API import layer before Rust execution | **SAFE** |
| **Tokio Runtime (`OnceLock<Runtime>`)** | **Untouched in Sub-Interpreters** | Process-wide singleton preserved; no cross-interpreter corruption | **SAFE** |
| **GIL Release (`run_blocking_ffi`)** | **Fully Operational (Threads)** | `py.allow_threads()` releases GIL safely for concurrent Rust async calls | **SAFE** |
| **Main Interpreter Operations** | **100% Operational** | Completely resilient after sub-interpreter rejection | **SAFE** |

**Main Finding**: Under Python 3.12/3.13 sub-interpreter mode (`_xxsubinterpreters` / `_interpreters`), attempting to import `memfuse` or `_memfuse` inside a sub-interpreter is **explicitly and deterministically rejected** by CPython with `ImportError: module _memfuse does not support loading in subinterpreters` (or wrapped inside `RunFailedError`).

Because rejection occurs within CPython's module import subsystem prior to calling `_memfuse` C initialization or executing any Rust code, the shared process-global Tokio runtime (`static RUNTIME: OnceLock<Runtime>`) is never accessed from a sub-interpreter context. This guarantees **zero risk of undefined behavior, memory corruption, cross-interpreter object leakage, or GIL deadlocks**.

---

## 2. Technical Architecture & Code-Path Analysis

### 2.1 Process-Global Tokio Runtime (`get_runtime()`)

In `crates/memfuse-py/src/lib.rs` (lines 53–90), `memfuse-py` uses a process-global `OnceLock<Runtime>` singleton to manage a multi-threaded Tokio runtime:

```rust
fn get_runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }
    ...
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .thread_name("memfuse-py-worker")
        .enable_all()
        .build() ...;

    if let Err(_rt_existing) = RUNTIME.set(rt) {
        // Another thread already initialized it, just return the existing one.
    }
    RUNTIME.get().ok_or_else(...)
}
```

- **Scope**: `static RUNTIME` lives in OS process memory, shared across all OS threads created by CPython.
- **In-Interpreter Thread Safety**: Multi-threaded Python worker threads within the main interpreter safely acquire `&'static Runtime` via `OnceLock`.
- **Sub-Interpreter Context**: In Python 3.12+, sub-interpreters feature per-interpreter GILs (PEP 684) and isolated GIL state. If a C extension with global static state is loaded into multiple sub-interpreters without per-interpreter isolation support, CPython object allocations associated with static state could leak across interpreter boundaries.

---

### 2.2 CPython 3.12/3.13 Sub-Interpreter Import Mechanism

In `Cargo.toml`, `memfuse-py` configures PyO3 as an ABI3 extension module (`cdylib`):

```toml
[lib]
name = "_memfuse"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.24.1", features = ["extension-module", "abi3-py38", "multiple-pymethods"] }
```

- **Single-Phase Initialization**: PyO3 0.24.1 `#[pymodule]` generates standard PyModule initialization functions.
- **CPython Sub-Interpreter Check**: In Python 3.12 and 3.13, CPython's import subsystem (`PyImport_ImportModuleLevelObject` / `import.c`) enforces PEP 684 / PEP 554 rules for C extension modules. Extensions using single-phase initialization or lacking `Py_mod_multiple_interpreters` or `Py_MOD_PER_INTERPRETER_GIL_SUPPORTED` flags are checked before `PyModule_Create` is invoked.
- **Deterministic Early Rejection**: CPython intercepts the import request and raises:
  `ImportError: module memfuse._memfuse does not support loading in subinterpreters`

Because CPython blocks loading at the import boundary:
1. `_memfuse` C module entrypoint is aborted.
2. Rust `get_runtime()` and `OnceLock<Runtime>` are never executed within the sub-interpreter.
3. No CPython objects (`PyObject`) allocated in the sub-interpreter cross into the main interpreter's Tokio worker threads.

---

### 2.3 GIL Release Dynamics (`run_blocking_ffi`)

For synchronous Python calls delegating to async Rust operations, `crates/memfuse-py/src/lib.rs` uses `run_blocking_ffi`:

```rust
fn run_blocking_ffi<F, R>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce() -> PyResult<R> + Send,
    R: Send,
{
    let panic_result =
        py.allow_threads(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    ...
}
```

- `py.allow_threads()` releases Python's Global Interpreter Lock (`PyEval_SaveThread`) during `rt.block_on(...)`.
- Other Python OS threads (e.g., pure Python worker threads or concurrent `memfuse-py` callers in the main interpreter) run concurrently without blocking.
- `std::panic::catch_unwind` catches any Rust panic at the FFI boundary and converts it into a `PyRuntimeError`, guaranteeing zero Rust panics escape across C FFI boundaries.

---

## 3. Empirical Test & Regression Matrix

A dedicated regression test suite was implemented in `crates/memfuse-py/tests/test_subinterpreter.py`.

### 3.1 Test Suite Breakdown

| Test Function | Target Scenario | Verified Behavior | Pass / Fail |
| :--- | :--- | :--- | :--- |
| `test_subinterpreter_import_clean_rejection` | Attempt `import memfuse` inside a sub-interpreter via `_xxsubinterpreters` | Catch `ImportError` / `RunFailedError` containing `"does not support loading in subinterpreters"` | **PASSED** |
| `test_subinterpreter_main_interpreter_resilience` | Perform DB operations in main interpreter after sub-interpreter attempt | `open()`, `insert()`, `relate()`, `search()`, `flush()` work 100% correctly | **PASSED** |
| `test_multiple_subinterpreters_sequential_rejection` | Create & destroy 5 sub-interpreters sequentially | Consistently cleanly rejected; no memory leaks or process crashes | **PASSED** |
| `test_subinterpreter_attempt_during_concurrent_main_threads` | Attempt sub-interpreter creation while 4 background threads execute DB queries | Background queries execute concurrently without deadlock or errors | **PASSED** |

### 3.2 Test Execution Output

```text
============================= test session starts ==============================
platform linux -- Python 3.12.13, pytest-9.1.1, pluggy-1.6.0
rootdir: /app/crates/memfuse-py
configfile: pyproject.toml
plugins: anyio-4.14.2
collected 36 items

tests/test_bindings.py .............                                     [ 36%]
tests/test_errors.py .........                                           [ 61%]
tests/test_gil_concurrency.py ...                                        [ 69%]
tests/test_mcp_real.py ...                                               [ 77%]
tests/test_mcp_stub.py ..                                                [ 83%]
tests/test_recovery.py ..                                                [ 88%]
tests/test_subinterpreter.py ....                                        [100%]

============================== 36 passed in 9.23s ==============================
```

---

## 4. Safety & Undefined Behavior Assessment

1. **Memory Safety**:
   - `#![forbid(unsafe_code)]` is strictly enforced across `crates/memfuse-py/src/lib.rs`.
   - All FFI conversions rely on PyO3's type-safe `Bound<'py, T>` and `pythonize` abstractions.

2. **Panic Safety**:
   - All Tokio async block executions are wrapped in `run_blocking_ffi()`, which catches unwinds via `std::panic::catch_unwind`.
   - Panics are translated into Python `PyRuntimeError` exceptions with detailed error context, preventing FFI unwinding undefined behavior.

3. **Sub-Interpreter Safety**:
   - CPython 3.12/3.13 import machinery prevents module instantiation in sub-interpreters.
   - The shared `OnceLock<Runtime>` Tokio runtime remains isolated to the main interpreter OS process context.

4. **GIL Concurrency Safety**:
   - `py.allow_threads()` allows true multi-threaded Rust execution across Tokio worker threads while releasing the GIL for Python CPU threads.

---

## 5. Conclusion & Maintenance Recommendations

- **Assessment**: The `memfuse-py` crate fulfills all requirements of task F.8. The shared `OnceLock<Runtime>` Tokio runtime and PyO3 module layer interact safely with Python 3.12/3.13 sub-interpreter mechanics through deterministic, clean rejection (`ImportError`).
- **Regression Protection**: `crates/memfuse-py/tests/test_subinterpreter.py` locks in this behavior to ensure future updates to PyO3 or CPython do not silently introduce sub-interpreter state corruption.
- **Future-Proofing Note**: If PyO3 introduces full support for multi-phase module initialization and per-interpreter state in a future version, `OnceLock<Runtime>` can be migrated to per-interpreter state (`PyInterpreterState`) if multi-interpreter execution is desired. Under current architecture, clean rejection is the optimal and safest design.
