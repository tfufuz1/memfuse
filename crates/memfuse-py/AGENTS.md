# AGENTS.md — memfuse-py
> Layer 3 | PyO3 Python Bindings, FFI-Boundary | ~1500 LOC

## 1. Zweck & Architekturrolle

Die offizielle Python-Anbindung für MemFuse. Übersetzt die asynchrone Rust-API (`memfuse-db`)
in eine synchrone und asynchrone Python-API via PyO3. Konvertiert Python-Typen
(NumPy, Dictionaries, Strings) in native MemFuse-Typen und kapselt den tokio-Threadpool.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, `PyMemFuse`, `PyCollection`, PyO3 Modul-Registrierung |
| `types.rs` | Konvertierung (FFI) für Dokumente, Suchergebnisse, NumPy-Vektoren |
| `error.rs` | FFI-Error-Mapping: `MemFuseError` -> Python Exceptions |
| `gil.rs` | GIL-Management und Sub-Interpreter State-Isolation |

## 3. Kritische Invarianten

### GIL-Release-Protokoll (AGT-PY-001)
Rust-Code DARF den Python Global Interpreter Lock (GIL) NICHT halten,
während I/O- oder rechenintensive Operationen ausgeführt werden.
Blockierende oder asynchrone memfuse-Aufrufe MÜSSEN in `py.allow_threads(|| { ... })`
oder äquivalente PyO3-Mechanismen gewrappt werden, um Python-Threads nicht zu freezen.

### FFI-Error-Mapping
Fehler aus Rust (`MemFuseError`) müssen in semantisch korrekte Python-Exceptions
übersetzt werden.
- `MemFuseError::NotFound` -> `KeyError` oder `ValueError`
- `MemFuseError::Storage` -> `IOError`
- `MemFuseError::Validation` -> `ValueError`
- `MemFuseError::Internal` -> `RuntimeError`

### NumPy Zero-Copy Pattern
Vektor-Embeddings sollten, wenn möglich, als `PyReadonlyArray1<f32>` (aus der `numpy` crate)
entgegengenommen werden, um teure Memory-Kopien an der FFI-Grenze zu vermeiden.

### Sub-Interpreter Isolation
Globale State-Variablen (wie lazy_statics oder tokio Runtimes) müssen isoliert
verwaltet werden, um Kompatibilität mit Pythons Sub-Interpretern (PEP 684) zu gewährleisten.

## 4. Public API Quick-Reference

```rust
// === PyMemFuse (lib.rs) ===
#[pyclass]
pub struct PyMemFuse { ... }
#[pymethods]
impl PyMemFuse {
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection>;
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyDbStats>;
}

// === PyCollection (lib.rs) ===
#[pyclass]
pub struct PyCollection { ... }
#[pymethods]
impl PyCollection {
    pub fn insert<'py>(&self, py: Python<'py>, id: &str, text: &str) -> PyResult<()>;
    pub fn get(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyDocument>>;
    pub fn hybrid_search<'py>(&self, py: Python<'py>, query: &str) -> PyResult<Vec<PySearchResult>>;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — GIL über asynchrone Blockierung halten:
// (Hält den GIL und blockiert alle Python Threads)
let result = tokio::runtime::Handle::current().block_on(async { self.db.search().await });

// ✅ KORREKT — GIL explizit releasen:
let result = py.allow_threads(|| {
    tokio::runtime::Handle::current().block_on(async { self.db.search().await })
});

// ❌ FALSCH — Rohe Vec<f32> für API erwarten:
pub fn search(&self, vector: Vec<f32>) { ... }
// ✅ KORREKT — NumPy Array für Zero-Copy erlauben:
pub fn search(&self, vector: numpy::PyReadonlyArray1<f32>) { ... }
```

## 6. Concurrency & Lock-Hierarchie

`PyMemFuse` besitzt einen eigenen `tokio` Threadpool für die Ausführung der asynchronen
memfuse-core/-db Tasks. Dieser Threadpool lebt unabhängig von Python-Threads.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: Alle Layer 0-2 Crates (`memfuse-core`, `memfuse-db`, etc.)
- **Verbotene Imports**: `memfuse-mcp` (L4), `memfuse-tauri` (L4), `memfuse-agent` (L3 Peer)
- **Genutzt von**: Python User-Space

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-042 | PyO3 Architektur & Async/Sync Bridging |
| `rules/ffi_boundary.md` | FFI-Panics verhindern, Python Exceptions |
