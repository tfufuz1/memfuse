# Phase 2: DiskANN & ADR-037 Generalisierung Audit Report

**Datum:** 2026-09-03
**Crate:** `memfuse-db`
**Thema:** `Collection<S, V>` VectorIndex-Generalisierung & DiskANN-Integration

---

## 1. Struct-Diff (`Collection` Definition)

### `crates/memfuse-db/src/collection/mod.rs`
```rust
// Vorher (Monomorph gekoppelt an HnswIndex):
pub struct Collection<S: StorageEngine = LsmStorage> {
    pub(super) index: Arc<HnswIndex>,
    // ...
}

// Nachher (ADR-037 Generisch):
pub struct Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex> {
    pub(super) index: Arc<V>,
    // ...
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    pub fn new(
        name: String,
        storage: Arc<S>,
        index: Arc<V>,
        graph_index: Arc<CsrGraph>,
        next_tx: Arc<AtomicU64>,
        dimension: usize,
        language: Language,
    ) -> Self { ... }
}

impl<S: StorageEngine> Collection<S, HnswIndex> {
    pub fn with_hnsw(...) -> Self { ... }
}
```

---

## 2. Aufrufer-Anpassungen & Abwärtskompatibilität

- Durch den Type-Default `V = HnswIndex` in `Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex>` bleiben alle bestehenden Typ-Signaturen wie `Collection<LsmStorage>` im gesamten Workspace 100% abwärtskompatibel.
- In `crates/memfuse-db/src/collection/tests.rs` wurden bei `test_collection_with_diskann_index_hybrid_search` die Modulqualifikationen für `super::StoredDocument` und `super::StoredDocumentMeta` bereinigt, um die Instanziierung von `Collection::<LsmStorage, DiskAnnIndex>::new(...)` sauber im Test harness auszuführen.

---

## 3. Feature-Flag Verifikation (`experimental-diskann`)

- `crates/memfuse-db/Cargo.toml` verdrahtet das Feature wie folgt:
  ```toml
  [features]
  experimental-diskann = ["memfuse-index/experimental-diskann"]
  ```
- `DiskAnnIndex` und dessen Nutzung in `Collection` wird ausschließlich in Tests oder hinter dem Feature-Flag `experimental-diskann` kompilativ einbezogen.
- Standard-Kompilate ohne Feature-Flag verweisen nicht unkontrolliert auf DiskANN.

---

## 4. Test-Execution Output

```
$ cargo test -p memfuse-db --features experimental-diskann test_collection_with_diskann_index_hybrid_search -- --nocapture

running 1 test
test collection::tests::test_collection_with_diskann_index_hybrid_search ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 141 filtered out; finished in 0.06s
```

Vollständiger Check:
- `cargo check --workspace --exclude memfuse-tauri`: OK
- `cargo check -p memfuse-db`: OK
- `cargo check -p memfuse-db --features experimental-diskann`: OK
- `cargo clippy -p memfuse-db -- -D warnings`: OK
- `cargo test -p memfuse-db`: OK (141 unit tests + all integration test suites passed)
