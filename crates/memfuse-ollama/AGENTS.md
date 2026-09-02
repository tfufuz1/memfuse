# AGENTS.md — memfuse-ollama
> Layer 3 | Ollama API Client, Embeddings, Context Prefixing | ~2500 LOC

## 1. Zweck & Architekturrolle

Bildet die Schnittstelle zu lokalen (oder remote) Ollama-Instanzen. Implementiert den
`TextEmbeddingEngine` Trait aus `memfuse-core` für Vektor-Einbettungen.
Bietet Funktionen für Text-Generierung, RAG-Chat (Streaming), Importance-Scoring
und Context-Prefix-Generierung (Erzeugung von kurzen Präfixen für Chunking).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, Re-Exports |
| `client.rs` | `OllamaClient`, `OllamaConfig` — Basis-HTTP-Client (reqwest), Retry-Logik, RAG-Prompts |
| `embedding.rs` | `OllamaEmbedder` — Implementiert `TextEmbeddingEngine` für Vektor-Indexierung |
| `context_prefixer.rs` | `ContextPrefixEngine` — Erzeugt 50-100 Token Präfixe zur Context-Erhaltung beim Chunking |
| `importance.rs` | `score_importance` — LLM-gestützte Bewertung der Wichtigkeit eines Memory-Chunks |
| `model_info.rs` | `ModelInfo` — Abruf von Model-Dimensionen und Validierung der Verfügbarkeit |

## 3. Kritische Invarianten

### HTTP Retry & Timeout Patterns
Aufrufe an Ollama können aufgrund lokaler Hardware (OOM, Cold Start) fehlschlagen.
Methoden wie `try_embed_batch` oder `try_generate_text` implementieren einen
eingebauten Retry-Mechanismus mit Exponential Backoff bei transienten Netz-Fehlern.
`embed` und `generate` wrappen diese mit endgültigem Error-Mapping.

### ContextPrefixEngine Protokoll
Beim Chunking (siehe `memfuse-db/chunker.rs`) kann optional die `ContextPrefixEngine`
genutzt werden, um jedem Chunk ein 50-100 Token langes Präfix voranzustellen, das
den globalen Dokument-Kontext beschreibt. Dieses Präfix muss strikt an die Token-Grenzen
gehalten werden (via `truncate_prefix`).

### Dimension-Discovery
Der HNSW-Index benötigt eine feste Dimension. Wenn Ollama-Modelle verwendet werden,
MUSS die Dimension idealerweise über `known_dimension()` statisch abgefragt oder
einmalig via Dummy-Embedding dynamisch ermittelt werden.

## 4. Public API Quick-Reference

```rust
// === HTTP Client (client.rs) ===
pub struct OllamaClient { ... }
impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self;
    pub async fn embed_batch(&self, model: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    pub async fn generate_text(&self, model: &str, prompt: &str) -> Result<String>;
    pub async fn is_model_available(&self, model: &str) -> bool;
    pub async fn chat_with_rag_streaming(...) -> Result<...>; // Streaming response
}

// === TextEmbeddingEngine (embedding.rs) ===
pub struct OllamaEmbedder { ... }
impl OllamaEmbedder {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self;
    pub fn with_expected_dimension(self, dim: usize) -> Self;
    // Implementiert TextEmbeddingEngine::embed und embed_batch
}

// === Context Prefixer (context_prefixer.rs) ===
pub struct ContextPrefixEngine { ... }
impl ContextPrefixEngine {
    pub async fn generate_prefix(&self, text: &str) -> Result<String>;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Ungeprüfte Model-Verwendung:
client.generate_text("llama3", prompt).await?; // Model vielleicht nicht gepullt!
// ✅ KORREKT — Vorher prüfen:
client.ensure_model_available("llama3").await?;

// ❌ FALSCH — reqwest direkt verwenden:
let res = reqwest::get("http://localhost:11434/api/generate").await?;
// ✅ KORREKT — IMMER OllamaClient verwenden (enthält Retry-Logik und Error-Handling).

// ❌ FALSCH — Eigene Prompt-Formatierung für RAG:
// ✅ KORREKT — `build_rag_prompt()` verwenden.
```

## 6. Concurrency & Lock-Hierarchie

`OllamaClient` hält intern einen thread-safen `reqwest::Client`. Erfordert keine
eigenen Locks. Bei parallelen `embed_batch` Aufrufen sorgt der Client-interne Connection-Pool
für die Steuerung. (Achtung: Zu hohe Concurrency bringt die lokale GPU zum OOM).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-store` (L1)
- **Genutzt von**: `memfuse-db` (MultiStep, ContextCompaction), `memfuse-agent`

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-039 | `reqwest` Workspace-Dependency Regel (Kein reqwest-blocker, aber Error-Mapping) |
| `rules/llm_protocol.md` | OOM Prevention bei Batch-Embeddings |
