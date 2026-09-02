# AGENTS.md — memfuse-py
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- PyO3 Python-Bindings für MemFuse DB (Layer 3).
- Kein PR gegen diese Crate ohne begleitenden Python-Test (`maturin develop` + `pytest`).

## Bekannte Fallstricke
- GIL-Handling bei async Rust Calls beachten; keine Rust-Panics in FFI entweichen lassen.

## Relevante rules/*.md
- `rules/testing.md` — Python FFI Integration Verification

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:PY-001] STATUS:IN-PROGRESS (REVIEW-PASS 1/2) (TS:2026-09-02T08:30:27Z) (SESSION:8e159fc9) — Smoke-Test für open(), collection() und close()
- ANCHOR[TEST:PY-002] STATUS:IN-PROGRESS (REVIEW-PASS 1/2) (TS:2026-09-02T08:30:27Z) (SESSION:8e159fc9) — Hybrid-Search Python Integration Test
