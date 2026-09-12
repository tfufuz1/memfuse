# MemFuse — AI-Assistenten-Kontext

<!-- Anker-Index (für §N-Referenzen in anderen Dokumenten) -->
<!-- §1 = Verifizierter Codestand -->
<!-- §2 = Crate-Topologie (verifiziert aus Cargo.toml & DAG-Analyse) -->
<!-- §3 = Was TATSÄCHLICH implementiert ist vs. FEHLT (verifiziert) -->
<!-- §4 = Bekannte offene Risiken -->
<!-- §5 = ⚠️ Frischhaltungspflicht dieser Datei -->
<!-- §6 = Entwicklungsprozess: Analyse- und Implementierungsstufe -->
<!-- §7 = Non-Obvious Decisions (would cause wrong code without this knowledge) -->

<a id="1"></a>
## Verifizierter Codestand · HEAD `caad7178`, 2026-09-12

> **Für AI-Assistenten:** Diese Datei beschreibt was TATSÄCHLICH implementiert ist,
> nicht was die Spec behauptet. Bei Widerspruch zwischen dieser Datei und Spec/README:
> Diese Datei hat Vorrang (Code-Befund > Spezifikation, §0.1 Quellenhierarchie).

---

<a id="2"></a>
## Crate-Topologie (verifiziert aus Cargo.toml & DAG-Analyse)

MemFuse ist in ein Schichten-Modell (Layer 0–7) gegliedert. Sämtliche Workspace-Crates (17 Crates im Hauptworkspace + `memfuse-py` als isoliertes Workspace) halten sich an einen strikten gerichteten azyklischen Graphen (DAG):

- **Layer 0 — Fundament**:
  - `memfuse-core`: Core types (`TenantId`, `ConfigFingerprint`, etc.), traits, and error handling (`crates/memfuse-core`)
- **Layer 1 — Storage-Primitiven & Vertikalen**:
  - `memfuse-calibration`: Calibration scalers (Platt, Isotonic, Replicator) (`crates/memfuse-calibration`)
  - `memfuse-checkpoint`: Snapshot & backup management (`crates/memfuse-checkpoint`)
  - `memfuse-graph`: CSR-Graph, `ConsistencyEnforcer` (F-04), `PathRAGEngine`, `EdgeProvenance` (`crates/memfuse-graph`)
  - `memfuse-security`: Encryption at Rest, `DeletionProof`, and KV-Cache Security (`crates/memfuse-crypto`, Package Name: `memfuse-security`; `memfuse-kv-bridge` ist in `crates/memfuse-crypto/src/kv_segment/` konsolidiert)
  - `memfuse-text`: BM25 full-text search & DACH compound splitting (`crates/memfuse-text`)
- **Layer 2 — Subsysteme**:
  - `memfuse-candle`: Native Candle GGUF ML inference backend (`crates/memfuse-candle`)
  - `memfuse-index`: HNSW vector index, SQ8 quantization, DiskANN (`crates/memfuse-index`)
  - `memfuse-ollama`: Ollama HTTP client & context prefix engine (`crates/memfuse-ollama`)
  - `memfuse-store`: LSM-Tree storage engine & WAL (`crates/memfuse-store`)
- **Layer 3 — Embeddings & Reranking**:
  - `memfuse-embed`: Text embeddings & Cross-Encoder reranking (`crates/memfuse-embed`, optional)
- **Layer 4 — Hauptdatenbank**:
  - `memfuse-db`: Embedded hybrid search & collection engine (`crates/memfuse-db`)
- **Layer 5 — Benchmarking, Routing & Desktop-Shell**:
  - `memfuse-bench`: Reproducible benchmark harness (`benchmarks/memfuse-bench`)
  - `memfuse-router`: Conformal router engine & SLM profiles (`crates/memfuse-router`)
  - `memfuse-tauri`: Deprecated/Entfernt (Produktfokus auf PyPI Library & MCP Server, ADR-077)
- **Layer 6 — Agenten-Engine**:
  - `memfuse-agent`: Persistent agent workflow loop (`crates/memfuse-agent`)
- **Layer 7 — Protocol & Sandbox**:
  - `memfuse-mcp`: Model Context Protocol (MCP) stdio JSON-RPC 2.0 server & `uvx`-paketierte Distribution (`crates/memfuse-mcp`)

---

<a id="3"></a>
## Was TATSÄCHLICH implementiert ist vs. FEHLT (verifiziert)

### Implementiert ✅

| Komponente / Typ | File:Line Reference | Beschreibung / Anmerkung |
|---|---|---|
| `TenantId` | `crates/memfuse-core/src/types/domain.rs:59` | Mandanten-Identifikator |
| `ConfigFingerprint` | `crates/memfuse-core/src/types/domain.rs:914` | Invalidation-Fingerprint für Kalibrierung & Profile |
| `DeletionProof` & `LayerCleanupProof` (D1) | `crates/memfuse-crypto/src/deletion_proof.rs:81` | Kryptographischer Löschnachweis mit typsystemischer Absicherung (PR #1921, #1926) |
| `memfuse-security` | `crates/memfuse-crypto/` | Package-Name `memfuse-security` in Cargo.toml. Encryption-at-Rest & konsolidierte KV-Cache Security (`crates/memfuse-crypto/src/kv_segment/`) |
| `EdgeProvenance` | `crates/memfuse-graph/src/provenance.rs:11` | Herkunftsnachweis für Graph-Kanten (`INV-GRAPH-PROV-1`, `DocEdgeIndex`) |
| `Kaskadierende CSR-Invalidierung` | `crates/memfuse-db/src/collection/crud.rs:958` | Kaskadierendes Tombstoning verknüpfter Graph-Kanten bei Dokument-Superseding via `DocEdgeIndex` |
| `memfuse-calibration` | `crates/memfuse-calibration/` | Scaler (Platt, Isotonic, Replicator) & P8 Compliance |
| `PathRAGEngine` | `crates/memfuse-graph/src/path_rag.rs:35` | Bidirektionale Graph-Retrieval Search Engine |
| `ConsistencyEnforcer` (F-04) | `crates/memfuse-graph/src/consistency_enforcement.rs:86` | Widerspruchserkennung & Edge-Suppression (ADR-069) |
| `memfuse-candle` | `crates/memfuse-candle/` | Workspace-Member (Layer 2), GGUF Inferenz-Backend |
| `ConsolidationSession` | `crates/memfuse-db/src/context_compaction.rs:188` | Context Compaction mit Transaktionssicherheit |
| `AdaptiveDecayController` (F-01) | `crates/memfuse-db/src/decay_controller.rs:64` | Thermodynamisches Adaptive-Decay hinter `adaptive-decay-control` / `adaptive-decay` (ADR-069) |
| `ConsolidationEngine` | `crates/memfuse-db/src/consolidation_executor.rs:107` | Hintergrund-Konsolidierung und Community-Synthese (`execute_sleep_cycle`) |
| `MarkdownChunker` | `crates/memfuse-db/src/chunker.rs` | Strukturiertes Dokumentsplitting vor Vektor-Embedding |
| `MultiStepEngine` & `total_cmp` (H) | `crates/memfuse-db/src/multistep.rs` & `fusion.rs` | Iterative Search Engine mit RRF-Signal-Fusion & robuster HeapEntry-Sortierung (PR #1925) |
| `scan_bounded` (F) | `crates/memfuse-core`, `memfuse-store`, `memfuse-db` | Speicherbeschränkter Range-Scan zur OOM-Vermeidung (PR #1927) |
| `WAL Header State Atomicity` | `crates/memfuse-store/src/wal.rs` | Atomare Schreibzustandsverfolgung im WAL Header (PR #1924) |
| `DiskANN HMAC Hardening` | `crates/memfuse-index/src/diskann.rs` | HMAC-Integritätsschutz für DiskANN-Indizes (PR #1919) |
| `CheckpointGuard` | `crates/memfuse-checkpoint/src/lib.rs` | RAII-Checkpoint & Persistent Store Management |
| `CrossEncoderReranker` | `crates/memfuse-embed/src/reranker.rs` | Cross-Encoder Reranking für High-Precision Retrieval (ONNX ist Default-Embedding-Backend) |
| `Inference Semaphore & Backpressure` (H-5) | `crates/memfuse-candle/src/inference.rs` | Bounded Inferenz-Concurrency / Backpressure für Candle ML Engine geschlossen |
| `Dimension Default 768` (H-8) | `crates/memfuse-py/` & `memfuse-core` | Dimension-Default auf 768 vereinheitlicht geschlossen |
| `memfuse_consolidate Tool` (H-13) | `crates/memfuse-mcp/src/lib.rs` | Fünftes MCP-Tool für sofortigen Konsolidierungstrigger geschlossen |
| `McpSandbox` | `crates/memfuse-mcp/src/lib.rs` | Read-Only MCP-Server Sandbox & Write Authorization Guard |
| `ContextPrefixEngine` | `crates/memfuse-ollama/src/context_prefixer.rs` | Context Prefix Compression Engine |
| `CSRGraph` & PPR | `crates/memfuse-graph/src/csr.rs` | Compressed Sparse Row Graph mit Personalized PageRank |
| `PersistentAgentWorkflow` | `crates/memfuse-agent/src/lib.rs` | Multi-Step Agent Execution Loop mit State Graph & Checkpointing |

### Fehlt / Nicht integriert ❌

| Komponente / Feature | Status | Befund / Grund |
|---|---|---|
| `memfuse-candle` Serving-Anbindung | NICHT VERDRAHTET | Crate existiert als Member, ist aber nicht in `memfuse-db`, `memfuse-router` oder `memfuse-ollama` eingebunden |
### Bewusst entkoppelte Architektur-Komponenten (Keine technische Schuld) 🟢

| Komponente / Feature | Status | Begründung / Dokumentation |
|---|---|---|
| `memfuse-py` Workspace-Isolierung | BEWUSST ISOLIERT | Eigenständiger Workspace in `crates/memfuse-py`, nicht in Root-`Cargo.toml` `members` (ADR-064). Benötigt `panic = "unwind"` im Release-Profil für FFI `catch_unwind()`, während der Haupt-Workspace `panic = "abort"` nutzt. CI deckt den Crate separat ab. |

---

<a id="4"></a>
## Bekannte offene Risiken

1. **`rebuild_region()` ohne Recall-Tests (F-02, `crates/memfuse-index/src/hnsw.rs:1812`)**:
   `rebuild_region()` führt reines Tombstone-Pruning durch, ohne dass wissenschaftliche Recall-Tests oder ein offizielles ADR vorliegen. Das Feature-Flag `partial-rebuild-pruning` MUSS deaktiviert bleiben, bis entsprechende Regressionstests vorliegen.
2. **`memfuse-candle` nicht in Serving-Pipeline verdrahtet**:
   `memfuse-candle` ist zwar als Workspace-Crate vorhanden, dient aber derzeit als isoliertes Modul und ist noch nicht in die Haupt-Serving-Pipeline (`memfuse-db` / `memfuse-router`) eingebunden.

---

<a id="5"></a>
## ⚠️ Frischhaltungspflicht dieser Datei

Diese Datei MUSS bei jedem PR aktualisiert werden, der:
- Eine neue Top-Level-Komponente hinzufügt (neuer Typ in memfuse-core, neues Crate)
- Eine als "Fehlt" markierte Komponente implementiert
- Ein Feature-Flag von non-default auf default umstellt

**Prüfpflicht vor jedem Commit-Merge:** Diff dieser Datei gegen `git log --oneline -20`
gegenprüfen — wurde in den letzten 20 Commits etwas implementiert, das hier noch
als "Fehlt" steht?

Eine veraltete AGENTS.md ist schlimmer als keine — sie führt Agenten aktiv in die Irre.

**Dokumentations-Governance & HEAD-Zitierung:**
Jedes Dokument, das einen Repository-Zustand als 'verifiziert' beschreibt, MUSS den HEAD-Commit-Hash im Format `HEAD <vollständiger-hash>, <ISO-Datum> <Uhrzeit mit Zeitzone>` exakt aus `git log -1 --format='%H %ci'` übernehmen — kein manuelles Abtippen von Kurz-Hashes.

---

<a id="6"></a>
## Entwicklungsprozess: Analyse- und Implementierungsstufe

### Stufe 1 — Analyse (Claude)
Claude liest Repository-Stand, analysiert Architektur und ADRs, identifiziert
Konflikte und trifft Entscheidungen. Claude schreibt KEINE Code-Änderungen.
Output: präzise Jules-Prompt-Spezifikation mit Dateiliste, Schritten, Constraints.

### Stufe 2 — Implementierung (Jules)
Jules empfängt den Prompt, führt Mandatory Bootstrap aus, setzt Claim,
implementiert genau die spezifizierten Änderungen, keine darüber hinaus.

### Regel: ADR-Erstellung durch Jules
Jules erstellt KEINEN neuen ADR eigenständig, wenn die Entscheidung
architekturrelevant ist (neue Dependency, DAG-Layer-Änderung, Feature-Scope-Änderung).
Stattdessen: Draft-Notiz im PR-Body mit Prefix "ADR-VORSCHLAG:" hinterlassen
und auf menschliche Freigabe warten (gemäß §5 ASK-Grenzen).
Jules DARF ADRs für rein technische Umsetzungsentscheidungen (Typ, Signatur,
Impl-Detail) schreiben, wenn keine Alternativen offen sind.

### Claim-Pflicht vor Arbeitsbeginn
Jede Session MUSS vor erstem Schreibzugriff einen Claim setzen:
  `cargo xtask claim --crate <ZIEL-CRATE> --issue <TASK-ID>`
Bei Claim-Konflikt: STOP — warten oder koordinieren, nicht überschreiben.

---

<a id="7"></a>
## Non-Obvious Decisions (would cause wrong code without this knowledge)

- **TxId generation**: ALWAYS `collection.allocate_tx()` — NEVER `SystemTime::as_nanos()`
- **fsync errors**: ALWAYS propagate with `?` — NEVER `let _ = dir.sync_all()`
- **unsafe scope**: EXCLUSIVELY in five production modules + test-only verification:
  - `memfuse-index/src/distance.rs` (SIMD hardware optimizations: AVX2, AVX-512, NEON; ADR-017/ADR-034)
  - `memfuse-index/src/diskann.rs` (Read-only memory-mapped index I/O: Mmap; ADR-017)
  - `memfuse-index/src/persistence.rs` (Read-only memory-mapped index persistence: Mmap; ADR-017)
  - `memfuse-store/src/wal.rs` (Win32 DACL/ACL file permission enforcement; `#[cfg(windows)]`)
  - `memfuse-db/src/volatile_vault.rs` (RAM buffer memory locking against OS swapping: `mlock`/`munlock`; feature-gated `volatile-vault`)
  - Exception: Test-only unsafe in `memfuse-crypto/src/anti_tamper.rs` (and `kv_segment/segment.rs` unit tests) exclusively for Zeroize drop-semantics verification via raw pointer inspection.
  All other crates strictly enforce `#![forbid(unsafe_code)]` or `#![deny(unsafe_code)]` with inline rationale.
- **AI-TAG[SMELL][CRITICAL]**: ALWAYS fix immediately — never just comment
- **Document chunking**: ALWAYS use `MarkdownChunker` — NEVER embed entire text as 1 vector
- **MCP transport**: stdio JSON-RPC 2.0 ONLY — axum was removed (ADR-010)
- **WAL HMAC key**: ALWAYS via `load_or_create_integrity_key()` — NEVER hardcoded
- **AI-TAG & ID Schema**: Alle neuen Tags verwenden das hash-basierte Schema `AGT-<CRATE>-<8-hex-hash>` (z.B. `AGT-STORE-a3f29c1d`). Bestehende `AGT-<CRATE>-NNN` IDs haben Bestandsschutz.
- **Tag-Zeitstempel- & Session-Pflicht**: Alle `AI-TAG`, `ANCHOR` und `REVIEW-PASS` Kommentare tragen zwingend sekundengenaue ISO-8601-UTC-Zeitstempel im Format `(TS: YYYY-MM-DDTHH:MM:SSZ)` und das `(SESSION: <8-hex-hash>)` Token (siehe `rules/tag_taxonomy.md`).
- **Trait-Default-Pflichttest**: Für jedes `pub trait` mit einer Default-Methode-Implementierung MUSS im selben PR, der einen neuen Implementor dieses Traits hinzufügt, ein Integrationstest existieren, der beweist, dass die Default-Implementierung NICHT still greift (entweder weil sie explizit überschrieben wurde, oder weil ein Test explizit den Default-Fehlerpfad als erwartetes, dokumentiertes Verhalten prüft). Referenz im Code: `capability_coverage` in `crates/memfuse-core/src/traits.rs` (prüft z.B. `VectorIndex::search_at` & `GraphIndex::traverse_at`). <!-- doc-ref-ignore -->
- **Typ-Dopplungs-Prävention**: Vor Anlegen eines neuen Typs oder Traits: `docs/TYPE_REGISTRY.md` nach ähnlichem Namen/Zweck durchsuchen. Bei Kollision: bestehenden Typ erweitern statt Duplikat anlegen, oder Kollision explizit per ADR begründen. Das CI-Gate `check-duplicate-symbols` läuft standardmäßig dateiintern (schnell, immer aktiv). Bei P0-Symbol-Neuanlagen wird die crate-weite Prüfung per `cargo xtask check-duplicate-symbols --cross-module` empfohlen.
- **Audit-Finding-Verifikation**: Jeder Finding aus einem extern zugelieferten Audit-Dokument oder Prompt MUSS vor Implementierung am AKTUELLEN Quellcode gegengelesen werden (siehe `.jules/AUDIT_INTAKE_PROTOCOL.md`). Falls der Finding nicht mehr zutrifft (Code bereits geändert, Test existiert bereits, Fix bereits gemerged): Finding im PR-Kommentar/Log explizit als "entkräftet" markieren mit Begründung — NICHT stillschweigend ignorieren und NICHT blind implementieren.
- **Sync-Docs Nix-Fallback**: `just sync-docs` verwendet `nix develop -c` — bei fehlendem Nix direkt `cargo xtask sync-docs` aufrufen. Beide Pfade sind in der justfile mit `||`-Fallback abgesichert.
- **Keine HTTP in memfuse-mcp**: Laut ADR-010 ausschließlich stdio JSON-RPC 2.0. Das GLOSSARY.md definierte dies fälschlicherweise als HTTP/JSON-RPC — die korrekte Definition gilt aus ADR-010 und AGENTS.md, nicht aus dem Glossar (wenn Konflikt). <!-- doc-ref-ignore -->
- **Typ-Existenz vor Anlage prüfen**: `find crates/ -name "*.rs" | xargs grep -l "<TYPNAME>"` und `grep "<TYPNAME>" docs/TYPE_REGISTRY.md` ausführen, bevor ein neuer Typ angelegt wird.
- **ADR-Nummernvergabe**: Vor Vergabe einer neuen ADR-Nummer IMMER `ls docs/decisions/ | grep -oP '(?<=ADR-)\d+' | sort -n | tail -1` live ausführen, NIEMALS eine Nummer aus einem älteren Prompt oder einer älteren Analyse übernehmen (schützt vor Duplikaten durch parallele Sessions, siehe ADR-020, ADR-046).
- **Namenskonventionen & Standard-Terminologie**: Gemäß [ADR-069](docs/decisions/ADR-069-standard-terminologie-norm.md) sind biologische Metaphern und Anbieter-Branding in Typnamen, Feature-Flags (`physio-*`) und Architektur-Labels untersagt; neue Features und Muster folgen verbindlich der MemFuse-Standard-Terminologie ("MemFuse [Funktion] Pattern"). <!-- doc-ref-ignore -->
- **TOMBSTONE_BIT-Disziplin (ADR-041)**: Bit 63 strikt maskieren (`seq & !TOMBSTONE_BIT`) vor `max_seq` Vergleichen.
- **SSTable Flush-Sichtbarkeit (ADR-043)**: `last_committed_tx` vor `sstables.push()` in `LsmStorage::flush` aktualisieren.
- **MCP Write-Authorization & Sandbox Policy (ADR-044)**: DB-Schreibzugriffe im MCP Server sind standardmäßig GESPERRT (Read-Only Policy).
- **Entkopplung memfuse-router und memfuse-mcp (ADR-045)**: JSON-RPC Typen liegen in `memfuse-core::ipc`, `memfuse-router` ist hängtfrei von `memfuse-mcp`.
