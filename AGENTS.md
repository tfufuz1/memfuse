# MemFuse — AI-Assistenten-Kontext
## Verifizierter Codestand · HEAD `79677186` · Stand 2026-09-07

> **Für AI-Assistenten:** Diese Datei beschreibt was TATSÄCHLICH implementiert ist,
> nicht was die Spec behauptet. Bei Widerspruch zwischen dieser Datei und Spec/README:
> Diese Datei hat Vorrang (Code-Befund > Spezifikation, §0.1 Quellenhierarchie).

---

## Crate-Topologie (verifiziert aus Cargo.toml & DAG-Analyse)

MemFuse ist in ein Schichten-Modell (Layer 0–6) gegliedert. Sämtliche Workspace-Crates halten sich an einen strikten gerichteten azyklischen Graphen (DAG):

- **Layer 0 — Fundament**:
  - `memfuse-core`: Core types (`TenantId`, `ConfigFingerprint`, etc.), traits, and error handling (`crates/memfuse-core`)
  - `memfuse-calibration`: Calibration scalers (Platt, Isotonic, Replicator) (`crates/memfuse-calibration`)
- **Layer 1 — Storage-Primitiven & Vertikalen**:
  - `memfuse-crypto`: Encryption at rest & cryptographic deletion proofs (`DeletionProof`) (`crates/memfuse-crypto`)
  - `memfuse-checkpoint`: Snapshot & backup management (`crates/memfuse-checkpoint`)
  - `memfuse-graph`: CSR-Graph, `ImmunMemory` (F-04), `PathRAGEngine` (`crates/memfuse-graph`)
  - `memfuse-text`: BM25 full-text search & DACH compound splitting (`crates/memfuse-text`)
  - `memfuse-candle`: Native Candle GGUF ML inference backend (`crates/memfuse-candle`)
- **Layer 2 — Subsysteme**:
  - `memfuse-embed`: Text embeddings & Cross-Encoder reranking (`crates/memfuse-embed`, optional)
  - `memfuse-index`: HNSW vector index, SQ8 quantization, DiskANN (`crates/memfuse-index`)
  - `memfuse-ollama`: Ollama HTTP client & context prefix engine (`crates/memfuse-ollama`)
  - `memfuse-store`: LSM-Tree storage engine & WAL (`crates/memfuse-store`)
- **Layer 3 — Hauptdatenbank**:
  - `memfuse-db`: Embedded hybrid search & collection engine (`crates/memfuse-db`)
- **Layer 4 — Orchestrierung & Frontend**:
  - `memfuse-bench`: Reproducible benchmark harness (`benchmarks/memfuse-bench`)
  - `memfuse-router`: Conformal router engine & SLM profiles (`crates/memfuse-router`)
  - `memfuse-tauri`: Desktop app shell (`crates/memfuse-tauri`)
- **Layer 5 — Agenten-Engine**:
  - `memfuse-agent`: Persistent agent workflow loop (`crates/memfuse-agent`)
- **Layer 6 — Protocol & Sandbox**:
  - `memfuse-mcp`: Model Context Protocol (MCP) stdio JSON-RPC 2.0 server (`crates/memfuse-mcp`)

---

## Was TATSÄCHLICH implementiert ist vs. FEHLT (verifiziert)

### Implementiert ✅

| Komponente / Typ | File:Line Reference | Beschreibung / Anmerkung |
|---|---|---|
| `TenantId` | `crates/memfuse-core/src/types/domain.rs:59` | Mandanten-Identifikator |
| `ConfigFingerprint` | `crates/memfuse-core/src/types/domain.rs:914` | Invalidation-Fingerprint für Kalibrierung & Profile |
| `DeletionProof` | `crates/memfuse-crypto/src/deletion_proof.rs:81` | Kryptographischer Löschnachweis (GDPR Art. 17) |
| `memfuse-calibration` | `crates/memfuse-calibration/` | Scaler (Platt, Isotonic, Replicator) & P8 Compliance |
| `PathRAGEngine` | `crates/memfuse-graph/src/path_rag.rs:35` | Bidirektionale Graph-Retrieval Search Engine |
| `ImmunMemory` (F-04) | `crates/memfuse-graph/src/immune.rs:84` | Immunologische Widerspruchserkennung & Edge-Suppression |
| `memfuse-candle` | `crates/memfuse-candle/` | Workspace-Member (Layer 1), GGUF Inferenz-Backend |
| `ConsolidationSession` | `crates/memfuse-db/src/context_compaction.rs:188` | Context Compaction mit Transaktionssicherheit |
| `FreeEnergyThermostat` (F-01) | `crates/memfuse-db/src/thermostat.rs:44` | Thermodynamisches Adaptive-Decay hinter `physio-features` |
| `SleepCycleEngine` | `crates/memfuse-db/src/sleep_cycle.rs:72` | NREM/REM Konsolidierung und Community-Synthese |

### Fehlt / Nicht integriert ❌

| Komponente / Feature | Status | Befund / Grund |
|---|---|---|
| `memfuse-kv-bridge` | FEHLT | Crate existiert nicht im Repository |
| `EdgeProvenance` | FEHLT | Typ existiert nicht in den Crates (`grep -rn "EdgeProvenance" crates/`) |
| `memfuse-candle` Serving-Anbindung | NICHT VERDRAHTET | Crate existiert als Member, ist aber nicht in `memfuse-db`, `memfuse-router` oder `memfuse-ollama` eingebunden |
### Bewusst entkoppelte Architektur-Komponenten (Keine technische Schuld) 🟢

| Komponente / Feature | Status | Begründung / Dokumentation |
|---|---|---|
| `memfuse-py` Workspace-Isolierung | BEWUSST ISOLIERT | Eigenständiger Workspace in `crates/memfuse-py`, nicht in Root-`Cargo.toml` `members` (ADR-064). Benötigt `panic = "unwind"` im Release-Profil für FFI `catch_unwind()`, während der Haupt-Workspace `panic = "abort"` nutzt. CI deckt den Crate separat ab. |

---

## Bekannte offene Risiken

1. **`rebuild_region()` ohne Recall-Tests (F-02, `crates/memfuse-index/src/hnsw.rs:1812`)**:
   `rebuild_region()` führt reines Tombstone-Pruning durch, ohne dass wissenschaftliche Recall-Tests oder ein offizielles ADR vorliegen. Das Feature-Flag `physio-nucleation` MUSS deaktiviert bleiben, bis entsprechende Regressionstests vorliegen.
2. **Kaskadierende Invalidierung Supersedes→CSR-Kante fehlt (`crates/memfuse-db/src/collection/search.rs:937`)**:
   `search.rs` filtert abgelöste Dokumente nur zur Abfragezeit (Query-Time Filter). Eine aktive Kaskaden-Invalidierung verknüpfter CSR-Graph-Kanten bei Dokument-Superseding fehlt.
3. **`EdgeProvenance`-Typ fehlt**:
   Herkunftsnachweise für Graph-Kanten (`EdgeProvenance`) sind in Spezifikationen erwähnt, jedoch im Codebase noch nicht als Typ implementiert.
4. **`memfuse-candle` nicht in Serving-Pipeline verdrahtet**:
   `memfuse-candle` ist zwar als Workspace-Crate vorhanden, dient aber derzeit als isoliertes Modul und ist noch nicht in die Haupt-Serving-Pipeline (`memfuse-db` / `memfuse-router`) eingebunden.

---

## ⚠️ Frischhaltungspflicht dieser Datei

Diese Datei MUSS bei jedem PR aktualisiert werden, der:
- Eine neue Top-Level-Komponente hinzufügt (neuer Typ in memfuse-core, neues Crate)
- Eine als "Fehlt" markierte Komponente implementiert
- Ein Feature-Flag von non-default auf default umstellt

**Prüfpflicht vor jedem Commit-Merge:** Diff dieser Datei gegen `git log --oneline -20`
gegenprüfen — wurde in den letzten 20 Commits etwas implementiert, das hier noch
als "Fehlt" steht?

Eine veraltete AGENTS.md ist schlimmer als keine — sie führt Agenten aktiv in die Irre.

---

## Non-Obvious Decisions (would cause wrong code without this knowledge)

- **TxId generation**: ALWAYS `collection.allocate_tx()` — NEVER `SystemTime::as_nanos()`
- **fsync errors**: ALWAYS propagate with `?` — NEVER `let _ = dir.sync_all()`
- **unsafe scope**: ONLY in `memfuse-index/src/distance.rs` (SIMD, ADR-017/ADR-034), `memfuse-index/src/diskann.rs` (Mmap, ADR-017) and `memfuse-index/src/persistence.rs` (Mmap, ADR-017). Exception: test-only unsafe in `memfuse-crypto/src/anti_tamper.rs` exclusively for Zeroize drop-semantics verification via raw pointer inspection. Production builds are unsafe-free via `#![cfg_attr(not(test), forbid(unsafe_code))]`.
- **AI-TAG[SMELL][CRITICAL]**: ALWAYS fix immediately — never just comment
- **Document chunking**: ALWAYS use `MarkdownChunker` — NEVER embed entire text as 1 vector
- **MCP transport**: stdio JSON-RPC 2.0 ONLY — axum was removed (ADR-010)
- **WAL HMAC key**: ALWAYS via `load_or_create_integrity_key()` — NEVER hardcoded
- **AI-TAG & ID Schema**: Alle neuen Tags verwenden das hash-basierte Schema `AGT-<CRATE>-<8-hex-hash>` (z.B. `AGT-STORE-a3f29c1d`). Bestehende `AGT-<CRATE>-NNN` IDs haben Bestandsschutz.
- **Tag-Zeitstempel- & Session-Pflicht**: Alle `AI-TAG`, `ANCHOR` und `REVIEW-PASS` Kommentare tragen zwingend sekundengenaue ISO-8601-UTC-Zeitstempel im Format `(TS: YYYY-MM-DDTHH:MM:SSZ)` und das `(SESSION: <8-hex-hash>)` Token (siehe `rules/tag_taxonomy.md`).
- **Trait-Default-Pflichttest**: Für jedes `pub trait` mit einer Default-Methode-Implementierung MUSS im selben PR, der einen neuen Implementor dieses Traits hinzufügt, ein Integrationstest existieren, der beweist, dass die Default-Implementierung NICHT still greift (entweder weil sie explizit überschrieben wurde, oder weil ein Test explizit den Default-Fehlerpfad als erwartetes, dokumentiertes Verhalten prüft). Referenz im Code: `capability_coverage` in `crates/memfuse-core/src/traits.rs` (prüft z.B. `VectorIndex::search_at` & `GraphIndex::traverse_at`).
- **Typ-Dopplungs-Prävention**: Vor Anlegen eines neuen Typs oder Traits: `docs/TYPE_REGISTRY.md` nach ähnlichem Namen/Zweck durchsuchen. Bei Kollision: bestehenden Typ erweitern statt Duplikat anlegen, oder Kollision explizit per ADR begründen.
- **Audit-Finding-Verifikation**: Jeder Finding aus einem extern zugelieferten Audit-Dokument oder Prompt MUSS vor Implementierung am AKTUELLEN Quellcode gegengelesen werden (siehe `.jules/AUDIT_INTAKE_PROTOCOL.md`). Falls der Finding nicht mehr zutrifft (Code bereits geändert, Test existiert bereits, Fix bereits gemerged): Finding im PR-Kommentar/Log explizit als "entkräftet" markieren mit Begründung — NICHT stillschweigend ignorieren und NICHT blind implementieren.
- **Sync-Docs Nix-Fallback**: `just sync-docs` verwendet `nix develop -c` — bei fehlendem Nix direkt `cargo xtask sync-docs` aufrufen. Beide Pfade sind in der justfile mit `||`-Fallback abgesichert.
- **Keine HTTP in memfuse-mcp**: Laut ADR-010 ausschließlich stdio JSON-RPC 2.0. Das GLOSSARY.md definierte dies fälschlicherweise als HTTP/JSON-RPC — die korrekte Definition gilt aus ADR-010 und AGENTS.md, nicht aus dem Glossar (wenn Konflikt).
- **Typ-Existenz vor Anlage prüfen**: `find crates/ -name "*.rs" | xargs grep -l "<TYPNAME>"` und `grep "<TYPNAME>" docs/TYPE_REGISTRY.md` ausführen, bevor ein neuer Typ angelegt wird.
- **ADR-Nummernvergabe**: Vor Vergabe einer neuen ADR-Nummer IMMER `ls docs/decisions/ | grep -oP '(?<=ADR-)\d+' | sort -n | tail -1` live ausführen, NIEMALS eine Nummer aus einem älteren Prompt oder einer älteren Analyse übernehmen (schützt vor Duplikaten durch parallele Sessions, siehe ADR-020, ADR-046).
