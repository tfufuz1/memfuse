# MemFuse Audit-Findings: Jules Fix-Prompts (Round 3)

**Erstellt:** 2026-09-09  
**Quelle:** Systematische Analyse aller `docs/audits/AUDIT_memfuse-*.md` Dokumente  
**Zweck:** Isolierte, parallelisierbare Jules-Prompts zur Behebung aller offenen Audit-Findings  

---

## Übersicht der offenen Findings

| # | Crate | Finding-ID | Severity | Kurzbeschreibung |
|---|---|---|---|---|
| 1 | `memfuse-tauri` | `AGT-TAURI-96c44961` | BLOCKER | Fehlendes `provenance`-Feld in `multi_step_search` |
| 2 | `memfuse-db` | `AGT-DB-8ddf8937` | MAJOR | Ungebundene Variable `text_str` im `reranking`-Feature-Block |
| 3 | `memfuse-db` | `AGT-DB-7c141164` | MAJOR | Konsolidierungstest mit identischen Embeddings |
| 4 | `memfuse-mcp` | `AGT-MCP-98350010` | MAJOR | `clippy::field_reassign_with_default` in `config.rs` |
| 5 | `memfuse-mcp` | — | MINOR | `clippy::unnecessary_lazy_evaluations` in `mcp_test.rs` |
| 6 | `memfuse-embed` | `AGT-EMBED-f07dcaf8` | MAJOR | ONNX Session wird bei jedem Aufruf neu instantiiert |
| 7 | `memfuse-embed` | `AGT-EMBED-62093e61` | MINOR | Unkalibrierte Sigmoid-Scores im Cross-Encoder |
| 8 | `memfuse-ollama` | `AGT-OLLAMA-47e6619b` | MINOR | 2 überlebende Mutanten in `context_prefixer.rs` |
| 9 | `memfuse-ollama` | `AGT-OLLAMA-14c0c140` | MINOR | `score_importance` ohne Konfidenz-Metadaten |
| 10 | `memfuse-tauri` | — | BLOCKER | Kompilierungsfehler durch fehlende Felder |
| 11 | `memfuse-py` | `AGT-PY-d5d2be30` | MAJOR | `panic = "abort"` deaktiviert `catch_unwind` |
| 12 | `memfuse-py` | `AGT-PY-ff475c8e` | MAJOR | `_trigger_panic_for_test` testet nicht die echte Boundary |
| 13 | `memfuse-bench` | `AGT-BENCH-3b6c4f9c` | MINOR | `clippy::vec_init_then_push` in `long_mem_eval.rs` |
| 14 | `memfuse-bench` | `AGT-BENCH-032cfc65` | MINOR | `clippy::unnecessary_filter_map` in `long_mem_eval.rs` |
| 15 | `memfuse-checkpoint` | `AGT-CHECKPOINT-a3ccc9fe` | MAJOR | Race-Condition auf `ORPHAN_REGISTRY` Singleton |

---

## Prompt 1 — `memfuse-tauri`: BLOCKER SearchResultDto Fix

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Desktop Application Architect mit Expertise in Tauri v2, 
IPC-Sicherheit und der MemFuse 4-Signal-Fusion-Architektur.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig — verifiziere HEAD-Commit und Crate-Topologie.
2. Führe `cargo xtask claim --crate memfuse-tauri --issue AGT-TAURI-96c44961` aus.
3. Lies `docs/audits/AUDIT_memfuse-tauri.md` (Finding BUG-TAURI-002, Abschnitt 14).

## Problem (BLOCKER)
In `crates/memfuse-tauri/src/commands/search.rs` fehlt das Feld `provenance` in 
der `SearchResultDto`-Initialisierung innerhalb der Funktion `multi_step_search`. 
Upstream-Commit #1598 hat `SearchResultDto` um `pub provenance: Option<ProvenanceDto>` 
erweitert. `hybrid_search` wurde angepasst, `multi_step_search` jedoch nicht.

**Kompilierungsfehler:** `error[E0063]: missing field 'provenance' in initializer of 
'SearchResultDto'`

## Implementierungsschritte
1. **Datei:** `crates/memfuse-tauri/src/commands/search.rs`
   - Finde die `SearchResultDto`-Konstruktion in `multi_step_search`
   - Füge `provenance: None` als Feld hinzu (Multi-Step-Search hat keinen 
     Provenance-Kontext, daher `None` als sicherer Default)
   - Prüfe, ob weitere `SearchResultDto`-Konstruktionen im selben File existieren 
     und ebenfalls das Feld benötigen

2. **Verifikation:**
   cargo check -p memfuse-tauri --all-features
   cargo clippy -p memfuse-tauri --no-deps -- -D warnings
   cargo test -p memfuse-tauri --all-features

3. **Tag-Resolution:** Markiere `AI-TAG` `AGT-TAURI-96c44961` als `RESOLVED` mit 
   Zeitstempel und Session-Hash.

## Constraints
- NUR Dateien in `crates/memfuse-tauri/` modifizieren
- Keine funktionalen Änderungen an der Search-Logik
- `cargo check --workspace --exclude memfuse-tauri` muss WEITERHIN kompilieren
```

---

## Prompt 2 — `memfuse-db`: Feature-Gate Variable Fix

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Datenbank-Architekt mit Expertise in der MemFuse 
4-Signal-Fusion, Feature-Gates und dem `memfuse-db` Orchestrator-Crate (Layer 2).

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig — verifiziere HEAD-Commit.
2. Führe `cargo xtask claim --crate memfuse-db --issue AGT-DB-8ddf8937` aus.
3. Lies `docs/audits/AUDIT_memfuse-db.md` (Abschnitt 11, Finding AGT-DB-8ddf8937).

## Problem (MAJOR)
In `crates/memfuse-db/src/collection/query_builder.rs` ist die Variable `text_str` 
im `feature="reranking"` Block ungebunden. Das Code-Smell wurde als 
`AI-TAG[SMELL][MAJOR]` dokumentiert.

## Implementierungsschritte
1. **Datei:** `crates/memfuse-db/src/collection/query_builder.rs`
   - Suche den `#[cfg(feature = "reranking")]`-Block
   - Identifiziere die ungebundene Variable `text_str`
   - Binde `text_str` korrekt an den Query-Text (wahrscheinlich aus 
     `self.text` oder dem Builder-State)
   - Stelle sicher, dass der Block mit UND ohne `reranking`-Feature kompiliert

2. **Verifikation:**
   cargo check -p memfuse-db --all-features
   cargo check -p memfuse-db --no-default-features
   cargo clippy -p memfuse-db -- -D warnings
   cargo test -p memfuse-db --all-features

3. **Tag-Resolution:** Markiere `AGT-DB-8ddf8937` als `RESOLVED`.

## Constraints
- NUR `crates/memfuse-db/src/collection/query_builder.rs` modifizieren
- Keine Änderungen an der öffentlichen API
- Bestehende Tests DÜRFEN NICHT brechen
```

---

## Prompt 3 — `memfuse-db`: Konsolidierungstest Embedding-Fix

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Datenbank-Architekt mit Fokus auf Context-Compaction, 
Near-Duplicate-Detection und Community-Synthese im `memfuse-db` Crate.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-db --issue AGT-DB-7c141164` aus.
3. Lies `docs/audits/AUDIT_memfuse-db.md` (Abschnitt 12, Finding AGT-DB-7c141164).

## Problem (MAJOR)
`test_execute_sleep_cycle_with_synthesis_pass` in 
`crates/memfuse-db/tests/consolidation_integration_test.rs` verwendet identische 
Embeddings `[1.0, 0.0, 0.0, 0.0]` für alle 5 Turns. Da Cosine Similarity = 1.0 > 0.99 
(`near_duplicate_cosine_threshold`), markiert der Consolidation Pass 4 von 5 Turns 
als Near-Duplicates und tombstoned sie. In Zyklus 2 verbleibt nur 1 Knoten im Graph, 
Community-Größe ist 1 < 3 (`min_community_size`), wodurch die Synthese-Assertion 
fehlschlägt.

## Implementierungsschritte
1. **Datei:** `crates/memfuse-db/tests/consolidation_integration_test.rs`
   - Ersetze die identischen Embedding-Vektoren durch distinkte, aber kohärente 
     Vektoren, z.B.:
     let emb_a = vec![1.0, 0.0, 0.0, 0.0];
     let emb_b = vec![0.9, 0.436, 0.0, 0.0];
     let emb_c = vec![0.8, 0.0, 0.6, 0.0];
     let emb_d = vec![0.7, 0.3, 0.0, 0.648];
     let emb_e = vec![0.6, 0.0, 0.5, 0.624];
   - Stelle sicher, dass die Vektoren:
     a) Normiert sind (||v|| ≈ 1.0)
     b) Cosine Similarity < 0.99 zueinander haben
     c) Cosine Similarity > 0.5 (kohärent genug für Community-Detection)

2. **Verifikation:**
   cargo test -p memfuse-db --test consolidation_integration_test
   cargo test -p memfuse-db --all-features

3. **Tag-Resolution:** Markiere `AGT-DB-7c141164` als `RESOLVED`.

## Constraints
- NUR `crates/memfuse-db/tests/consolidation_integration_test.rs` modifizieren
- Keine Änderungen am Produktionscode
```

---

## Prompt 4 — `memfuse-mcp`: Clippy Lint Fixes

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Protocol Engineer mit Expertise in stdio JSON-RPC 2.0, 
MCP-Sandbox-Security und dem `memfuse-mcp` Crate (Layer 4).

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-mcp --issue AGT-MCP-98350010` aus.
3. Lies `docs/audits/AUDIT_memfuse-mcp.md` (Abschnitt 15, Findings).

## Probleme
### Problem A (MAJOR): `clippy::field_reassign_with_default` in `config.rs`
In `crates/memfuse-mcp/src/config.rs:209-210` wird ein Struct mit `Default::default()` 
initialisiert und anschließend Felder überschrieben.

### Problem B (MINOR): `clippy::unnecessary_lazy_evaluations` in `mcp_test.rs`
In `crates/memfuse-mcp/tests/mcp_test.rs` werden `ok_or_else` mit String-Literalen 
verwendet, wo `ok_or` ausreicht.

## Implementierungsschritte
1. **Datei A:** `crates/memfuse-mcp/src/config.rs`
   - Finde die Stelle ab Zeile 209 mit `field_reassign_with_default`
   - Refactore zu einer direkten Struct-Konstruktion mit benannten Feldern
   - Alternativ: Verwende einen Builder-Pattern falls vorhanden

2. **Datei B:** `crates/memfuse-mcp/tests/mcp_test.rs`
   - Ersetze `ok_or_else(|| "string literal".to_string())` durch 
     `ok_or("string literal")` (nur bei String-Literalen, nicht bei 
     dynamisch konstruierten Fehlern)

3. **Verifikation:**
   cargo clippy -p memfuse-mcp --no-deps -- -D warnings
   cargo test -p memfuse-mcp --all-features
   cargo fmt --check -p memfuse-mcp

4. **Tag-Resolution:** Markiere `AGT-MCP-98350010` als `RESOLVED`.

## Constraints
- NUR Dateien in `crates/memfuse-mcp/` modifizieren
- Keine funktionalen Änderungen am Protokoll-Verhalten
- ADR-010 (stdio-only, kein HTTP) MUSS weiterhin eingehalten werden
```

---

## Prompt 5 — `memfuse-embed`: ONNX Session Caching (Performance)

```
## Rolle & Kompetenzen
Du bist ein Senior Rust ML-Infrastructure Engineer mit Expertise in ONNX Runtime 
Session-Management, tokio::spawn_blocking und dem `memfuse-embed` Crate.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-embed --issue AGT-EMBED-f07dcaf8` aus.
3. Lies `docs/audits/AUDIT_memfuse-embed.md` (Abschnitt 14.3, PERF Finding).

## Problem (MAJOR — Performance)
In `crates/memfuse-embed/src/lib.rs` instantiiert `TextEmbedder::embed_async` bei 
JEDEM Aufruf eine neue `ort::session::Session` aus der Datei innerhalb von 
`spawn_blocking`. Dies verursacht:
- Unnötigen I/O pro Embedding-Anfrage
- Hohe Latenz durch ONNX Graph-Optimierung bei jeder Session-Erstellung
- Inkonsistenz mit `OnnxReranker`, der die Session korrekt als 
  `parking_lot::Mutex<ort::session::Session>` hält

## Implementierungsschritte
1. **Datei:** `crates/memfuse-embed/src/lib.rs`
   - Refactore `TextEmbedder` zu einer Struktur, die die `ort::session::Session` 
     einmalig beim Erstellen lädt und als `Arc<parking_lot::Mutex<Session>>` hält
   - Modifiziere `embed_async` so, dass es die gecachte Session über 
     `spawn_blocking` + Mutex-Lock verwendet (analog zu `OnnxReranker::rerank`)
   - Behalte den Semaphore-basierten Pool-Schutz (`pool_size`) bei
   - Alle Änderungen MÜSSEN hinter `#[cfg(feature = "onnx")]` stehen

2. **Verifikation:**
   cargo check -p memfuse-embed --no-default-features
   cargo check -p memfuse-embed --all-features
   cargo test -p memfuse-embed --all-features
   cargo clippy -p memfuse-embed --all-features -- -D warnings

3. **Tag-Resolution:** Markiere `AGT-EMBED-f07dcaf8` als `RESOLVED`.

## Constraints
- NUR `crates/memfuse-embed/src/lib.rs` modifizieren
- Default-Build (ohne `onnx`-Feature) DARF NICHT beeinflusst werden
- `#![deny(unsafe_code)]` MUSS eingehalten werden
- Hermetic Feature-Gate Isolation MUSS bestehen bleiben
```

---

## Prompt 6 — `memfuse-py`: Panic-Abort und Test-Boundary Fixes

```
## Rolle & Kompetenzen
Du bist ein Senior Rust FFI-Engineer mit Expertise in PyO3, GIL-Management, 
catch_unwind/panic-Boundary und dem `memfuse-py` Crate.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig — beachte insbesondere §3 "Bewusst entkoppelte 
   Architektur-Komponenten": `memfuse-py` ist bewusst isoliert (ADR-064).
2. Führe den Claim aus.
3. Lies `docs/audits/AUDIT_memfuse-py.md` (Findings AGT-PY-d5d2be30 und 
   AGT-PY-ff475c8e).

## Problem A (MAJOR — Security): `panic = "abort"` deaktiviert catch_unwind
**Finding ID:** `AGT-PY-d5d2be30`  
Der Workspace-Root `Cargo.toml` setzt `panic = "abort"` im `[profile.release]`. 
Dies deaktiviert `std::panic::catch_unwind` in `run_blocking_ffi` bei Release-Builds, 
wodurch Rust-Panics den CPython-Prozess via SIGABRT terminieren statt als 
`PyRuntimeError` aufgefangen zu werden.

## Problem B (MAJOR — Test): Fake-Panic-Test
**Finding ID:** `AGT-PY-ff475c8e`  
`_trigger_panic_for_test` in `crates/memfuse-py/src/lib.rs` gibt direkt 
`PyRuntimeError` zurück statt über `run_blocking_ffi(py, || panic!(...))` einen 
echten Panic auszulösen. Dadurch wird die catch_unwind-Boundary nicht getestet.

## Implementierungsschritte
1. **Datei A:** `crates/memfuse-py/Cargo.toml`
   - Prüfe ob `memfuse-py` eine eigene `Cargo.toml` mit Workspace-Referenz hat
   - Da `memfuse-py` bewusst isoliert ist (ADR-064, eigener Workspace), 
     stelle sicher dass das Release-Profil `panic = "unwind"` gesetzt ist:
     ```toml
     [profile.release]
     panic = "unwind"
     ```
   - ACHTUNG: Die Root-`Cargo.toml` NICHT ändern!

2. **Datei B:** `crates/memfuse-py/src/lib.rs`
   - Finde `_trigger_panic_for_test` (ca. Zeile 1435)
   - Ersetze die direkte `PyRuntimeError`-Rückgabe durch:
     ```rust
     run_blocking_ffi(py, move || {
         panic!("{}", msg);
     })
     ```
   - Stelle sicher, dass die Python-Tests in `tests/test_panic_isolation.py` 
     korrekt `PyRuntimeError` erwarten

3. **Verifikation:**
   cargo check --manifest-path crates/memfuse-py/Cargo.toml --all-features
   cargo test --manifest-path crates/memfuse-py/Cargo.toml --all-features
   cargo clippy --manifest-path crates/memfuse-py/Cargo.toml -- -D warnings

4. **Tag-Resolution:** Markiere `AGT-PY-d5d2be30` und `AGT-PY-ff475c8e` als 
   `RESOLVED`.

## Constraints
- NUR Dateien in `crates/memfuse-py/` modifizieren
- Die Root-`Cargo.toml` NICHT ändern (ADR-064 Workspace-Isolation)
- `#![forbid(unsafe_code)]` MUSS eingehalten werden
```

---

## Prompt 7 — `memfuse-bench`: Clippy Lint Fixes

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Benchmark-Engineer mit Fokus auf Retrieval-Accuracy-Regression 
und dem `memfuse-bench` Crate.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-bench --issue AGT-BENCH-CLIPPY` aus.
3. Lies `docs/audits/AUDIT_memfuse-bench.md` (Abschnitt 2, Findings).

## Probleme
### Problem A (MINOR): `clippy::vec_init_then_push`
**Finding:** `AGT-BENCH-3b6c4f9c` in `long_mem_eval.rs:189`  
`let mut scenarios = Vec::new()` gefolgt von mehreren `.push()`-Aufrufen.

### Problem B (MINOR): `clippy::unnecessary_filter_map`
**Finding:** `AGT-BENCH-032cfc65` in `long_mem_eval.rs:1225`  
`json_val_to_string` nutzt `.filter_map(...)` wo `.map(...)` ausreicht.

## Implementierungsschritte
1. **Datei:** `benchmarks/memfuse-bench/src/long_mem_eval.rs`
   - **Fix A (Zeile ~189):** Ersetze `Vec::new()` + multiple `.push()` durch 
     `vec![scenario1, scenario2, ...]` Makro-Initialisierung
   - **Fix B (Zeile ~1225):** Ersetze `.filter_map(|x| ...)` durch `.map(|x| ...)` 
     in der Funktion `json_val_to_string` (nur wenn die Closure immer `Some` 
     zurückgibt)

2. **Verifikation:**
   cargo clippy -p memfuse-bench -- -D warnings
   cargo test -p memfuse-bench --all-features
   cargo fmt --check -p memfuse-bench

3. **Tag-Resolution:** Markiere `AGT-BENCH-3b6c4f9c` und `AGT-BENCH-032cfc65` 
   als `RESOLVED`.

## Constraints
- NUR `benchmarks/memfuse-bench/src/long_mem_eval.rs` modifizieren
- Keine funktionalen Änderungen an der Benchmark-Logik
- Benchmark-Ergebnisse MÜSSEN identisch bleiben
```

---

## Prompt 8 — `memfuse-checkpoint`: ORPHAN_REGISTRY Race-Condition

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Transaktionssystem-Engineer mit Expertise in RAII-Guards, 
OnceLock-Singleton-Patterns, paralleler Testausführung und dem `memfuse-checkpoint` 
Crate (Layer 1).

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-checkpoint --issue AGT-CHECKPOINT-a3ccc9fe` aus.
3. Lies `docs/audits/AUDIT_memfuse-checkpoint.md` (Abschnitt 8, Finding 
   AGT-CHECKPOINT-a3ccc9fe).

## Problem (MAJOR)
`test_orphan_registry_persists_across_drop` leidet unter einer Race-Condition auf 
dem globalen `ORPHAN_REGISTRY` `OnceLock`-Singleton, wenn `cargo test` mehrere 
Tests parallel ausführt und `PersistentCheckpointStore::new` gleichzeitig 
`recover_and_clean()` aufruft.

Die `InstanceOrphanRegistry` (ADR-053) wurde eingeführt, um pro Instanz zu 
arbeiten, aber der Test greift weiterhin auf den globalen `OnceLock`-Singleton zu.

## Implementierungsschritte
1. **Analysiere** die Architektur:
   - Prüfe ob `ORPHAN_REGISTRY` (globaler `OnceLock`) noch verwendet wird
   - Prüfe den Status der Migration zu `InstanceOrphanRegistry` (ADR-053)
   
2. **Option A — Test-Isolation (bevorzugt):**
   - Markiere `test_orphan_registry_persists_across_drop` mit 
     `#[serial_test::serial]` oder verwende ein Test-spezifisches Mutex
   - Stelle sicher, dass der Test die `InstanceOrphanRegistry` statt des 
     globalen Singletons testet

3. **Option B — Globales Singleton eliminieren:**
   - Falls `ORPHAN_REGISTRY` ausschließlich in Tests verwendet wird, entferne 
     den globalen Singleton und migriere vollständig zu 
     `InstanceOrphanRegistry`
   - Falls Produktionscode den Singleton verwendet, refactore zu per-Instance-
     Scoping

4. **Verifikation:**
   # 10x parallele Ausführung zum Nachweis der Race-Freedom:
   for i in $(seq 1 10); do
     cargo test -p memfuse-checkpoint --all-features -- --test-threads=8 || exit 1
   done
   cargo clippy -p memfuse-checkpoint -- -D warnings

5. **Tag-Resolution:** Markiere `AGT-CHECKPOINT-a3ccc9fe` als `RESOLVED`.

## Constraints
- NUR Dateien in `crates/memfuse-checkpoint/` modifizieren
- `#![forbid(unsafe_code)]` MUSS eingehalten werden
- ADR-011 und ADR-015 Konformität MUSS gewahrt bleiben
- Bestehende 45+ Unit-Tests und 32+ Integrationstests DÜRFEN NICHT brechen
```

---

## Prompt 9 — `memfuse-ollama`: Mutation-Testing Lücken & APM-22

```
## Rolle & Kompetenzen
Du bist ein Senior Rust Security & LLM Integration Lead mit Expertise in 
HTTP-Client-Robustheit, Prompt-Injection-Resistance und dem `memfuse-ollama` 
Crate (Layer 1).

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-ollama --issue AGT-OLLAMA-MUTATIONS` aus.
3. Lies `docs/audits/AUDIT_memfuse-ollama.md` (Abschnitte 14.4 und 14.5).

## Problem A (MINOR): 2 überlebende Mutanten in `context_prefixer.rs`
**Finding:** `AGT-OLLAMA-47e6619b`  
Von 16 getesteten Mutanten in `context_prefixer.rs` überlebten 2. Dies deutet 
auf fehlende Grenzwert-Tests hin.

## Problem B (MINOR): Score-Importance ohne Konfidenz-Metadaten
**Finding:** `AGT-OLLAMA-14c0c140`  
`score_importance` gibt einen punktuellen Float-Wert ohne Konfidenzmaß oder 
Modell-Metadaten zurück (APM-22, APM-24 Risiko).

## Implementierungsschritte
1. **Datei:** `crates/memfuse-ollama/src/context_prefixer.rs`
   - Identifiziere die 2 überlebenden Mutanten (typischerweise 
     Grenzwertprüfungen in `truncate_prefix` oder `truncate_chars`)
   - Füge Unit-Tests hinzu, die diese spezifischen Operatorgrenzen abdecken:
     - Off-by-one in Wort-/Zeichengrenzen
     - Leere Eingaben, Eingaben exakt am Limit

2. **Datei:** `crates/memfuse-ollama/src/importance.rs`
   - Erweitere den Rückgabewert von `score_importance` um ein Struct:
     ```rust
     pub struct ImportanceScore {
         pub score: f32,
         pub confidence: Option<f32>,     // None wenn nicht kalibriert
         pub model_id: Option<String>,
     }
     ```
   - ODER: Dokumentiere die bewusste Entscheidung als ADR-VORSCHLAG im 
     PR-Body, wenn die Änderung architekturrelevant ist (gemäß AGENTS.md §6)

3. **Verifikation:**
   cargo test -p memfuse-ollama --all-features
   cargo clippy -p memfuse-ollama -- -D warnings

4. **Tag-Resolution:** Markiere beide Findings als `RESOLVED`.

## Constraints
- NUR Dateien in `crates/memfuse-ollama/` modifizieren
- XML-Escaping (`xml_escape`) NICHT verändern
- HTTP-Client-Retry-Logik NICHT verändern
```

---

## Prompt 10 — `memfuse-embed`: Platt-Kalibrierung für Cross-Encoder

```
## Rolle & Kompetenzen
Du bist ein Senior Rust ML-Infrastructure Engineer mit Expertise in Score-
Kalibrierung, Platt-Scaling und dem `memfuse-embed` Crate.

## Kontext & Mandatory Bootstrap
1. Lies `AGENTS.md` vollständig.
2. Führe `cargo xtask claim --crate memfuse-embed --issue AGT-EMBED-62093e61` aus.
3. Lies `docs/audits/AUDIT_memfuse-embed.md` (Abschnitt 14.2, APM-22 Finding).

## Problem (MINOR — ML-Scoring)
In `crates/memfuse-embed/src/reranker.rs` werden Raw Cross-Encoder Logits via 
unkalibrierte Sigmoid-Transformation (1 / (1 + e^{-x})) auf [0,1] gemappt. 
Für die Kombination heterogener Model-Backends wird eine Kalibrierung empfohlen 
(Temperature Scaling / Platt Calibration).

**Hinweis:** Laut Abschnitt 15.2 der Audits wurde `PlattScaler` bereits in 
`CrossEncoderReranker` verifiziert. Prüfe den aktuellen Stand!

## Implementierungsschritte
1. **Prüfe zunächst** ob `PlattScaler` bereits in `reranker.rs` integriert ist 
   (Audit 15.2 deutet darauf hin)
2. **Falls NICHT integriert:**
   - Integriere `memfuse_calibration::PlattScaler` in `CrossEncoderReranker`
   - Wende die Kalibrierung NACH dem ONNX-Forward-Pass, VOR der Score-Sortierung an
   - Behalte den Passthrough-Fallback (ohne ONNX) unverändert
3. **Falls BEREITS integriert:**
   - Markiere `AGT-EMBED-62093e61` als `RESOLVED` mit Verweis auf die 
     existierende Implementierung
   - Füge einen dedizierten Test hinzu, der die ECE-Reduktion nach 
     Kalibrierung verifiziert

4. **Verifikation:**
   cargo check -p memfuse-embed --no-default-features
   cargo check -p memfuse-embed --all-features
   cargo test -p memfuse-embed --all-features

## Constraints
- NUR Dateien in `crates/memfuse-embed/` modifizieren
- Passthrough-Fallback (ohne ONNX) MUSS unverändert bleiben
- `#![deny(unsafe_code)]` MUSS eingehalten werden
```

---

## Parallelisierungs-Matrix

Die folgende Matrix zeigt, welche Prompts **sicher parallel** an Jules gesendet 
werden können (keine Dateiüberschneidungen):

| Gruppe | Prompts | Betroffene Crates | Parallel-sicher? |
|--------|---------|-------------------|-----------------|
| **A** | 1 | `memfuse-tauri` | ✅ Unabhängig |
| **B** | 2, 3 | `memfuse-db` (verschiedene Dateien) | ✅ Parallel zueinander |
| **C** | 4 | `memfuse-mcp` | ✅ Unabhängig |
| **D** | 5 | `memfuse-embed` (`lib.rs`) | ⚠️ Sequenziell mit 10 |
| **E** | 6 | `memfuse-py` | ✅ Unabhängig |
| **F** | 7 | `memfuse-bench` | ✅ Unabhängig |
| **G** | 8 | `memfuse-checkpoint` | ✅ Unabhängig |
| **H** | 9 | `memfuse-ollama` | ✅ Unabhängig |
| **I** | 10 | `memfuse-embed` (`reranker.rs`) | ⚠️ Sequenziell mit 5 |

### Empfohlene Ausführungsreihenfolge

**Welle 1 (8 parallele Sessions):**
> Prompts **1, 2, 3, 4, 6, 7, 8, 9** — Alle betreffen unterschiedliche Crates

**Welle 2 (2 sequenzielle Sessions):**
> Prompt **5** (memfuse-embed/lib.rs), danach Prompt **10** (memfuse-embed/reranker.rs)  
> *Alternativ:* Da die Prompts verschiedene Dateien betreffen (`lib.rs` vs. `reranker.rs`), 
> können sie potentiell auch parallel ausgeführt werden, Merge-Konfliktrisiko ist gering.
