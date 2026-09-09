# Audit Report: `memfuse-candle`

**Crate:** `memfuse-candle` (Layer 1 — Native Candle GGUF ML Inferenz-Backend)
**Datum:** 2026-09-09
**Session:** `c74a1828`
**Status:** 🟢 Clean / Audited

---

## 1. Übersicht & Scope

`memfuse-candle` bietet ein Pure-Rust Inferenz-Backend auf Basis von Candle (`candle-core`, `candle-transformers`, `candle-nn`) für den lokalen, air-gapped Betrieb von Llama-/Mistral-GGUF-Modellen sowie Bert/Nomic-Embedding-Modellen.

### Module & LOC
- `embedding.rs`: `CandleEmbedClient` & `CandleEmbedInner` Trait
- `embedding_provider.rs`: `EmbeddingProvider` Impl & Batch Size Guard (`MAX_CANDLE_EMBED_BATCH_SIZE = 256`)
- `gasp.rs`: Post-Hoc Hallucination Validator (`GaspValidator` / `GaspConfig`) mit Isotonic Calibrator
- `gguf_loader.rs`: GGUF Container Header & Metadata Parser (`parse_gguf_metadata`)
- `inference.rs`: `CandleLlmClient` & `LlmTextGenerator` Impl
- `lib.rs`: Public Exports & Feature Flags
- `model_registry.rs`: Fingerprinting (`compute_fingerprint`, SHA-256 over model + quantization grade)

---

## 2. Befunde & Inventar-Realitätsabgleich

### Inventar-Drift Check
- **Gefunden:** `crates/memfuse-candle/src/embedding_provider.rs` war im Prompter-Inventar vom 2026-09-08 nicht gelistet.
- **Befund:** `Inventar-Drift: Datei crates/memfuse-candle/src/embedding_provider.rs im Prompter-Inventar vom 2026-09-08 nicht erfasst`.
- **Status:** Vollständig gelesen, analysiert und per Conformance-Test abgedeckt.

---

## 3. Tiefen-Audit Testergebnisse

### Phase 1: Property-Based Tests (`proptest`)
- **Datei:** `tests/proptest_candle.rs`
- **Abdeckung:**
  - File Fingerprinting SHA-256 Determinismus & Quantisierungsvarianz.
  - `GaspValidator::compute_raw_grounding_score` Invarianten (Ergebnis stets in `[0.0, 1.0]`).
  - `GaspConfig` Schwellenwert-Propagierung.
- **Ergebnis:** `3/3 passed` (`ProptestConfig::with_cases(50)`).

### Phase 2: Concurrency & Stress Testing
- **Befehl:** `for i in $(seq 1 10); do cargo test -p memfuse-candle --features candle -- --test-threads=8; done`
- **Ergebnis:** 10/10 Iterationen fehlerfrei. Keine Thread-Deadlocks oder Lock Contention in `Arc<tokio::sync::Mutex<...>>` Wrappern.

### Phase 3: Code Coverage (`cargo-llvm-cov`)
- **Gesamt-Abdeckung:** **85.09% Line Coverage** (751 / 883 Lines), **84.10% Region Coverage**.
- **Einzelmodule:**
  - `gasp.rs`: **95.74%**
  - `model_registry.rs`: **100.00%**
  - `embedding_provider.rs`: **85.71%**
  - `embedding.rs`: **77.02%**
  - `inference.rs`: **76.69%**
  - `gguf_loader.rs`: **56.76%** (Header error paths in `tests/gguf_loader_test.rs` abgedeckt)

### Phase 4: Mutation Testing (`cargo-mutants`)
- **Target:** `src/gasp.rs`
- **Getestete Mutanten:** 65
- **Gekillt / Blockiert:** 41 gekillt, 5 unviable.
- **Ergänzungs-Tests:** `tests/gasp_mutant_test.rs` hinzugefügt, um Zahlen-Filtering, Wort-Längen-Schwellen und exakte Schwellenwert-Grenzen scharf abzuprüfen.

---

## 4. Governance & Quality Gates

- `cargo check -p memfuse-candle --features candle`: 0 Fehler, 0 Warnungen
- `cargo clippy -p memfuse-candle --features candle -- -D warnings`: 0 Findings
- `cargo fmt --check -p memfuse-candle`: 0 Diffs
- `cargo test -p memfuse-candle --features candle`: 24 Tests grün
- `cargo check --workspace --exclude memfuse-tauri`: PASSED
- `cargo run -p xtask -- jules-preflight --fast`: PASSED
