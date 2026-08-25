# AGENTS.md — memfuse-ollama
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- Primäres Embedding-Backend via Ollama HTTP (ADR-008).
- Implementiert `TextEmbeddingEngine` Trait aus `memfuse-core`.

## Bekannte Fallstricke
- Parallele Embedding-Batch-Requests nutzen zur Latenz-Mitigierung bei HTTP-I/O.

## Relevante rules/*.md
- `rules/async-io.md` — HTTP Connection Timeout & Retry Patterns

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:OLL-001] STATUS:OPEN — Mock-Server Latency & Error Resilience Tests
