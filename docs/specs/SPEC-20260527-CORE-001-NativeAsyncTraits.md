# SPEC-20260527-CORE-001-NativeAsyncTraits

## 🎯 1. Das Ziel (Context & "Why")
Modernisierung des Trait-Systems durch Ablösung von `#[async_trait]` durch native async Traits (Rust 1.75+). Ziel ist die Reduktion von Heap-Allokationen (dyn Boxing) in Performance-kritischen Pfaden (Zero-Cost-Abstractions).

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-NATIVE-ASYNC-1]**: Alle Kern-Traits (`Checkpoint`, `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`) nutzen native `async fn` ohne das `#[async_trait]` Makro.
- **[INV-SEND-SYNC-1]**: Die resultierenden Futures müssen `Send` sein, um Kompatibilität mit dem Multithreaded Tokio Scheduler zu gewährleisten.
- **[INV-ZERO-PANIC]**: Die Umstellung darf keine neuen `.unwrap()` oder Panics einführen.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-core`
- **File**: `src/traits.rs`

```rust
// Beispiel Signatur (StorageEngine):
pub trait StorageEngine: Send + Sync {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    // ... weitere Methoden
}
```

- **Betroffene Implementierer**:
    - `memfuse-store`: `LsmStorage`
    - `memfuse-index`: `HnswIndex`
    - `memfuse-text`: `InvertedIndex`
    - `memfuse-graph`: `CsrGraph`

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- Keine funktionalen Änderungen am Fehlerverhalten. Bestehende `Result`-Typen bleiben erhalten.
- Kompilierfehler bei fehlenden `Send`-Bounds in asynchronen Workflows gelten als fehlschlagende Invariante.

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Die "Red-Phase" wird hier primär durch den Compiler abgebildet:
    1. Entfernen von `#[async_trait]` in `memfuse-core`.
    2. Die Implementier-Crates MÜSSEN Kompilierfehler zeigen.
    3. Behebung der Fehler durch Entfernen von `#[async_trait]` in den Impls.
    4. Validierung durch `just test`.
