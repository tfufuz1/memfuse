# AGENTS.md — memfuse-embed
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- Layer 3 optionales Crate für ONNX Embeddings (Feature-gated, `default = []`).
- Pure-Rust USP wahren durch leere Default-Features (ADR-005, ADR-008).

## Bekannte Fallstricke
- `SessionPool` wurde refactored; `OnnxReranker` nutzt ein eigenes `Arc<Mutex<Session>>` getrennt von `TextEmbedder`s `Semaphore`-Pool.

## Relevante rules/*.md
- `rules/dependencies.md` — Feature-Gated ONNX Dependency Isolation

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:EMB-001] STATUS:DONE — Zero-Panic Refactoring
