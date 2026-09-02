# AGENTS.md — memfuse-embed
> Layer 3 | Lokale ONNX-Modelle, Embeddings, Cross-Encoder | ~1200 LOC

## 1. Zweck & Architekturrolle

Ermöglicht vollständig lokale, offline-fähige Vektor-Einbettungen und Reranking
(ohne Ollama-Abhängigkeit) mittels ONNX-Runtime (`ort`).
Implementiert den `TextEmbeddingEngine` Trait aus `memfuse-core`.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, `TextEmbedder`, `TextEmbedderConfig` |
| `pool.rs` | `SessionPool` — Thread-sicheres Pooling von ONNX-Sessions (pub(crate)) |
| `reranker.rs` | `CrossEncoderReranker` — Präzises Re-Scoring der RRF-Ergebnisse |

## 3. Kritische Invarianten

### Feature-Gate-Protokoll (`onnx`)
Die gesamte Crate erfordert umfangreiche C++-Abhängigkeiten (`ort`).
Um Pure-Rust-Builds nicht zu brechen, MUSS jeder Code, der `memfuse-embed` nutzt,
dies strikt hinter dem `#[cfg(feature = "onnx")]` Gate verstecken.
Default ist dieses Feature **deaktiviert**.

### spawn_blocking-Pattern für Inferenz
ONNX-Runtime-Aufrufe (`session.run()`) sind blockierend und CPU-intensiv.
Sie DÜRFEN NIEMALS direkt im tokio-Threadpool aufgerufen werden.
Jeder Embedding-Aufruf muss in `tokio::task::spawn_blocking` gekapselt werden,
um den async-Executor nicht zu blockieren (`embed_async` Methode).

### SessionPool Architektur
Das Laden von ONNX-Modellen in den Speicher dauert lange. `SessionPool` (intern)
hält N Modell-Instanzen vor. Aufrufer dürfen nicht für jeden Aufruf ein
`TextEmbedder::new()` machen, sondern müssen den `TextEmbedder` via `Arc` teilen.

## 4. Public API Quick-Reference

```rust
// === TextEmbedder (lib.rs) ===
pub struct TextEmbedder { ... }
impl TextEmbedder {
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self>;
    pub fn load_with_config(model_dir: impl AsRef<Path>, config: TextEmbedderConfig) -> Result<Self>;
    pub async fn embed_async(&self, text: &str) -> Result<Vec<f32>>;
}

pub struct TextEmbedderConfig {
    pub intra_op_threads: usize,
    pub inter_op_threads: usize,
    pub pooling_strategy: PoolingStrategy, // Mean, CLS
}

// === Cross-Encoder Reranking (reranker.rs) ===
pub struct CrossEncoderReranker { ... }
impl CrossEncoderReranker {
    pub fn new(config: RerankConfig) -> Result<Self>;
    pub async fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<RerankResult>>;
}
pub struct RerankResult {
    pub index: usize,
    pub score: f32,
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Code ohne Feature-Gate nutzen:
let embedder = memfuse_embed::TextEmbedder::load("..."); // Bricht CI ohne --features onnx!
// ✅ KORREKT:
#[cfg(feature = "onnx")]
let embedder = memfuse_embed::TextEmbedder::load("...");

// ❌ FALSCH — Modelle pro Request neu laden:
async fn handle_search(query: &str) {
    let embedder = TextEmbedder::load("models/all-MiniLM").unwrap();
    // ...
}
// ✅ KORREKT — Einmal beim Start laden und `Arc<TextEmbedder>` teilen.

// ❌ FALSCH — embed() blockierend aufrufen (Fehlt in API-Ref oben absichtlich):
// ✅ KORREKT — Immer embed_async() nutzen!
```

## 6. Concurrency & Lock-Hierarchie

`TextEmbedder` und `CrossEncoderReranker` sind intern zustandslos bzw. verwalten
ihre ONNX-Sessions thread-safe. Keine Locks nach außen sichtbar. Der C-FFI
der ONNX-Runtime verwaltet seine eigenen internen Threadpools.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-ollama` (L3 Peer)
- **Genutzt von**: Optional in `memfuse-db` (falls konfiguriert) und `memfuse-tauri`.

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-005 | Sovereign Core Doctrine (Erklärt, warum `onnx` default=off ist) |
| `COMMON_LLM_ERRORS.md` | Fehler-Klasse 12: Feature-Gate-Vergessen (`onnx`) |
| `rules/async_drop.md` | spawn_blocking für CPU-bound Workloads |
