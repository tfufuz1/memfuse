# AGENTS.md — memfuse-tauri
> Layer 4 | Tauri Desktop Backend, IPC-Commands, Ingestion-Pipeline | ~2200 LOC

## 1. Zweck & Architekturrolle

Desktop-App-Backend für MemFuse. Hostet die Tauri-App, verwaltet den globalen
`AppState` (inkl. Datenbankverbindung und Modell-Pools), exponiert Tauri-Commands
für das Frontend (IPC) und treibt die `IngestionPipeline` (PDF, DOCX, E-Mail
Parsen) voran, inkl. asynchronem Progress-Reporting.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, Tauri App-Builder, Command-Registrierung |
| `state.rs` | `AppState` — Globaler State (DB, Embedder, Router) im Tauri-Kontext |
| `ollama.rs` | `OllamaBridge` — Tauri-spezifische Wrapper für Ollama (Streaming) |
| `commands/` | `ingest.rs`, `chat.rs`, `transform.rs` — `#[tauri::command]` IPC-Endpoints |
| `ingestion/` | `pipeline.rs`, `pdf.rs`, `docx.rs`, `email.rs`, `progress.rs` — File Extraction & Progress |

## 3. Kritische Invarianten

### AppState-Management (AGT-TAU-001)
Tauri State (`tauri::State<AppState>`) verwendet intern `parking_lot::RwLock`.
**Invariante:** Guard-Drop-vor-Await-Regel! Lese-/Schreib-Guards auf den State
DÜRFEN NIEMALS über einen `.await`-Punkt gehalten werden.
Fehlverhalten führt zu unwiderruflichen Deadlocks im UI-Thread.

### IPC Command Protokoll
Alle `#[tauri::command]` Funktionen **MÜSSEN** asynchron sein, wenn sie I/O oder DB-Aufrufe machen.
Alle Rückgabetypen müssen serialisierbar sein.
Alle Errors müssen in das `MemFuseErrorDto` gemappt werden (`Result<T, MemFuseErrorDto>`),
da native Rust-Errors nicht via IPC an das Frontend serialisiert werden können.

### Progress Emission Throttling
Die `IngestionPipeline` verarbeitet potentiell tausende Dateien.
Event-Emissionen via Tauri (`app_handle.emit_all`) sind teuer.
Es **MUSS** der `IngestProgressThrottler` verwendet werden, um UI-Updates
auf z.B. 100ms Intervalle zu begrenzen (verhindert UI-Freezes).

## 4. Public API Quick-Reference

```rust
// === AppState (state.rs) ===
pub struct AppState { ... }
impl AppState {
    pub fn new() -> Self;
}

// === IPC Commands (commands/*.rs) ===
#[tauri::command]
pub async fn ingest_file(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<IngestReport, MemFuseErrorDto>;

#[tauri::command]
pub async fn chat_with_rag(state: tauri::State<'_, AppState>, query: String) -> Result<ChatResponse, MemFuseErrorDto>;

// === Ingestion (ingestion/pipeline.rs, ingestion/progress.rs) ===
pub struct IngestionPipeline { ... }
impl IngestionPipeline {
    pub async fn ingest_folder(&self, path: &Path, collection: &Collection, emitter: &impl ProgressEmitter) -> Result<IngestReport>;
}
pub trait ProgressEmitter: Send + Sync {
    fn emit_progress(&self, batch: &IngestProgressBatch);
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Lock über await halten (Deadlock):
#[tauri::command]
async fn do_work(state: tauri::State<'_, AppState>) -> Result<(), MemFuseErrorDto> {
    let db = state.db.read(); // Guard erstellt
    db.collection("test").search(...).await?; // AWAIT BLOCKIERT DEN LOCK!
    Ok(())
}

// ✅ KORREKT — Klonen oder Guard manuell droppen:
#[tauri::command]
async fn do_work(state: tauri::State<'_, AppState>) -> Result<(), MemFuseErrorDto> {
    let collection = { state.db.read().collection("test")?.clone() };
    collection.search(...).await?;
    Ok(())
}

// ❌ FALSCH — Native Errors über IPC zurückgeben:
async fn my_cmd() -> Result<(), MemFuseError> { ... }
// ✅ KORREKT:
async fn my_cmd() -> Result<(), MemFuseErrorDto> { ... }
```

## 6. Concurrency & Lock-Hierarchie

Der Tauri-Event-Loop (Main Thread) und der Tokio-Executor (Background Threads)
sind getrennt. IPC-Commands laufen auf Tokio-Tasks.
`AppState` ist der einzige geteilte Zustand, geschützt durch `parking_lot::RwLock`.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: Alle Layer 0-3 Crates
- **Verbotene Imports**: `memfuse-mcp` (L4 Peer)
- **Genutzt von**: Rust-Binary Entrypoint, JS/TS Frontend

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| `rules/async_drop.md` | Hintergrund-Aufgaben und Executor-Blocking |
| `COMMON_LLM_ERRORS.md` | Fehler-Klasse 11: Guard über `.await` halten |
