# AGENTS.md — memfuse-py
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- PyO3 Python-Bindings für MemFuse DB (Layer 3).
- Kein PR gegen diese Crate ohne begleitenden Python-Test (`maturin develop` + `pytest`).

## Bekannte Fallstricke
- GIL-Handling bei async Rust Calls beachten; keine Rust-Panics in FFI entweichen lassen.

## Relevante rules/*.md
- `rules/testing.md` — Python FFI Integration Verification

## Pflicht-Tests Status (ANCHOR-Status)
- ANCHOR[TEST:PY-001] STATUS:DONE (TS:2026-08-31T21:15:40Z SESSION:846802ab) — Smoke-Test für open(), collection() und close() in test_bindings.py
- ANCHOR[TEST:PY-002] STATUS:DONE (TS:2026-08-31T21:15:40Z SESSION:846802ab) — Hybrid-Search Python Integration Test in test_bindings.py
