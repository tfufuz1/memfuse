# DECISIONS.md — Architecture Decision Records (ADR)

Dieses Dokument erfasst alle grundlegenden Architekturentscheidungen. Bei Widersprüchen zwischen Code und ADRs ist der Mensch zu konsultieren — kein Agent darf eine dokumentierte Entscheidung eigenmächtig überschreiben.

**Pflicht**: Jede nicht-triviale Architekturentscheidung wird hier dokumentiert, *bevor* mit der Umsetzung begonnen wird (siehe `AGENTS.md §6 Schleife 6`).

---

## ADR-001: LSM-Tree für Persistenz
*   **Datum**: 2026-05-10
*   **Status**: ✅ Final
*   **Entscheidung**: Verwendung einer LSM-Tree-Architektur (Log-Structured Merge-tree) für die lokale Datenhaltung.
*   **Alternativen**: B-Tree, relationale DBs (z. B. SQLite).
*   **Begründung**: Hoher Schreibdurchsatz und Crash-Konsistenz durch sequenzielle WAL-Schreiboperationen und immutable SSTables. Ermöglicht saubere Snapshot-Isolation.

---

## ADR-002: HNSW für Vektor-Indexierung
*   **Datum**: 2026-05-15
*   **Status**: ✅ Final
*   **Entscheidung**: Verwendung des Hierarchical Navigable Small World (HNSW) Graphen für die Vektorsuche.
*   **Alternativen**: IVF-PQ (Quantisierung), Flat Index.
*   **Begründung**: HNSW bietet exzellente Suchpräzision (Recall) und sehr geringe Suchlatenz auf CPU, kombiniert mit SIMD-Befehlssatz-Erkennung.

---

## ADR-003: RRF (Reciprocal Rank Fusion) für Hybridisierung
*   **Datum**: 2026-05-20
*   **Status**: ✅ Final
*   **Entscheidung**: Kombination von HNSW- und BM25-Suche mittels Reciprocal Rank Fusion (RRF).
*   **Alternativen**: Lineare Gewichtung der Scores.
*   **Begründung**: RRF fusioniert Ränge statt roher, nicht normierter Scores (Kosinus-Distanz vs. BM25-Score) und benötigt kein manuelles Parameter-Tuning.

---

## ADR-004: Sovereign Core (Pure Rust Policy)
*   **Datum**: 2026-06-01
*   **Status**: ✅ Final (Refactored)
*   **Entscheidung**: Striktes `#![forbid(unsafe_code)]` in Layer 0-2 (ausgenommen SIMD in `memfuse-index`). Keine C-Bibliotheken im Default-Profil.
*   **Alternativen**: Einbindung von C++ Vektorbibliotheken oder OpenSSL.
*   **Begründung**: Gewährleistet maximale Speichersicherheit, deterministisches Cross-Compiling und unkomplizierten Betrieb in isolierten Systemen.

---

## ADR-005: Feature-Based Scaling
*   **Datum**: 2026-06-15
*   **Status**: ✅ Final
*   **Entscheidung**: Optionale Features (z. B. auto-embedding via ONNX, Raft-basiertes Clustering) werden als Opt-in Features in Layer 3 ausgelagert.
*   **Alternativen**: Feste Verlinkung aller Module.
*   **Begründung**: Verhindert, dass C-Abhängigkeiten (z. B. `ort` für ONNX Runtime) oder komplexe Netzwerkbibliotheken den souveränen Kern belasten.

---

## ADR-006: Eigenständige DECISIONS.md statt inline in SOURCE_OF_TRUTH.md
*   **Datum**: 2026-07-17
*   **Status**: ✅ Final
*   **Entscheidung**: ADRs werden in einer eigenständigen `DECISIONS.md` geführt, nicht mehr inline in `docs/SOURCE_OF_TRUTH.md`.
*   **Alternativen**: Beibehaltung der ADRs in `SOURCE_OF_TRUTH.md` (bisheriges Modell).
*   **Begründung**: LLM-Agenten können `DECISIONS.md` gezielt laden, ohne den gesamten SOT-Ballast (Backlog, Roadmap, Crate-Inventar) in den Kontext aufnehmen zu müssen. Reduziert Tokenverbrauch und erhöht Treffsicherheit. `CONSTITUTION.md` wurde entsprechend aktualisiert.

---

## ADR-007: Produktstrategie — Lokale Agent-Memory-Library (Richtung C)
*   **Datum**: 2026-07-19
*   **Status**: ✅ Final
*   **Entscheidung**: MemFuse wird als **eingebettete 4-Signal-Memory-Engine für lokale AI-Agenten** positioniert — kein Server, kein Docker, kein Cloud-Account. Primäre Vertriebskanäle: `pip install memfuse` (PyPI) und `cargo add memfuse-db` (crates.io). Richtung A (Sovereign Edge-DB) ist der langfristige Erweiterungspfad auf derselben Codebasis, nicht ein separater Pivot.
*   **Alternativen**:
    - (A) Air-Gapped / Sovereign Edge-DB — strategisch wertvoll, aber Enterprise-Vertrieb als Solo-Entwickler aktuell nicht realisierbar.
    - (B) DACH Enterprise-Search (Morphologie-Fokus) — das Morphologie-Merkmal ist zu schmal für ein eigenständiges Produkt, aber wertvoll als Differenzierungsfeature innerhalb von C.
*   **Begründung**: Option C erfordert den geringsten Pivot (80% des Codes existiert bereits), liefert in 4–8 Wochen überprüfbares Feedback (Benchmarks, PyPI-Downloads statt 12+ Monate Enterprise-Verkaufszyklen), und schließt Richtung A nicht aus — im Gegenteil: Zero-C-Deps und ACID-Garantien sind der Vorbereitungsschritt für Sovereign Edge. Die Sovereign-Core-Eigenschaften bleiben vollständig erhalten.
*   **Konsequenzen**:
    - `memfuse-graph` und `memfuse-py` werden in den aktiven Workspace reaktiviert (höchste Priorität).
    - `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent` wurden physisch aus dem Repo entfernt (ausgelagert).
    - README und alle Governance-Dokumente werden auf "eingebettete Agent-Memory-Library" ausgerichtet.

---

## ADR-008: Embedding-Backend — ONNX (memfuse-embed) → Ollama HTTP (memfuse-ollama)
*   **Datum**: 2026-08-22
*   **Status**: ✅ Final (Ersetzt ADR-007 bzgl. lokaler ONNX-Inferenz)
*   **Entscheidung**: Ollama via `memfuse-ollama` als primäres Embedding-Backend. `memfuse-embed` wird vollständig aus Workspace-Dependencies und Features entfernt.
*   **Alternativen**: ONNX In-Process Embeddings (`memfuse-embed`).
*   **Begründung**:
    - Ollama dient im KMU-Desktop-Szenario bereits als LLM-Runtime.
    - Modell-Tausch ohne Code-Änderung (Ollama-Modell-Name konfigurierbar).
    - Apple-Silicon ARM-Optimierung durch Ollama nativ vorhanden.
    - Reduziert C++ Native Build-Komplexität (kein ONNX-Runtime-Vendoring).
*   **Kosten & Konsequenzen**:
    - Höhere Latenz pro Embedding vs. In-Process-ONNX (mitigiert durch parallele Embedding-Batch-Requests in `memfuse-ollama`).
    - Harte Laufzeit-Abhängigkeit von lokalem Ollama-Prozess.
    - `memfuse-ollama` als shared Crate bereitgestellt für `memfuse-tauri`, `memfuse-mcp` und `memfuse-py`.

---

## ADR-009: Crate `memfuse-tauri` als Grundgerüst für Desktop-App ("MemFuse Brain")
*   **Datum**: 2026-07-20
*   **Status**: ✅ Final
*   **Entscheidung**: Anlegen des Crates `crates/memfuse-tauri` als Tauri-Desktop-Applikation ("MemFuse Brain") und Einbindung als Workspace-Mitglied.
*   **Alternativen**: Reine CLI- oder HTTP-Server-Applikation.
*   **Begründung**: Strategische Neuausrichtung hin zu einer benutzerfreundlichen Desktop-Anwendungs-Shell mit GUI und direkter Anbindung an die MemFuse Storage & Graph DB-Kern-Crates.

---

## ADR-010: MCP-Transport — HTTP-REST-Stub → stdio JSON-RPC 2.0
*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: `memfuse-mcp` implementiert den stdio-Transport des Model Context Protocol (MCP Spec v2024-11-05) anstelle eines HTTP-REST-Stubs. Alle JSON-RPC-Nachrichten werden zeilenweise über stdin/stdout ausgetauscht.
*   **Alternativen**: SSE+HTTP-Transport (ebenfalls MCP-konform, aber komplexer für lokale Clients).
*   **Begründung**:
    - Claude Desktop, Cursor und andere MCP-Clients erwarten für lokale Server den stdio-Transport per Definition.
    - stdio ist zero-config (kein Port-Binding, keine Firewall-Regeln, kein TLS).
    - Logging wird auf stderr beschränkt, damit stdout ausschließlich dem Protokoll gehört.
    - axum/tower-Abhängigkeiten aus `memfuse-mcp` entfernt; das Crate verwendet nur tokio-util + futures-util als zusätzliche Dependencies (bereits transitiv im Workspace vorhanden).
*   **Konsequenzen**:
    - `mcp.json` im Repo-Root enthält das `mcpServers`-Format für Claude Desktop.
    - Kein HTTP-Listener mehr — der Server kann nicht via curl/Postman direkt getestet werden; stattdessen via `echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | cargo run --bin memfuse-mcp-server`.

---

## ADR-011: Consolidate Checkpoint Subsystems (CheckpointCoordinator Trait)
*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Einführung des Trait `CheckpointCoordinator` in `memfuse-core::traits` zur Harmonisierung der Checkpoint-Architektur. `PersistentCheckpointStore` (in `memfuse-checkpoint`) implementiert `CheckpointCoordinator`. `Checkpointer`/`CheckpointGuard` in `memfuse-store` verbleiben als interne RAII-Guards für transaktionale WAL-Rollbacks.
*   **Alternativen**: Physische Löschung von `memfuse-checkpoint` und Migration aller Typen in `memfuse-store`.
*   **Begründung**: Klare Rollentrennung: `CheckpointCoordinator` stellt die öffentliche, benannte API für persistenten State bereit (verwendet in `memfuse-db`), während `Checkpointer`/`CheckpointGuard` RAII-Abstraktionen für WAL-Level Rollbacks innerhalb der LSM-Engine sind. Behebt Befund AGT-STORE-002 [DUPLICATION][MAJOR].

---

## ADR-012: Invarianten-Spannungsfeld — std::fs innerhalb spawn_blocking vs. Pure Async-I/O
*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Die Modul-Dokumentation von `memfuse-store/src/lib.rs` behauptet "Alle Disk-I/O via tokio::fs (zero std::fs imports)". Jedoch verwenden `SstableReader` und `SstableBuilder` `std::fs::File` innerhalb von `tokio::task::spawn_blocking`.
*   **Alternativen**:
    - **Option A (Empfohlen)**: Doku und `docs/ARCHITECTURE.md` anpassen zu: *"tokio::fs für alle Metadaten- und Lifecycle-Operationen; std::fs::File ausschließlich innerhalb von spawn_blocking für Performanz-kritische Block-Level Random-Access Reads/Writes."*
    - **Option B**: Code vollständig auf `tokio::fs::File` refactoren (bringt Wrapper-Overhead bei wahlfreien Block-Zugriffen mit sich).
*   **Begründung**: Option A wahrt die maximale Lese-/Schreibperformanz von SSTables auf NVMe-Speichern, ohne Async-Executoren zu blockieren (da `spawn_blocking` dedizierte Worker-Threads nutzt). Option B verringert die Komplexität der Invarianten-Aussage auf Kosten von Latenz.
*   **Eskalation**: Entscheidung erfordert Freigabe durch den Entwickler (ASK-FIRST Tier).

---

## ADR-013: DiskANN als experimentelles Feature (memfuse-index)
*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Die Out-of-Core-Vektorsuche (DiskANN) im `memfuse-index` Crate wird als experimentell markiert und hinter dem Cargo-Feature `experimental-diskann` sowie `#[doc(hidden)]` verborgen. Sie wird (vorerst) nicht in die abstrahierte `VectorIndexBackend`-Schnittstelle des `memfuse-db`-Crates integriert.
*   **Alternativen**:
    - **Option A**: Volle Integration durch Refactoring der `VectorIndex`-Abstraktion und Anpassung der `memfuse-db::Collection`, um dynamisch zwischen HNSW und DiskANN zu wechseln.
*   **Begründung**: `memfuse-db::Collection` und `HnswIndex` sind aktuell extrem eng verzahnt (z.B. direkte Nutzung von `all_doc_ids_from_map()` in der Collection). Eine überhastete Integration würde die Architektur-Integrität und Snapshot-Isolation gefährden, da DiskANN derzeit `insert()` und `delete()` nicht vollständig (oder nur mit `Err`) implementiert. Option A hätte gravierende Umbauten am Kern-Datenfluss der Collection zur Folge gehabt. Das Verbergen von DiskANN schützt die Produktionspfade, lässt aber den Code für zukünftige Entwicklungen im Baum.
*   **Konsequenzen**:
    - `memfuse-db` nutzt HNSW weiterhin hartcodiert.
    - Endnutzer sehen die DiskANN-Funktionalität nicht in der öffentlichen API.

---

---

## ADR-014: Regex-Engine-Wahl & ReDoS-Härtung für `run_regex_transformation`
*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Entscheidung**: `run_regex_transformation` (in `crates/memfuse-tauri/src/commands/transform.rs`) verwendet die `regex`-Crate v1.13.1 (NFA/DFA-basiert, kein Backtracking). Der `spawn_blocking` + `tokio::time::timeout`-Ansatz wird als defensives Sicherheitsnetz beibehalten, nicht als primärer ReDoS-Schutz. Ein `Arc<Semaphore>` in `AppState` begrenzt gleichzeitige Blocking-Thread-Belegungen auf `MAX_CONCURRENT_REGEX_OPS = 8`.
*   **Alternativen**:
    - **Option A (verworfen)**: Kooperativer Abbruch via `Arc<AtomicBool>` + Iterator-Pattern über alle Matches. Nicht nötig, da die `regex`-Crate keine pathologischen Laufzeiten erzeugen kann (NFA garantiert lineare Zeit).
    - **Option B (verworfen)**: Wechsel auf `regex` mit PCRE-Syntax-Erweiterungen (Lookahead, Backreferences). Bricht die Linearitätsgarantie — explizit abgelehnt.
*   **Begründung**:
    - **Engine-Analyse** (Prüfung gegen Cargo.lock): `regex v1.13.1` + `regex-automata v0.4.18` verwenden NFA-basiertes Matching ohne Backtracking. Backreferences und Lookahead werden beim Kompilieren (`Regex::new()`) mit einem harten Fehler abgelehnt. Das klassische ReDoS-Muster `(a+)+$` ist mit dieser Engine **strukturell kein pathologisches Pattern** — das NFA evaluiert es in O(n·|NFA-Zustände|).
    - **Timeout-Funktion**: `REGEX_TIMEOUT = 5 s` dient nicht als ReDoS-Schutz, sondern als Sicherheitsnetz gegen unerwartete Bugs. Bei `MAX_REGEX_INPUT_BYTES = 1 MiB` und einer konservativen Durchsatzschätzung von ~50 MB/s beträgt die reale Worst-Case-Ausführungszeit << 100 ms. Ein Timeout entspricht einem ~250× Puffer — ein Timeout-Ereignis signalisiert daher einen Bug, keine normale Nutzung.
    - **Semaphore-Schutz**: Da Bulk-Transform viele Snippets gleichzeitig verarbeiten kann und `spawn_blocking` dedizierte OS-Threads belegt (tokio-Default-Pool: 512), begrenzt `regex_semaphore` (Permits: 8) die gleichzeitige Blocking-Thread-Belegung durch Regex-Ops. Auch wenn ein hypothetischer Hang auftreten würde, kann nie der gesamte Pool erschöpft werden.
    - **Adaptives Input-Limit**: Normal bewertete Patterns: 1 MiB. Als strukturell komplex bewertete Patterns (>8 Gruppen, >4 Alternationen, >500 Zeichen): 64 KiB. Dies ist kein ReDoS-Schutz, sondern stellt sicher, dass lineares Matching innerhalb des Timeouts bleibt.
*   **Konsequenzen**:
    - `regex = "1"` als workspace dependency in `Cargo.toml` (bereits transitiv vorhanden, keine neuen Downloads).
    - `AppState` enthält `regex_semaphore: Arc<Semaphore>`.
    - Drei Tauri-Commands: `run_regex_transform`, `run_bulk_regex_transform`, `validate_regex_pattern`.
    - Timeout-Ereignisse werden via `tracing::warn!` geloggt (Monitoring-Pflicht gemäß Auftrag §5).


---

## ADR-015: RAII CheckpointGuard Integration & Konsolidierung in `memfuse-checkpoint` (AGT-CKPT-001 / AGT-STORE-002)
*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Das RAII-Guard-Muster für transaktionales Auto-Rollback bei Drop (`CheckpointGuard`) wird aus `memfuse-store::checkpoint` abstrahiert und als generischer Guard `CheckpointGuard<S: StorageEngine>` in `memfuse-checkpoint` (Layer 1) implementiert.
    2. `PersistentCheckpointStore` wird um ein optionales RAII-Guard-Verfahren ergänzt (`begin_guarded_checkpoint(...) -> Result<CheckpointGuard<S>>`), welches `StorageEngine::rollback_to_tx` im `Drop`-Handler ausführt, sofern der Guard nicht vorab via `.commit()` explizit konsumiert wurde.
    3. `memfuse-store::checkpoint::Checkpointer` entfällt als redundantes Duplikat bzw. delegiert fortan intern an `PersistentCheckpointStore<LsmStorage>`.
*   **Alternativen**:
    - **Option A (Entkoppelt lassen)**: Führt zu dauerhafter Code-Duplizierung und zwei verschiedenen Checkpoint-Konzepten (`StateCheckpoint` vs `CheckpointMeta`), was gegen AGT-STORE-002 und AGT-CKPT-001 verstößt.
    - **Option B (Entfernen von CheckpointGuard)**: Entfernt die RAII-Garantie gegen Transaktions-Leaks bei Unhandled Panics oder unvollständigen Operationen.
*   **Begründung**:
    - `memfuse-checkpoint` ist Layer 1 und die in ADR-011 definierte Zielarchitektur für Checkpointing.
    - `CheckpointGuard` hängt funktional nur vom Trait `memfuse_core::StorageEngine` ab (Layer 0), nicht von `LsmStorage` (Layer 1). Daher kann `CheckpointGuard<S: StorageEngine>` ohne DAG-Zyklen sauber in Layer 1 (`memfuse-checkpoint`) beheimatet werden.
    - Die bestehende öffentliche API von `PersistentCheckpointStore` und `CheckpointRegistry` bleibt zu 100% abwärtskompatibel erhalten.
*   **Konsequenzen**:
    - Verlinkung mit `AGT-STORE-002` in `memfuse-store`.
    - Sobald der Entwurf vom Entwickler freigegeben ist, erfolgt die Migration in `memfuse-checkpoint` und `memfuse-store` ohne API-Bruch.

---

## Vorlage für neue ADRs
```markdown
## ADR-NNN: <Titel>
*   **Datum**: YYYY-MM-DD
*   **Status**: 🟡 Proposed / ✅ Final / ❌ Superseded by ADR-XXX
*   **Entscheidung**: <Was wird entschieden?>
*   **Alternativen**: <Welche Alternativen wurden erwogen?>
*   **Begründung**: <Warum genau diese Lösung?>
```
