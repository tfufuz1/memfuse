# AGENTS.md — memfuse-py
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- PyO3 Python-Bindings für MemFuse DB (Layer 3).
- Kein PR gegen diese Crate ohne begleitenden Python-Test (`maturin develop` + `pytest`).
- **Sub-Interpreter-Isolation & Tokio Runtime**:
  - Tokio Runtime ist streng pro Python-Modul-Zustand (`PyRuntimeState` an `_memfuse` `PyModule` gebunden) isoliert.
  - `MEMFUSE_WORKER_THREADS` wird bei der Modul-Initialisierung pro Interpreter-Kontext ausgewertet.
  - Keine statischen globalen `static RUNTIME: OnceLock` Datenstrukturen.
  - Datenbank-Instanzen (`PyMemFuse`) halten direkte Referenzen (`Arc<Runtime>`) auf die Runtime ihres erzeugenden Interpreters.

## Sub-Interpreter-Isolations- und Ressourcen-Richtlinie (PEP 684)
- **Tokio Worker Pool**: Strikt isoliert pro Interpreter/Modul-State. Jeder Sub-Interpreter besitzt ein eigenes, unabhängig konfigurierbares Tokio Worker Pool Instance.
- **Database Handles (`MemFuse`)**: Kapseln Rust Core Handles und die dazugehörige `Arc<Runtime>`. Dateisystem-Sperren verhindern ungeschützte parallele Zugriffe auf denselben Pfad.
- **Python Exceptions & Typen**: Werden pro Modulimport (`_memfuse`) im jeweiligen Interpreter registriert, um PyType-Verschmutzung zu vermeiden.
- **Bekannte Einschränkung (PyO3 0.24.1 / PEP 684)**:
  - PyO3 0.24.1 nutzt `abi3` Single-Phase Module Initialization. CPython 3.12+ lehnt Single-Phase C-Extensions in isolierten Sub-Interpretern ab (`ImportError: module _memfuse does not support loading in subinterpreters`).
  - Dies schützt vor ungewollter Speicher- und Runtime-Teilung.
  - **Upgrade-Pfad**: Sobald PyO3 Multi-Phase Module Initialization (PEP 489 / PEP 684 `Py_mod_multiple_interpreters`) für `abi3` stabilisiert, wird `_memfuse` mit expliziter Multi-Phase-Unterstützung deklariert.

## Bekannte Fallstricke
- GIL-Handling bei async Rust Calls beachten; keine Rust-Panics in FFI entweichen lassen.

## Relevante rules/*.md
- `rules/testing.md` — Python FFI Integration Verification

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:PY-001] STATUS:IN-PROGRESS (REVIEW-PASS 1/2) (TS:2026-09-02T08:30:27Z) (SESSION:8e159fc9) — Smoke-Test für open(), collection() und close()
- ANCHOR[TEST:PY-002] STATUS:IN-PROGRESS (REVIEW-PASS 1/2) (TS:2026-09-02T08:30:27Z) (SESSION:8e159fc9) — Hybrid-Search Python Integration Test
