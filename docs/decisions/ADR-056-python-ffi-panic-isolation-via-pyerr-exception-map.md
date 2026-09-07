# ADR-056: Python FFI Panic Isolation via PyErr Exception Mapping

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Ersetzung aller `panic!()` Aufrufe in Nicht-Test-Quellcode von `memfuse-py` durch strukturierte PyO3 Exception-Returns (`PyValueError`, `PyRuntimeError`). Blockierende Aufrufe werden durch `run_blocking_ffi` mit `std::panic::catch_unwind` geschützt.
*   **Begründung**: Verhindert CPython-Prozessabstürze über die PyO3 FFI-Grenze hinweg.

---
