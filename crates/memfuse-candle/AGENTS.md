# AGENTS.md — memfuse-candle
> Layer 3 | Candle Inferenz-Backend, GGUF-Modelle | ~400 LOC

## 1. Zweck & Architekturrolle

Ermöglicht lokales Candle-basiertes Inferenz-Backend für native GGUF-Modellausführung (`candle-core`, `candle-transformers`).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![forbid(unsafe_code)]`, Modulexporte |
| `embedding.rs` | `CandleEmbedder` — Inferenz & Embeddings via Candle |
| `inference.rs` | `CandleInferenceEngine` — Textgenerierung via Candle |

## 3. Kritische Invarianten

### Zero-Panic-Doctrine
Keinesfalls `.unwrap()` oder `.expect()` im Produktionscode verwenden.

### spawn_blocking-Pattern für Inferenz
Candle-Tensor-Operationen sind CPU-blockierend. Alle Inferenz- und Embed-Aufrufe MÜSSEN via `tokio::task::spawn_blocking` ausgeführt werden.

### Error Handling
Alle Fehler sind als `MemFuseError` zu strukturieren.

## 4. Public API Quick-Reference

```rust
// === CandleEmbedder (embedding.rs) ===
pub struct CandleEmbedder { ... }

// === CandleInferenceEngine (inference.rs) ===
pub struct CandleInferenceEngine { ... }
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Candle Tensors direkt im Tokio Executor Thread ausführen:
let embedding = model.forward(&tensor)?;

// ✅ KORREKT — Kapselung in spawn_blocking:
tokio::task::spawn_blocking(move || {
    model.forward(&tensor)
}).await??;
```

## 6. Concurrency & Lock-Hierarchie

`CandleEmbedder` und `CandleInferenceEngine` kapseln CPU-intensive Tensor-Inferenz. Threads nutzen `tokio::task::spawn_blocking`. Keine Locks nach außen sichtbar.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-agent` (L3 Peer)
- **Genutzt von**: Optionale Inferenz-Backends.

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-005 | Sovereign Core Doctrine |
| `rules/async_drop.md` | spawn_blocking für CPU-bound Workloads |
