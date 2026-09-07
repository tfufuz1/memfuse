# `memfuse-candle` Crate Instructions

## Zweck
Inferenz-Backend auf Basis von Candle (`candle-core`, `candle-transformers`) für native GGUF-Modellausführung.

## Invarianten
- Zero-Panic-Doctrine: Keinesfalls `.unwrap()` oder `.expect()` im Produktionscode verwenden.
- Async Thread Safety: Candle-Tensor-Operationen sind CPU-blockierend. Alle Inferenz- und Embed-Aufrufe MÜSSEN via `tokio::task::spawn_blocking` ausgeführt werden.
- Error Handling: Alle Fehler sind als `MemFuseError` zu strukturieren.
