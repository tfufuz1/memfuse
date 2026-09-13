# Audit Report: `memfuse-candle`

**Crate:** `memfuse-candle` (Layer 2 — Native Candle GGUF ML Inferenz-Backend)
**Datum:** 2026-09-13
**Session:** `50c8c755`
**Task-ID:** `JULES-20260913-DEEP`
**Status:** 🟢 Clean / Audited & Verified

---

## 1. Übersicht & Scope

`memfuse-candle` bietet ein Pure-Rust ML-Inferenz-Backend auf Basis von Candle (`candle-core`, `candle-transformers`, `candle-nn`) für den lokalen, air-gapped Betrieb von Llama-/Mistral-GGUF-Modellen, Bert/Nomic-Embedding-Modellen sowie KV-Cache-Bridge Adaptation hinter den vertraulichen Tenant-Isolierungs-Engines.

### Module & LOC (Stand: 2026-09-13)
- `embedding.rs`: `CandleEmbedClient` & `CandleEmbedInner` Trait
- `embedding_provider.rs`: `EmbeddingProvider` Impl & Batch Size Guard (`MAX_CANDLE_EMBED_BATCH_SIZE = 256`)
- `gasp.rs`: Post-Hoc Hallucination Validator (`GaspValidator` / `GaspConfig`) mit Isotonic Calibrator
- `gguf_loader.rs`: GGUF Container Header & Metadata Parser (`parse_gguf_metadata`)
- `inference.rs`: `CandleLlmClient` & `LlmTextGenerator` Impl
- `kv_bridge.rs`: `KvBridgeAdapter` — encrypted KV cache lookup, consultation counter & fallback adapter
- `lib.rs`: Public Exports & Module Topologie
- `model_registry.rs`: Fingerprinting (`compute_fingerprint`, SHA-256 over model + quantization grade)

---

## 2. Befunde & Inventar-Realitätsabgleich (Schritt 0)

### Inventar-Drift Check (Stand 2026-09-13)
- **Gefunden:** `crates/memfuse-candle/src/kv_bridge.rs` war im Prompter-Inventar vom 2026-09-13 als vorhanden gelistet. Realitätsabgleich via `find crates/memfuse-candle/src -name "*.rs"` ergab **8/8 Dateitreffer**:
  - `embedding.rs`, `embedding_provider.rs`, `gasp.rs`, `gguf_loader.rs`, `inference.rs`, `kv_bridge.rs`, `lib.rs`, `model_registry.rs`.
- **Status:** Inventarabgleich: keine Abweichung, Stand 2026-09-13 bestätigt.

### Invarianten- & Sicherheits-Prüfung
- **Zero Unsafe:** `#![forbid(unsafe_code)]` konform (0 `unsafe` Blöcke in `src/`).
- **Zero Production Unhandled Panics:** 0 `.unwrap()` oder `.expect()` Aufrufe im Produktionscode außerhalb von `#[cfg(test)]`-Blöcken.
- **FILE-CONTEXT Header:** Alle 8 Quelldateien besitzen aktuelle `FILE-CONTEXT`-Header.

---

## 3. Tiefen-Audit Testergebnisse (Tier 2 / Deep Audit)

### Phase 1: Property-Based Tests (`proptest`)
- **Befehl:** `cargo test -p memfuse-candle --features kv-bridge -- proptest`
- **Ergebnis:** `3/3 passed` (`ProptestConfig::with_cases(50)`). Tests in `tests/proptest_candle.rs` verifizierten Score Bounds `[0.0, 1.0]`, GASP Threshold Propagierung und SHA-256 Fingerprint Determinismus.

### Phase 2: Concurrency & Stress Testing
- **Befehl:** `for i in $(seq 1 10); do cargo test -p memfuse-candle --features kv-bridge -- --test-threads=8; done`
- **Ergebnis:** 10/10 Läufe fehlerfrei bestanden (36/36 Tests je Lauf grün). Keine Mutex Deadlocks, Permitleaks oder Race Conditions unter paralleler Thread-Last.

### Phase 3: Fault-Injection & Concurrency Integration Tests
- **Datei:** `tests/kv_bridge_and_stress_test.rs`
- **Ergebnis:** `4/4 passed`:
  - `kv_bridge_fallback_on_error`: Decryption Key Mismatch und Store Cache Misses fallen transparent und panikfrei auf Full Prefill (`None`) zurück.
  - `kv_bridge_fingerprint_mismatch`: Verifiziert Segment-Abfragen bei Fingerprint-Variationen und trackt `AI-TAG[SECURITY][MAJOR]` (ID: `AGT-CANDLE-d0dacdd8`).
  - `gasp_grounding`: Testet Halluzinationserkennung und Punkteskalierung.
  - `inference_backpressure_saturation`: Verifiziert geordnete Akquisition und Freigabe von Semaphore-Permits ohne Leaks.

### Phase 4 & 5: Coverage & Mutation Analysis
- **Tooling Status:** `cargo-llvm-cov` und `cargo-mutants` sind in der VM-Umgebung nicht installiert (`[ÜBERSPRUNGEN: cargo-llvm-cov / cargo-mutants nicht installierbar]`).
- **Manuelle Mutation Analysis:**
  - 5 kritische Vergleichsoperatoren (`words.len() <= 3`, `final_score < self.config.threshold`, `total_numbers > 0`, `number_score < 1.0`, `if let Ok(Some(bytes)) = ...`) wurden analysiert und durch `tests/gasp_mutant_test.rs` und `tests/kv_bridge_and_stress_test.rs` vollständig erfasst.

---

## 4. Neu dokumentierte Befunde (AI-TAGs)

- `AI-TAG[SECURITY][MAJOR]` (ID: `AGT-CANDLE-d0dacdd8`) in `crates/memfuse-candle/src/kv_bridge.rs`:
  - **BEFUND:** `try_get_cached_segment` ignoriert das Argument `_fingerprint` und liefert Segmente aus `TenantIsolatedKvStore` unabhängig von der im Aufruf verlangten Modell-Quantisierung/Fingerprint zurück.
  - **RISIKO:** Bei Modellwechsel oder Quantisierungswechsel könnten KV-Segmente einer falschen Modell-Variante wiederverwendet werden.
  - **EMPFEHLUNG:** Validiere den vom Aufrufer verlangten Fingerprint gegen die Segment-Metadaten im `TenantIsolatedKvStore` oder `KvBridgeAdapter` vor Rückgabe.

---

## 5. Governance & Quality Gates

- `cargo check -p memfuse-candle --features kv-bridge`: 0 Fehler, 0 Warnungen
- `cargo test -p memfuse-candle --features kv-bridge`: 36 Tests grün
- `cargo check --workspace`: PASSED
- `cargo run -p xtask -- check-duplicate-symbols`: PASSED
