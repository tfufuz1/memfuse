# AUDIT REPORT: `memfuse-py`

**Datum:** 2026-09-02
**Auditor:** Senior Rust FFI-Engineer (PyO3, GIL, Zero-Panic-Boundary Specialist)
**Crate:** `crates/memfuse-py` (Layer 3 — Python PyO3 Bindings)
**Ziel-Repository:** MemFuse (`https://github.com/tfufuz1/memfuse`)

---

## 1. Executive Summary

Das Crate `memfuse-py` stellt die Python-FFI-Grenzschicht (Layer 3) für die MemFuse-Vektordatenbank bereit. Es nutzt PyO3 (0.24.1) und NumPy für Hochleistungs-Vektoroperationen und IPC.

### Kernaussagen des Audits:
1. **Zero-Panic FFI Boundary:** **PASSED**. Alle FFI-Aufrufe werden über `run_blocking_ffi` mit `std::panic::catch_unwind` ausgeführt. Kein Rust-Panic kann die C-FFI-Grenze überschreiten und undefiniertes Verhalten in Python auslösen.
2. **GIL-Freigabe bei I/O & Async (GIL Release):** **PASSED**. `run_blocking_ffi` verwendet `py.allow_threads()`, um die GIL während Tokio-`block_on`-Aufrufen freizugeben. Parallele Python-Threads laufen unblockiert weiter.
3. **Sub-Interpreter-Isolation (CPython 3.12 / 3.13):** **PASSED**. Das C-Extension-Modul schützt den Prozess-weiten Tokio-Runtime-Singleton (`OnceLock<Runtime>`) durch saubere, vorzeitige Ablehnung (`ImportError`) beim Versuch des Imports in einem Sub-Interpreter.
4. **DAG-Architekturinvariante (ADR-044 / ARCH:DAG-003):** **PASSED**. Die direkte Abhängigkeit von `memfuse-db` ist eine dokumentierte und zugelassene Architektur-Ausnahme für Layer-3-Bindings.
5. **Eingabe-Validierung & FFI-Sicherheit:** **PASSED**. IDs, Dokumentenschlüssel, Vektor-Attribute (NaN/Inf-Check), Batch-Größen (<= 10.000) und Beziehungslabels werden strikt an der FFI-Grenze vor Aufruf der Rust-Kernelemente geprüft.

---

## 2. Invarianten & Zero-Panic-Boundary

### 2.1 Exception Mapping (`memfuse_err`)
`MemFuseError` Instanzen aus `memfuse-core` werden deterministisch in strukturierte Python-Exceptions abgebildet:
- `NotFound` -> `PyKeyError`
- `PolicyViolation` / `Sandbox` -> `PyPermissionError`
- `InvalidInput` / `Serialization` / `Json` -> `MemFuseValueError`
- `Storage` / `Io` / `WalCorruption` -> `MemFuseIOError`
- `Index` / `HnswConnectivityDegraded` -> `MemFuseIndexError`
- `Crypto` -> `MemFuseCryptoError`
- `MemoryBudgetExceeded` -> `PyMemoryError`
- `CapabilityUnsupported` -> `PyNotImplementedError`
- `Internal` / `Cluster` -> `MemFuseInternalError`

Jede erzeugte Exception trägt zusätzliche dynamische Attribute (`kind`, `message`, `details`), die Python-Clients eine präzise programmatische Fehlerbehandlung ermöglichen.

### 2.2 Unwind-Schutz (`run_blocking_ffi`)
```rust
fn run_blocking_ffi<F, R>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce() -> PyResult<R> + Send,
    R: Send,
{
    let panic_result =
        py.allow_threads(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    match panic_result {
        Ok(res) => res,
        Err(panic_payload) => {
            let panic_msg = ...;
            Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Rust panic caught at FFI boundary: {}", panic_msg
            )))
        }
    }
}
```
Inklusive Unit-Test `test_run_blocking_ffi_panic_containment`, der nachweist, dass simulierte Core-Panics als `PyRuntimeError` gefangen werden.

---

## 3. GIL Concurrency & Sub-Interpreter Safety

1. **GIL Concurrency Verification**:
   Parallel ausgeführe Hintergrund-Python-Threads (z.B. in `test_gil_concurrency.py`) laufen bei laufenden Batch-Insert-/Search-Operationen ungestört weiter, da `py.allow_threads()` die GIL während der Rust-Execution freigibt.

2. **Sub-Interpreter Safety**:
   In CPython 3.12/3.13 wird ein Versuch, `_memfuse` in einem Sub-Interpreter zu importieren, direkt mit `ImportError: module _memfuse does not support loading in subinterpreters` abgefangen. Dadurch kann der globale `OnceLock<Runtime>` Tokio-Worker nicht beschädigt werden.

---

## 4. Testabdeckung & Verifikation

Das Python-Testsuite umfasst 36 Integrationstests unter `crates/memfuse-py/tests/`:

```text
============================= test session starts ==============================
platform linux -- Python 3.12.13, pytest-9.1.1, pluggy-1.6.0
rootdir: /app/crates/memfuse-py
configfile: pyproject.toml
plugins: anyio-4.14.2
collected 36 items

crates/memfuse-py/tests/test_bindings.py .............                  [ 36%]
crates/memfuse-py/tests/test_errors.py .........                        [ 61%]
crates/memfuse-py/tests/test_gil_concurrency.py ...                     [ 69%]
crates/memfuse-py/tests/test_mcp_real.py ...                            [ 77%]
crates/memfuse-py/tests/test_mcp_stub.py ..                             [ 83%]
crates/memfuse-py/tests/test_recovery.py ..                             [ 88%]
crates/memfuse-py/tests/test_subinterpreter.py ....                     [100%]

============================== 36 passed in 3.17s ==============================
```

Ergebnis: **Alle 36 Tests erfolgreich (PASSED)**.

---

## 5. Audit-Status

- **Zero-Panic-Boundary:** 🟢 In Ordnung
- **GIL Release:** 🟢 In Ordnung
- **Sub-Interpreter Guard:** 🟢 In Ordnung
- **Unsafe-Code:** 🟢 0 Unsafe Blocks (`#![forbid(unsafe_code)]`)
- **DAG-Invariante:** 🟢 Formell genehmigte Ausnahme (ADR-044 / ARCH:DAG-003)
- **Gesamtergebnis:** 🟢 **PASSED** (Stand: 2026-09-02)
