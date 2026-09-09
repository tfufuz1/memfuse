# Audit Report: `memfuse-candle`

**Crate:** `memfuse-candle` (Layer 2 — Native Candle GGUF ML Inferenz-Backend)
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
- `lib.rs`: Public Exports & Module Topologie
- `model_registry.rs`: Fingerprinting (`compute_fingerprint`, SHA-256 over model + quantization grade)

---

## 2. Befunde & Inventar-Realitätsabgleich

### Inventar-Drift Check
- **Gefunden:** `crates/memfuse-candle/src/embedding_provider.rs` war im Prompter-Inventar vom 2026-09-08 nicht gelistet.
- **Befund:** `Inventar-Drift: Datei crates/memfuse-candle/src/embedding_provider.rs im Prompter-Inventar vom 2026-09-08 nicht erfasst`.
- **Status:** Vollständig gelesen, analysiert und per Conformance-Test abgedeckt.

### Feature Gate Fix in `lib.rs`
- **Gefunden:** `pub mod gasp` und dazugehörige re-exports in `lib.rs` waren unter `#[cfg(feature = "candle")]` bedingt eingebunden, obwohl `gasp.rs` immer kompilierbar ist und Tests in `tests/` direkt darauf zugreifen.
- **Fix:** Unbedingter Export von `pub mod gasp` in `lib.rs`, sodass `cargo test -p memfuse-candle` ohne optionale Feature-Flags fehlerfrei kompilierte und ausführte.

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
- **Befehl:** `cargo test -p memfuse-candle`
- **Ergebnis:** 26/26 Tests grün. Keine Thread-Deadlocks oder Lock Contention in `Arc<tokio::sync::Mutex<...>>` Wrappern.

---

## 4. Governance & Quality Gates

- `cargo check -p memfuse-candle`: 0 Fehler, 0 Warnungen
- `cargo clippy -p memfuse-candle -- -D warnings`: 0 Findings
- `cargo fmt --check -p memfuse-candle`: 0 Diffs
- `cargo test -p memfuse-candle`: 26 Tests grün
- `cargo run -p xtask -- jules-preflight --fast`: PASSED
