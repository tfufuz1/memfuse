# ADR-059: Python FFI Panic Isolation (ehemals docs/decisions/ADR-048)

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Alle `panic!()`-Aufrufe in `memfuse-py` außerhalb von `#[cfg(test)]` werden durch `Err(PyErr)` ersetzt.
*   **Begründung**: Ein Rust-Panic über die PyO3 FFI-Grenze hinweg führt zum Absturz von CPython. `catch_unwind` ist kein Ersatz für korrekte Fehlerbehandlung an Aufrufstellen.

---
