# Hebel 4: In-Process GGUF & Quantized Tensor Engine (aus Project Chimera SPEC-041)

## 1. Ausgangslage & Optimierungspotenzial für MemFuse
MemFuse erfordert für semantische Vektorsuchen Text-Embeddings. Aktuell nutzt `memfuse-embed` entweder:
- Eine optionale Abhängigkeit zu `ort` (ONNX Runtime), was große C++ Shared Libraries (`libonnxruntime.so`) und Plattform-Inkompatibilitäten mit sich bringt, oder
- Externe HTTP-Aufrufe an lokale Server wie Ollama (`/api/embeddings`), was Netzwerk-Overhead und Latenzen erzeugt und die Unabhängigkeit der Desktop-App gefährdet.

**Project Chimera** hat in `crates/chimera-compute` (SPEC-041) eine reine Rust-Lösung implementiert:
- **Hugging Face Candle + GGUF Quantisierung:** Ermöglicht das direkte Laden von quantisierten SOTA-Embedding-Modellen (z. B. `nomic-embed-text-v1.5.Q4_K_M.gguf` oder `bge-small-en-v1.5.Q8_0.gguf`) **ohne jegliche externe C/C++ Dependencies**.
- **Minimaler RAM-Footprint:** Durch 4-Bit- und 8-Bit-Quantisierung sinkt der Speicherbedarf des Embedding-Modells von ~500 MB auf **unter 80 MB**.
- **Autolinker:** Berechnet semantische Ähnlichkeiten zwischen neuen Chunks und bestehenden Knoten, um den Wissensgraphen (CSR) automatisch mit Kanten anzureichern.

## 2. Extrahierte Komponenten

| Datei | Quelle | Beschreibung |
|:---|:---|:---|
| [`model.rs`](./model.rs) | `chimera-compute/src/model.rs` | `GgufEmbeddingModel` via Candle; lädt quantisierte GGUF-Tensoren und führt Inferenz auf CPU aus |
| [`embedder.rs`](./embedder.rs) | `chimera-compute/src/embedder.rs` | Batch-Embedding-Engine mit Tokio Mutex und automatischem Chunking |
| [`autolinker.rs`](./autolinker.rs) | `chimera-compute/src/autolinker.rs` | Wissensgraph-Autolinker zur Erkennung semantischer Relationen |
| [`SPEC-041_ingest_embedding.md`](./SPEC-041_ingest_embedding.md) | `docs/specs/SPEC-041_ingest_embedding.md` | Vollständige Spezifikation der In-Process Ingest- & Embedding-Pipeline |

## 3. Implementierungsplan für MemFuse
1. Füge `candle-core` und `candle-transformers` als optionale Dependency zu `memfuse-embed` hinzu:
   ```toml
   candle-core = { version = "0.8", default-features = false }
   candle-transformers = { version = "0.8" }
   ```
2. Übertrage `GgufEmbeddingModel` aus [`model.rs`](./model.rs) als neues Backend `CandleGgufEngine` in `crates/memfuse-embed/src/gguf.rs`.
3. Damit kann MemFuse auf jedem x86_64- und ARM64-Linux-Rechner vollständig autark (ohne Ollama, ohne Docker, ohne ONNX Runtime) Embedded RAG ausführen!
