# Architecture Decision Records (ADR)

> **Kanonische Einzel-Quelle:** Gemäß ADR-060 ist `DECISIONS.md` die einzige maßgebliche
> Quelle für Architecture Decision Records im MemFuse-Projekt. Neue Entscheidungen werden
> ausschließlich append-only am Ende dieser Datei ergänzt (`cargo xtask generate-adr "<Titel>"`).

## Dokumentierte Lücken & Umnummerierungen

* ADR-057: Lücken-Dokumentation (Umnummerierung / Ausgelassen im Zuge paralleler Audit-Sessions)
* ADR-067: Umnummeriert zu ADR-074 (Normative Kalibrierung des PathRAG Sufficiency-Gate Thresholds)
* ADR-068: Umnummeriert zu ADR-076 (Studie zur DiskANN PENDING_FLUSH_THRESHOLD Write-Amplification)

---

# ADR-001: LSM-Tree für Persistenz

*   **Datum**: 2026-05-10
*   **Status**: ✅ Final
*   **Entscheidung**: Verwendung einer LSM-Tree-Architektur (Log-Structured Merge-tree) für die lokale Datenhaltung.
*   **Alternativen**: B-Tree, relationale DBs (z. B. SQLite).
*   **Begründung**: Hoher Schreibdurchsatz und Crash-Konsistenz durch sequenzielle WAL-Schreiboperationen und immutable SSTables. Ermöglicht saubere Snapshot-Isolation.

---

---

# ADR-002: HNSW für Vektor-Indexierung

*   **Datum**: 2026-05-15
*   **Status**: ✅ Final
*   **Entscheidung**: Verwendung des Hierarchical Navigable Small World (HNSW) Graphen für die Vektorsuche.
*   **Alternativen**: IVF-PQ (Quantisierung), Flat Index.
*   **Begründung**: HNSW bietet exzellente Suchpräzision (Recall) und sehr geringe Suchlatenz auf CPU, kombiniert mit SIMD-Befehlssatz-Erkennung.

---

---

# ADR-003: RRF (Reciprocal Rank Fusion) für Hybridisierung

*   **Datum**: 2026-05-20
*   **Status**: ✅ Final
*   **Entscheidung**: Kombination von HNSW- und BM25-Suche mittels Reciprocal Rank Fusion (RRF).
*   **Alternativen**: Lineare Gewichtung der Scores.
*   **Begründung**: RRF fusioniert Ränge statt roher, nicht normierter Scores (Kosinus-Distanz vs. BM25-Score) und benötigt kein manuelles Parameter-Tuning.

---

---

# ADR-004: Sovereign Core (Pure Rust Policy)

*   **Datum**: 2026-06-01
*   **Status**: ✅ Final (Refactored)
*   **Entscheidung**: Striktes `#![forbid(unsafe_code)]` in Layer 0-2 (ausgenommen SIMD in `memfuse-index`). Keine C-Bibliotheken im Default-Profil.
*   **Alternativen**: Einbindung von C++ Vektorbibliotheken oder OpenSSL.
*   **Begründung**: Gewährleistet maximale Speichersicherheit, deterministisches Cross-Compiling und unkomplizierten Betrieb in isolierten Systemen.

---

---

# ADR-005: Feature-Based Scaling

*   **Datum**: 2026-06-15
*   **Status**: ✅ Final
*   **Entscheidung**: Optionale Features (z. B. auto-embedding via ONNX, Raft-basiertes Clustering) werden als Opt-in Features in Layer 3 ausgelagert.
*   **Alternativen**: Feste Verlinkung aller Module.
*   **Begründung**: Verhindert, dass C-Abhängigkeiten (z. B. `ort` für ONNX Runtime) oder komplexe Netzwerkbibliotheken den souveränen Kern belasten.

---

---

# ADR-006: Eigenständige DECISIONS.md statt inline in SOURCE_OF_TRUTH.md

*   **Datum**: 2026-07-17
*   **Status**: ✅ Final
*   **Entscheidung**: ADRs werden in einer eigenständigen `DECISIONS.md` geführt, nicht mehr inline in `docs/SOURCE_OF_TRUTH.md`.
*   **Alternativen**: Beibehaltung der ADRs in `SOURCE_OF_TRUTH.md` (bisheriges Modell).
*   **Begründung**: LLM-Agenten können `DECISIONS.md` gezielt laden, ohne den gesamten SOT-Ballast (Backlog, Roadmap, Crate-Inventar) in den Kontext aufnehmen zu müssen. Reduziert Tokenverbrauch und erhöht Treffsicherheit. `CONSTITUTION.md` wurde entsprechend aktualisiert.

---

---

# ADR-007: Produktstrategie — Lokale Agent-Memory-Library (Richtung C) [TEILWEISE ERSETZT durch ADR-018 bzgl. Vertriebskanal-Priorisierung, 2026-08-24]

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

---

# ADR-008: Embedding-Backend — ONNX (memfuse-embed) → Ollama HTTP (memfuse-ollama)

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

---

# ADR-009: Crate `memfuse-tauri` als Grundgerüst für Desktop-App ("MemFuse Brain")

*   **Datum**: 2026-07-20
*   **Status**: ✅ Final
*   **Entscheidung**: Anlegen des Crates `crates/memfuse-tauri` als Tauri-Desktop-Applikation ("MemFuse Brain") und Einbindung als Workspace-Mitglied.
*   **Alternativen**: Reine CLI- oder HTTP-Server-Applikation.
*   **Begründung**: Strategische Neuausrichtung hin zu einer benutzerfreundlichen Desktop-Anwendungs-Shell mit GUI und direkter Anbindung an die MemFuse Storage & Graph DB-Kern-Crates.

---

---

# ADR-010: MCP-Transport — HTTP-REST-Stub → stdio JSON-RPC 2.0

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

---

# ADR-011: Consolidate Checkpoint Subsystems (CheckpointCoordinator Trait)

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Einführung des Trait `CheckpointCoordinator` in `memfuse-core::traits` zur Harmonisierung der Checkpoint-Architektur. `PersistentCheckpointStore` (in `memfuse-checkpoint`) implementiert `CheckpointCoordinator`. `Checkpointer`/`CheckpointGuard` in `memfuse-store` verbleiben als interne RAII-Guards für transaktionale WAL-Rollbacks.
*   **Alternativen**: Physische Löschung von `memfuse-checkpoint` und Migration aller Typen in `memfuse-store`.
*   **Begründung**: Klare Rollentrennung: `CheckpointCoordinator` stellt die öffentliche, benannte API für persistenten State bereit (verwendet in `memfuse-db`), während `Checkpointer`/`CheckpointGuard` RAII-Abstraktionen für WAL-Level Rollbacks innerhalb der LSM-Engine sind. Behebt Befund AGT-STORE-002 [DUPLICATION][MAJOR].

---

---

# ADR-012: Invarianten-Spannungsfeld — std::fs innerhalb spawn_blocking vs. Pure Async-I/O

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Die Modul-Dokumentation von `memfuse-store/src/lib.rs` behauptet "Alle Disk-I/O via tokio::fs (zero std::fs imports)". Jedoch verwenden `SstableReader` und `SstableBuilder` `std::fs::File` innerhalb von `tokio::task::spawn_blocking`.
*   **Alternativen**:
    - **Option A (Empfohlen)**: Doku und `docs/ARCHITECTURE.md` anpassen zu: *"tokio::fs für alle Metadaten- und Lifecycle-Operationen; std::fs::File ausschließlich innerhalb von spawn_blocking für Performanz-kritische Block-Level Random-Access Reads/Writes."*
    - **Option B**: Code vollständig auf `tokio::fs::File` refactoren (bringt Wrapper-Overhead bei wahlfreien Block-Zugriffen mit sich).
*   **Begründung**: Option A wahrt die maximale Lese-/Schreibperformanz von SSTables auf NVMe-Speichern, ohne Async-Executoren zu blockieren (da `spawn_blocking` dedizierte Worker-Threads nutzt). Option B verringert die Komplexität der Invarianten-Aussage auf Kosten von Latenz.
*   **Eskalation**: Entscheidung erfordert Freigabe durch den Entwickler (ASK-FIRST Tier).

---

---

# ADR-013: DiskANN als experimentelles Feature (memfuse-index)

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

# ADR-014: Regex-Engine-Wahl & ReDoS-Härtung für `run_regex_transformation`

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

---

# ADR-015: RAII CheckpointGuard Integration & Konsolidierung in `memfuse-checkpoint` (AGT-CKPT-001 / AGT-STORE-002)

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

---

# ADR-016: DocId 64-Bit BLAKE3-Trunkierung und Kollisionsschutz (BEFUND AGT-CORE-002)

*   **Datum**: 2026-08-25
*   **Status**: ✅ Final
*   **Entscheidung**: `DocId::from_key()` behält den 64-Bit-u64-Wrapper (BLAKE3 8-Byte Trunkierung) zur Kompatibilität mit HNSW- / Index-Knoten-IDs bei. In Layer 2 (`Collection::insert_op` / `Collection::update_op`) wird vor Indexierungs- / Schreiboperationen eine Kollisionsprüfung über den `doc_key` (Metadaten-Reverse-Lookup) durchgeführt. Im Falle einer Kollision für zwei unterschiedliche Quellschlüssel wird ein expliziter Fehler `MemFuseError::Internal("DocId-Kollision erkannt für Schlüssel '{id}' — bitte Support kontaktieren")` zurückgegeben (Fail-Safe).
*   **Alternativen**:
    - **Option A**: Umstellung von `DocId` auf 128 Bit / 256 Bit UUID/Hash. Verworfen, da dies alle Vektor-Index-Anbindungen (HNSW-Knoten-IDs) und Speicherstrukturen grundlegend verändern würde.
    - **Option B (Bisheriger Status - verworfen)**: Stilles Überschreiben im Kollisionsfall (Fail-Silent). Verworfen, da dies zu inkonsistenter Datenkorruption zwischen Vektorsuche und Direktzugriff führt.
*   **Begründung**: Die Kombination aus deterministischer 64-Bit Hash-Ableitung und expliziter Kollisionsprüfung auf Orchestrationsebene wahrt die Effizienz von u64-DocIds im Index und verhindert absolut jegliche stille Datenkorruption (Zero-Silent-Corruption-Doktrin). Bei einer theoretischen Kollision schlägt der Einfügeversuch laut und kontrolliert fehl.
*   **Konsequenzen**:
    - `Collection::insert_op()` und `Collection::update_op()` verifizieren existierende `doc_key`-Metadaten.
    - Dokumentation in `DocId::from_key()` und Regressionstests dokumentieren und verifizieren dieses Fail-Safe-Verhalten.

---

---

# ADR-017: Explicit Authorization of `unsafe` Mmap in DiskANN (BEFUND AGT-AUDIT-002)

*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Entscheidung**: Die generelle Architekturregel ("`unsafe` ist ausschließlich in `memfuse-index/src/distance.rs` erlaubt") wird für `memfuse-index/src/diskann.rs` und `memfuse-index/src/persistence.rs` erweitert. Ein expliziter `unsafe { Mmap::map(...) }`-Aufruf ist dort zulässig, MUSS aber zwingend durch einen `// SAFETY:`-Kommentar begründet sein, der die Validität des File-Deskriptors und der Längenprüfung belegt. Modulweite `#![allow(unsafe_code)]`-Attribute bleiben strengstens verboten.
*   **Alternativen**:
    - **Option A**: Refactoring auf sichere I/O-Methoden (z. B. pread) ohne Mmap. Verworfen, da DiskANN (Out-of-Core) für maximale Lese-Performance und Memory-Sharing zwingend auf direktes Memory-Mapping großer Vektor-Graphen angewiesen ist. Die Latenzeinbußen wären inakzeptabel.
*   **Begründung**: Mmap ist ein inhärent unsafer OS-Call, aber für High-Performance Vektor-Indizes unabdingbar. Die explizite Ausnahme legitimiert die Nutzung transparent und erzwingt gleichzeitig die Einhaltung lokaler `// SAFETY:`-Beweise, statt die generelle Code-Hygiene durch `#![allow(unsafe_code)]` auszuhebeln.

---

---

# ADR-018: Doppelstrategie — PyPI-Library UND Desktop-App (Auflösung ADR-007/ADR-009-Konflikt)


*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Kontext**: ADR-007 (2026-07-19) erklärt PyPI als primären Vertriebskanal und verwirft Desktop-App. ADR-009 (2026-07-20, einen Tag später) beschloss den Aufbau von memfuse-tauri. Heute ist memfuse-tauri das größte Feature-Investment. Kein ADR hat ADR-007 formal revidiert — beide galten gleichzeitig als "final".
*   **Entscheidung**: MemFuse verfolgt eine bewusste Doppelstrategie:
    - **Kanal 1 — Desktop-App** (memfuse-tauri / "MemFuse Brain"): Zielgruppe DACH-Unternehmensanwender, nicht-technische Nutzer. Positionierung als lokaler, air-gapped Unternehmensassistent. Aktiv in Entwicklung, primäres UI-Investment.
    - **Kanal 2 — Library** (memfuse-py / memfuse-core): Zielgruppe Python-KI-Entwickler, Rust-Entwickler. Technisch fertig (maturin-Build, mcp-Dependencies), noch nicht in README dokumentiert. Nächster Schritt: `pip install`-Anleitung in README ergänzen.
*   **Alternativen**: Einer der beiden Kanäle wird aufgegeben. Verworfen — beide adressieren komplementäre Zielgruppen ohne Kannibalisierung.
*   **Begründung**: Die Desktop-App erreicht nicht-technische Nutzer über GUI-First-Erfahrung. Die Library erreicht KI-Entwickler über programmatische Integration. Beide teilen denselben Kern (memfuse-db, Layer 0–2). Die bisherige Inkohärenz lag nicht an der Strategie, sondern am fehlenden ADR der die Koexistenz formal legitimiert.
*   **Ersetzt**: ADR-007 bzgl. Vertriebskanal-Priorisierung (nicht bzgl. technischer Entscheidungen wie Zero-C-Deps, kein Docker).
*   **Ergänzt**: ADR-009 (Desktop-App-Grundstein).
*   **Konsequenzen**:
    - README-Aktualisierung (`pip install`-Anleitung) ist priorisierte Tech-Debt.
    - Bis dahin: memfuse-tauri als primäres User-facing Produkt behandeln.

---

---

# ADR-019: Contextual Retrieval via `combined_text_owned()`


*   **Datum**: 2026-08-25
*   **Status**: ✅ Final
*   **Kontext**: Anthropic Contextual Retrieval erfordert ein LLM-generiertes Dokument-Kontextpräfix vor der BM25- und Embedding-Indexierung von Chunks, um Vector & BM25-Verluste bei isolierten Text-Passagen zu verhindern.
*   **Entscheidung**:
    - `ContextChunk` in `memfuse-core` wird um das optionale Feld `contextual_prefix: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) erweitert.
    - Das Präfix wird NICHT im Originalinhalt des Chunks persistent überschrieben, sondern bei Bedarf synthetisiert und über `combined_text_owned()` ("prefix\n\ncontent") bereitgestellt.
    - `OllamaClient` in `memfuse-ollama` wird um `ContextPrefixer` erweitert, welcher das Prompt-Caching-Muster durch Wiederverwendung des gekürzten `whole_doc`-Kontexts nutzt.
*   **Alternativen**:
    - **Option A**: Erstellung eines separaten `ContextualDocumentChunk`-Typs außerhalb von `ContextChunk`. Verworfen, um Typ-Explosion und Inkonsistenzen in bestehenden Pipeline-Ketten zu vermeiden.
    - **Option B**: Festes Mutieren von `content` mit vorangestelltem Präfix. Verworfen, da Nutzer beim Retrieval den unveränderten Originaltext zurückerhalten sollen.
*   **Begründung**: Die Erweiterung von `ContextChunk` wahrt die Abwärtskompatibilität (Serde `#[serde(default)]`) und trennt die Speicherung des Originalinhalts von den indexierten Signalrepräsentationen.

---

---

# ADR-020: Cognitive Operating System als Produktvision


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Der strategische Forschungsbericht 2026-08-26 zeigt:
    Der Wettbewerb (Mem0 ECAI-2025, Zep/Graphiti, MemOS) hat sich zu
    kognitiven Gedächtnisarchitekturen entwickelt. MemFuse als reiner
    "4-Signal RAG-Engine" ist 2026/2027 nicht SOTA.
*   **Entscheidung**: MemFuse positioniert sich als **Cognitive Operating
    System für LLM-Agenten**. Das bedeutet:
    - Explizite Differenzierung von Gedächtnistypen (Episodic/Semantic/
      Procedural/Working) als Roadmap-Ziel ab Phase 2
    - Temporale Wissensgraphen (bi-temporal) als Phase-2-Feature
    - Memory Consolidation als Phase-3-Feature
    Die 4-Signal-Architektur bleibt erhalten und ist die korrekte Basis.
    Der neue Begriff "Cognitive OS" beschreibt das Ziel-Endprodukt.
*   **Alternativen**:
    - Beibehaltung "4-Signal Memory Engine" — zu eng, kein Alleinstellungsmerkmal
    - Pivot auf Cloud-Service — widerspricht Sovereign-Core-Doktrin (ADR-004)
*   **Begründung**: Die Forschungslandschaft 2025/2026 (Generative Agents,
    Mem0, MIRIX, A-MEM, Trajectory-Informed Memory) zeigt: passive
    Speichersysteme verlieren gegen aktiv selbstorganisierende Gedächtnis-
    Architekturen. Der strategische Hebel ist Qualität und Kognitivität
    der Memory-Layer, nicht mehr nur Retrieval-Geschwindigkeit.
*   **Konsequenzen**:
    - README, SOURCE_OF_TRUTH, ARCHITECTURE werden auf "Cognitive OS"
      umformuliert (nicht nur "Memory Engine")
    - docs/memfuse_strategic_roadmap.md wird auf 4-Phasen-Plan aktualisiert <!-- doc-ref-ignore -->
    - Phase-2-Features (Gedächtnistypen, temporaler Graph) als ADR-geplant

---

---

# ADR-021: Multi-Signal RAG-Pipeline (Contextual → RRF → Reranking)


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Die RAG-Sprints (RAG-01 bis RAG-05) haben die Ingestion-
    und Retrieval-Pipeline mit mehreren Schichten erweitert. Diese
    Entscheidung kodifiziert die Gesamtarchitektur.
*   **Entscheidung**: MemFuse implementiert eine mehrstufige RAG-Pipeline:
    1. **Contextual Ingestion**: ContextPrefixEngine (memfuse-ollama)
       generiert 50–100 Token LLM-Präfixe vor BM25/HNSW-Indexierung
    2. **4-Signal Indexierung**: HNSW + Contextual-BM25 + CSR-Graph +
       Metadaten parallel indexiert
    3. **Hybrid Retrieval via RRF**: Alle Signale über reciprocal_rank_fusion()
       fusioniert (memfuse-db/fusion.rs) <!-- doc-ref-ignore -->
    4. **Multi-Step Expansion**: MultiStepEngine (memfuse-db/multistep.rs) <!-- doc-ref-ignore -->
       führt bis zu 3 iterative Retrieval-Schleifen aus
    5. **Cross-Encoder Reranking**: CrossEncoderReranker (memfuse-embed,
       --features onnx) reordnet Top-K Kandidaten (optionaler Schritt)
    6. **Context Compaction**: ContextCompactor (memfuse-db/compaction.rs) <!-- doc-ref-ignore -->
       ersetzt alte Tool-Outputs durch StatusToken
*   **Alternativen**: Jeder Schritt einzeln opt-in — zu komplex für Nutzer
*   **Begründung**: Empirisch (Anthropic, 2024): Contextual Embeddings →
    35% weniger Fehler; + Contextual BM25 → 49%; + Cross-Encoder → 67%.
    Die gestaffelte Pipeline ist additiv und gracefully degradierend
    (jede Stufe funktioniert ohne die nächste).
*   **Konsequenzen**:
    - BUG-03 (Audit 2026-08-27): combined_token_count() statt token_count()
      in ContextCompactor — Fix-Prompt existiert in docs/Audit-Reports/
    - BUG-02: parking_lot::Mutex statt std::sync::Mutex im Reranker
    - Alle Pipeline-Stufen sind optional und rückwärtskompatibel

---

---

# ADR-022: Dokumenten-Entduplizierung & Single Responsibility Protocol


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Bisher trugen `AGENTS.md`, `docs/SOURCE_OF_TRUTH.md`, `docs/ARCHITECTURE.md` und `WORKING_STATE.md` teilweise identische Fakten (Crate-Listen, Layer-DAG, Sprint-Historien) redundant und manuell gepflegt vor. Dies führte zu Drift-Risiken.
*   **Entscheidung**:
    - Strikte Trennung der Dokumentenzuständigkeiten gemäß "Dokumenten-Landkarte":
      - `AGENTS.md`: Verbindliche Verhaltensregeln (manuell, stabil).
      - `docs/ARCHITECTURE.md`: Technische Ist-Architektur (DAG, Layer, Crate-Zweck — **auto-generiert** via `xtask sync-docs`).
      - `docs/SOURCE_OF_TRUTH.md`: Produktstrategie, Roadmap, Entscheidungskontext (WARUM — manuell + auto-generierte Crate-Inventartabelle).
      - `WORKING_STATE.md`: Nur Session-zu-Session-Handoff (aktueller Zustand, offene Tags — auto-generiert + minimaler manueller Zusatz).
      - `docs/CHANGELOG.md`: Historische Sprint-Tabelle (aus `WORKING_STATE.md` ausgelagert).
      - `DECISIONS.md`: Chronologisches ADR-Log (manuell).
    - Konsistenzprüfung `cargo run -p xtask -- check-consistency` schlägt fehl, wenn manuell genannte Zahlen (z. B. Crate-Anzahl in `AGENTS.md` oder `README.md`) von der tatsächlichen `Cargo.toml`-Workspace-Topologie abweichen.
*   **Alternativen**: Weiterhin manuelle Redundanzen in mehreren Dateien pflegen. Verworfen wegen hohem Wartungsaufwand und Inkonsistenzgefahr.
*   **Begründung**: Single Responsibility Prinzip für Dokumentation stellt sicher, dass Fakten nur an genau einem Ort gepflegt oder automatisch generiert werden.
*   **Konsequenzen**:
    - `xtask` wird um `check-consistency` und CLI-Flag `--check` für `sync-docs` erweitert.
    - Gate 8 in `context-gates.yml` schützt gegen manuelle Inhaltsabweichungen und Drift.

---

---

# ADR-023: Kompensierende Transaktion für Multi-Store relate() Operations (F-01 / AGT-DB-005)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: `Collection::relate()` führt Operationen über heterogene Storage-Backends (`LsmStorage` und `CsrGraph`) aus. Nachdem `storage.commit(tx)` aufgerufen wurde, ist der `TxBuffer`-Eintrag für `tx` geleert und im WAL dauerhaft persistiert. Ein nachfolgender Fehler in `graph_index.commit(tx)` führte dazu, dass `rollback_relate(tx)` aufgerufen wurde, was wiederum `storage.rollback(tx)` aufrief. Da `storage.rollback(tx)` jedoch nur uncommittete `TxBuffer`-Einträge verwirft (`tx_buffer.discard(tx)`), war der Rollback für den Storage-Teil ein wirkungsloser No-Op. Dies führte zu inkonsistentem Zustand zwischen Storage und Graph-Index.
*   **Entscheidung**: Implementierung von Option A: Kompensierende Transaktion. Falls `storage.commit(tx)` erfolgreich ist, aber `graph_index.commit(tx)` fehlschlägt, wird eine kompensierende Löschtransaktion (`storage.delete()` + `storage.commit()`) mit einer neu allokierten `TxId` ausgeführt, um den bereits committeten Relations-Key wieder aus dem LSM-Storage zu entfernen (Tombstone-Eintrag schreiben).
*   **Alternativen**:
    - **Option B (2-Phase Commit Protocol)**: Einführung einer `prepare()`-Methode auf `GraphIndex`. Verworfen, da dies Trait-Verträge in `memfuse-core` und allen Implementierungen anpassen müsste und höhere API-Komplexität mit sich bringt.
    - **Option C (Vereinheitlichung der Commit-Klammer)**: `CsrGraph` und `LsmStorage` in eine gemeinsame Transaktionsklammer verschmelzen. Verworfen, da `CsrGraph` in-memory eigene CSR-Strukturen und Delta-Buffer verwaltet und eine Zusammenlegung die Layer-Architektur aufbrechen würde.
*   **Begründung**: Option A benötigt keine breaking API-Änderungen an den Trait-Schnittstellen (`memfuse-core`), hat vernachlässigbaren Performance-Overhead im Fehlerfall und ist vollständig konsistent mit bestehenden Tombstone- und Kompensationsmustern im Repo (wie `DbTransaction::commit()` in `transaction.rs`).
*   **Konsequenzen**:
    - `Collection::relate()` führt bei Fehlschlag von `graph_index.commit(tx)` nach erfolgreichem `storage.commit(tx)` einen kompensierenden Delete-Commit aus.
    - Doc-Kommentare in `LsmStorage` und `StorageEngine` beschreiben die exakte Garantie: `rollback()` verwirft nur uncommittete `TxBuffer`-Einträge; ein Undo nach physischem Commit erfordert einen Compensating-Write.

---

---

# ADR-024: Snapshot-Isolation auf Storage- und Text-Signale beschränkt (Vektor/Graph nicht snapshot-isoliert)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Das Trait-Design in `memfuse-core::traits` definiert snapshot-isolierte Methoden `search_at` (`VectorIndex`, `TextIndex`, `StorageEngine`) und `traverse_at` (`GraphIndex`). Eine Quellcode-Analyse ergab, dass `scan_prefix_at` (`LsmStorage`) und `search_at` (`InvertedIndex`) voll snapshot-isoliert implementiert sind. `HnswIndex::search_at`, `DiskAnnIndex::search_at` und `CsrGraph::traverse_at` sind aktuell nicht überschrieben und liefern standardmäßig `Err(MemFuseError::PolicyViolation(...))` zurück. `Collection::hybrid_search()` verwendet für Vektor- und Graph-Signale die aktuellen in-memory Suchmethoden `search()` und `traverse()`, während Storage-Dokumenthydration und Textsuche über `snapshot_seq()` isoliert werden.
*   **Entscheidung**:
    - Es wird explizit dokumentiert, dass Snapshot-Isolation in MemFuse aktuell auf Storage- (LSM-Tree) und Text-Signale (BM25) beschränkt ist. Vektorsuche (`HnswIndex`, `DiskAnnIndex`) und Graph-Traversal (`CsrGraph`) operieren auf dem jeweils aktuellen In-Memory-Zustand.
    - Die Default-Fehlermeldungen in `VectorIndex::search_at` und `GraphIndex::traverse_at` werden präzisiert, um transparent auf ADR-024 zu verweisen: `"Snapshot isolation for vector/graph search is not yet implemented — tracked in ADR-024"`.
    - Sobald Snapshot-Isolation für In-Memory Vektor- und Graph-Strukturen implementiert wird, werden `HnswIndex::search_at`, `DiskAnnIndex::search_at` und `CsrGraph::traverse_at` überschrieben und in `Collection::hybrid_search()` angebunden.
*   **Alternativen**:
    - **Option A (Feature erzwingen)**: Sofortiges Re-Engineering von `HnswIndex` und `CsrGraph` zur vollständigen Node/Edge-Versionierung pro Sequence-Number. Verworfen wegen hohem Risiko komplexer Regressionen in den Kern-Traversierungs-Performanzpfaden ohne vorheriges Design-Review.
    - **Option B (Fail-silent belassen)**: Unveränderte Beibehaltung generischer Trait-Fehlermeldungen ohne Dokumentation. Verworfen, da dies das `CONSTITUTION.md`-Prinzip "No Silent Failures" und "Ehrliche Invarianten" verletzt.
*   **Begründung**: Option B bzw. Klärung via ADR-024 stellt sicher, dass Entwickler und Nutzer exakt wissen, welche Signale snapshot-isoliert sind (Storage + Text) und welche auf dem aktuellen In-Memory-Stand arbeiten (Vektor + Graph), ohne falsche API-Versprechungen zu machen.
*   **Konsequenzen**:
    - Aktualisierung der Invariantentabelle in `docs/ARCHITECTURE.md`.
    - Aktualisierung der Trait-Default-Fehlermeldungen in `crates/memfuse-core/src/traits.rs`. <!-- doc-ref-ignore -->
    - Hinzufügen expliziter Integrationstests, die das dokumentierte Verhalten absichern.

---
---

---

# ADR-025: Memory Importance Score & Recency-Decay als Post-Processing-Filter (Erweiterung ADR-021 & ADR-024)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Roadmap Phase 2 fordert ein LLM-bewertetes Memory Importance Scoring (`ImportanceScore`) und eine Recency-Decay-Funktion (`DecayFunction`) für episodische Relevanz. Es stellte sich die Frage, wie der berechnete `effective_score(now_tx)` in die RAG-Pipeline (ADR-021) integriert wird.
*   **Entscheidung**:
    - Der `effective_score(now_tx)` wird als Nachbearbeitungsschritt **NACH** RRF (Reciprocal Rank Fusion) und **NACH** Cross-Encoder Reranking in der RAG-Pipeline ausgeführt (`Collection::filter_by_importance`).
    - Kandidaten mit `effective_score` unterhalb eines konfigurierbaren Schwellwerts werden aus den finalen Suchergebnissen entfernt.
    - Es findet **KEINE** Neubewertung / Re-Ranking durch Multiplikation des RRF- / Cross-Encoder-Scores mit dem `effective_score` statt.
*   **Alternativen**:
    - Multiplikation des `effective_score` direkt in die RRF-Rankings: Verworfen, da dies die mathematischen RRF-Skalierungsunabhängigkeiten und die empirisch validierte RRF/Reranking-Reihenfolge aus ADR-021 zerstören würde.
*   **Begründung**: Filterung statt Re-Ranking schützt die empirisch nachgewiesenen Trefferquoten des Hybrid-Retrievals (Anthropic Pattern, ADR-021), während irrelevante oder veraltete Erinnerungen (Low Importance / High Decay) zuverlässig ausgeschieden werden.
*   **Konsequenzen**:
    - `filter_by_importance()` in `Collection` filtert nach RRF/Reranker ohne Umsortierung.
    - Zero-Panic Invariante in `ImportanceScore`, `DecayFunction` und `MemoryImportance`.

---

---

# ADR-026: Personalized PageRank (PPR) Graph Retrieval

*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Implementierung von Personalized PageRank (PPR) als eigenständige, deterministische Power-Iterations-Methode auf der bestehenden CSR-Struktur (`CsrGraph`) in `crates/memfuse-graph/src/ppr.rs` ohne externe Bibliotheken (wie `petgraph`).
    2. Ergänzung von `PprConfig` und des Trait-Methoden-Contracts `personalized_page_rank` an `GraphIndex` in `memfuse-core`.
    3. Integration von PPR in `HybridQuery` (`memfuse-core`) und `Collection::hybrid_search_with_strategy` (`memfuse-db`) über die additiv wählbare `GraphTraversalStrategy` (`Hops` vs `PersonalizedPageRank`). Standardverhalten bleibt unverändert `GraphTraversalStrategy::Hops` (3 Hops BFS decay).
*   **Alternativen**:
    - **Option A (In-Tree `petgraph` Dependency)**: Verwendung von `petgraph` für PageRank. Verworfen, da `petgraph` eine Konvertierung/Kopie des CSR-Graphen erzwingen würde (Speicher- & Latenz-Overhead) und unkontrollierte Nicht-Determinismen einbringen könnte.
    - **Option B (`traverse` überschreiben)**: Ersetzung von BFS-Traversierung in `traverse()`. Verworfen, da BFS-Hop-Traversierung und PPR grundlegend unterschiedliche Retrieval-Semantiken besitzen (Hop-Distanz vs. Stationärverteilung eines Random-Walk-mit-Restart).
*   **Begründung**:
    - **Deterministische Konvergenz**: Die Power-Iteration auf dem CSR-Format verwendet eine explizite L1-Norm-Abbruchbedingung (`convergence_epsilon: 1e-6`) und eine harte Obergrenze (`max_iterations: 100`). Rank-Masse an Sackgassen-Knoten (Sackgassen / out-degree 0) wird gleichmäßig auf die Restart-Menge redistribuiert, um die stochastische Matrix-Eigenschaft zu wahren. Tie-Breaking über sekundäre Sortierung nach `EntityId` garantiert bitidentische Ergebnisse über mehrere Läufe.
    - **Zero-Panic / Zero-Hang**: Harte Abbruchschranken verhindern Endlosschleifen selbst auf pathologischen Graphen.
    - **Ruckfreie 4-Signal-Integration**: PPR ist als `GraphTraversalStrategy::PersonalizedPageRank` in `HybridQuery` und `Collection` nahtlos nutzbar und speist seine Ränge direkt in die Reciprocal Rank Fusion (RRF) ein.

---

---

# ADR-027: Label Propagation für Community Detection & GraphRAG


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Für Phase 3 ("Community Detection & GraphRAG") wird eine Methode zur semantischen Clusterbildung von Wissensgraph-Knoten benötigt. Das Ergebnis (Community-Zuordnung pro EntityId) soll asynchron als Batch-Prozess berechnet, im Storage unter `__graph:community:<entity_id>` abgelegt und beim Retrieval gelesen werden.
*   **Entscheidung**:
    - Wahl des **Label-Propagation-Algorithmus (LPA)** anstelle von Louvain.
    - Vollständig deterministische Ausführung durch fixierten RNG-Seed für Knoten-Shuffling und ein striktes Tie-Breaking: Bei relativer oder absoluter Gleichheit von Label-Gewichten gewinnt das kleinstmögliche `EntityId` (numerischer `u64`-Wert).
    - Implementierung direkt auf der bestehenden `CsrGraph`-Struktur in `memfuse-graph::community` ohne zusätzliche externe Abhängigkeiten.
    - Persolidierung im LSM-Storage über `Collection::run_community_detection()` mit strenger TxId-Allokation (`self.allocate_tx()`).
    - Anbindung an das Retrieval über `HybridQuery::same_community_as`, welches Kandidaten derselben Community vor der RRF-Fusion filtert bzw. verstärkt.
*   **Alternativen**:
    - **Louvain-Algorithmus**: Louvain ist bei paralleler Ausführung ohne schwere Synchronisation nicht-deterministisch und erfordert komplexe Graph-Hierarchie-Strukturen.
    - **Echtzeit-Clustering bei jeder Query**: Zu hohe Latenz und Token-Kosten, widerspricht den Zero-Latency- und Sovereign-Core-Prinzipien.
*   **Begründung**: Label Propagation ist hochgradig speichereffizient, lässt sich nahtlos auf CSR-Arrays ausführen, ist ohne externe C/Rust-Dependencies umsetzbar und garantiert bei striktem Tie-Breaking 100%ige Reproduzierbarkeit und Zero-Panic-Sicherheit.
*   **Konsequenzen**:
    - Neue Datei `crates/memfuse-graph/src/community.rs`.
    - Neuer Subcommand `run-community-detection` in `xtask`.
    - Erweiterung von `HybridQuery` und `Collection::hybrid_search_ext`.

---

---

# ADR-028: Dezentrales Inline-Kontextsystem, Sekundengenaue Zeitstempel & Verpflichtendes Mehrfach-Session-Review


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Kontext**:
    1. `WORKING_STATE.md` war ein Merge-Konflikt-Hotspot, da jede Session Freitext und auto-generierte Blöcke in dieselben Zeilen derselben zentralen Datei schrieb. Bei hoher paralleler Jules-Sitzungsdichte führten konkurrierende PRs zu manueller Re-Intervention.
    2. Die Tages-Zeitstempel-Granularität (`TS:YYYY-MM-DD`) verhinderte die exakte Sequenzierung von Ereignissen innerhalb eines Tages bei bis zu 100 Sitzungen pro Tag.
    3. Sequenzielle IDs (`AGT-<CRATE>-NNN`) führten zu Zähler-Kollisionen bei parallelen Sitzungen.
    4. Es fehlte eine strukturierte Mehrfach-Session-Qualitätssicherung. Ein Einzel-Agent-Review leidet unter Bestätigungs-Bias.
*   **Entscheidung**:
    1. **`WORKING_STATE.md` als reine, voll-generierte Projektion**: Die Datei enthält NULL manuell editierten Freitext mehr und liegt vollständig in einem Auto-Marker-Block. Git-Merge-Konflikte in dieser Datei werden deterministisch durch erneutes Ausführen von `just sync-docs` aufgelöst.
    2. **Sekundengenaue Zeitstempel & Hash-IDs**: Alle neuen Tags tragen `TS:YYYY-MM-DDTHH:MM:SSZ` (UTC), ein Pflichtfeld `SESSION:<8-hex-hash>` und eine hash-basierte ID `AGT-<CRATE>-<8-hex-hash>` (`sha256(crate + pfad + zeile + ts)[..8]`). Bestehende `AGT-<CRATE>-NNN`-IDs bleiben unter Bestandsschutz.
    3. **Erweiterter `FILE-CONTEXT`-Kommunikationskanal**: Ergänzt um ein optionales `AGENT-NOTIZ:`-Feld als dezentraler Kommunikationskanal zwischen Sitzungen direkt am Code.
    4. **Verpflichtendes Mehrfach-Session-Review (`REVIEW-PASS`)**: Einführung der Grammatik `REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL` mit Pflichtfeld `PRÜFER-KONTEXT: FRESH`. Jede `STATUS:DONE`-Markierung eines `ANCHOR` erfordert 2 (Standard) bzw. 3 (`ASK`/security/unsafe) `REVIEW-PASS`-Einträge mit unterschiedlichen `SESSION:`-Hashes.
    5. **CI Gate 8**: Unterbefehl `cargo xtask check-review-coverage` erzwingt die Mindestanzahl unabhängiger Review-Pässe in CI (`context-gates.yml`).
*   **Alternativen**:
    - Einbindung externer Go/Python Task-Management-Tools (z.B. Beads). Verworfen, um MemFuse sovereigntiesicher und ohne Netzwerk/neue Fremdabhängigkeiten nativ über Rust/`xtask` zu betreiben.
*   **Begründung**: Beseitigt Merge-Konflikte strukturell durch Konstruktion, stellt sekundengenaue Rückverfolgbarkeit her und eliminiert Bestätigungs-Bias bei Reviews durch das Unabhängigkeitsgebot.
*   **Konsequences**:
    - `rules/tag_taxonomy.md`, `rules/llm_protocol.md` (Schleife 8), `AGENTS.md §6` und `environment_script.sh` aktualisiert.
    - `xtask` generiert `WORKING_STATE.md` und `docs/CHANGELOG.md` deterministisch aus Inline-Tags.

---

---

# ADR-029: WAL-V3 Format & tx_id HMAC-Integritätskette


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Kontext**:
    In `WalEntry::compute_checksum` wurde `tx_id` bisher ignoriert. Ein Angreifer mit Dateisystemzugriff konnte `tx_id` manipulieren, während die HMAC-Kette valide blieb. Beim Replay erhielt die Transaktion eine falsche ID, was die Kausalordnung gestört hätte.
*   **Entscheidung**:
    1. Einführung des WAL-Formats V3 mit Header `b"MFW3"` (`WAL_V3_HEADER`) und `WalVersion::V3`.
    2. Die HMAC-Berechnung in `compute_checksum_v3` bindet `tx_id` (vor `op_type`) sowie Längen-Präfixe `u32` für `key` und `value` ein, um HMAC-Längen-Extension-Angriffe und `tx_id`-Tampering strukturell zu verhindern.
    3. `Wal::try_new` und `append_batch` erzeugen ausnahmslos WAL V3 Dateien.
    4. Version-aware `replay()` validiert V1, V2 und V3 Formate abwärtskompatibel. Beim Öffnen einer V1/V2-Datei wird nach erfolgreichem Replay automatisch eine transparente Migration/Rewrite zu V3 durchgeführt.
*   **Alternativen**:
    - Belassen von V2 und Vertrauen auf Dateisystem-Rechte: Verworfen, da dies das Zero-Trust/Cryptographic-Integrity-Gebot von MemFuse verletzt.
*   **Begründung**:
    Stellt sicher, dass WAL-Einträge nicht nur bzgl. `seq_no` und Key/Value fälschungssicher sind, sondern auch die Kausalordnung der Transaktions-IDs (`tx_id`) kryptographisch authentifiziert ist.
*   **Konsequenzen**:
    - Neue WAL-Dateien nutzen `MFW3`.
    - Vollständige Abwärtskompatibilität und automatische In-Place-Migration für Alt-WALs.

---

# ADR-030: Pre-Commit-Hook für rustfmt & Workflow-Automatisierung


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Erstellung von `.githooks/pre-commit`, das automatisch `cargo fmt --all` (schreibend) vor jedem Commit ausführt und durch rustfmt formatierte Dateien automatisch per `git add -u` zum Commit hinzufügt.
    2. Ergänzung von `.jules/setup/environment_script.sh` um `git config core.hooksPath .githooks`, um den Hook in jeder Jules-VM-Session beim Setup automatisch zu aktivieren.
    3. Härtung von `.github/workflows/rust-ci.yml`, um bei Fehlschlag des Format-Checks in den CI-Logs klare, direkt ausführbare Handlungsanweisungen zur lokalen Korrektur auszugeben.
*   **Alternativen**:
    - Manuelles Einfordern von `cargo fmt` ohne automatischen Hook: Verworfen, da dies nachweislich zu wiederholten CI-Fehlschlägen bei automatisierten Agenten-Commits führte.
*   **Begründung**: Beseitigt wiederkehrende rustfmt-Zeilenumbruch- und Einrückungsdifferenzen in CI an der Quelle und stellt sicher, dass alle Commits konsistent formatiert sind.
*   **Konsequenzen**:
    - `.githooks/pre-commit` existiert und ist ausführbar.
    - `AGENTS.md §6` verweist auf den Ablauf und manuelle Bypasses.

---

---

# ADR-031: Realistic-Scale Benchmark Suite & Semantische Retrieval-Evaluierung


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: Einführung einer reproduzierbaren, skalierbaren Benchmark-Suite (`benches/scale_bench.rs`), RSS-Speicherprofilierung (`/proc/self/status` logging nach `benches/results/scale_rss.csv`), semantischer Retrieval-Evaluierung (`crates/memfuse-db/tests/semantic_recall.rs` Recall@k) und eines CI-Baseline-Jobs (`.github/workflows/bench.yml`).
*   **Alternativen**: Weiterhin Verlass auf Micro-Benchmarks (1–1000 Chunks) und Quantisierungs-Konsistenz-Tests. Verworfen, da diese keine empirische Grundlage für künftige Architekturentscheidungen bzgl. Vamana/DiskANN und Quantisierung (v2-Spezifikation R3/R6) bieten.
*   **Begründung**: Bietet empirisch gemessene Durchsatz-, Latenz-Perzentil- (p50/p95/p99) und Speicher-Baselines (VmRSS) auf In-Memory HNSW & LSM-Storage sowie automatisierte Qualitäts-Gates für `hybrid_search()`.

---

---

# ADR-032: Async LLM-Summarization & Provenance Tracking in ContextCompactor (ID: AGT-DB-004)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Der bisherige `ContextCompactor` in `memfuse-db/src/compaction.rs` ersetzte veraltete Tool-Outputs durch Status-Token (ADR-021). Dies entsprach einer Kürzung/Löschung ohne kognitiven Wissenserhalt. Für Phase 3 der Roadmap ("Memory Consolidation") wird die Zusammenfassung alter Chunks via LLM unter Erhaltung der Provenienz benötigt. <!-- doc-ref-ignore -->
*   **Entscheidung**:
    - Erweiterung der `CompactionStrategy` Enum um die additive Variante `LlmSummarize { max_input_chunks: usize }`.
    - Implementierung der asynchronen Methode `consolidate_via_llm(&self, chunks: &[ContextChunk], ollama: &OllamaClient) -> Result<CompactedContext>` in `compaction.rs`.
    - Das Ergebnis `CompactedContext` enthält ein neues Feld `pub source_doc_ids: Vec<DocId>` zur Nachvollziehbarkeit der Quell-Dokumente.
    - Fehler im LLM-Aufruf werden direkt als `Err(...)` an den Aufrufer propagiert und schlagen NICHT still auf StatusToken zurück (Prinzip: Kein stiller Kontrollflussverlust; Fallback-Entscheidung obliegt der Agenten-Orchestrierung).
*   **Alternativen**:
    - Stiller Fallback auf StatusToken innerhalb von `consolidate_via_llm` bei Netzwerk-/LLM-Fehlern. Verworfen, da dies Kontrollflussverluste verschleiern würde.
*   **Begründung**: Bietet eine saubere, provenance-bewahrende Konsolidierungsstrategie für Memory Consolidation und erfüllt das Gebot "No Silent Failures".
*   **Konsequenzen**:
    - Aufrufer können veraltete Chunks via `consolidate_via_llm` zusammenfassen und behalten Rückverfolgbarkeit auf alle Quell-DocIds.

---

---

# ADR-033: Bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit) im Wissensgraphen (Phase 2 Roadmap)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Entscheidung**:
    - Der öffentliche Edge-Typ in `memfuse-core` (`pub struct Edge`) wird additiv um `valid_from: Option<TxId>` und `valid_to: Option<TxId>` mit `#[serde(default)]` erweitert.
    - `valid_from = None` signalisiert "seit jeher gültig", `valid_to = None` signalisiert "weiterhin gültig".
    - `TxId` wird ausnahmslos als Träger der fachlichen Zeitachsen verwendet (Einhaltung des `SystemTime`-Verbots gemäß AGENTS.md Abschnitt 4).
    - Der `GraphIndex`-Trait erhält die Methode `traverse_at_time(&self, start: EntityId, max_hops: usize, as_of: TxId) -> Result<Vec<(EntityId, f32)>>` mit Fail-Safe Default-Implementierung `Err(MemFuseError::PolicyViolation(...))`.
    - `CsrGraph` implementiert `traverse_at_time` konkret: Traversierung filtert Kanten heraus, für die `as_of < valid_from` oder `valid_to.is_some_and(|t| as_of >= t)` gilt.
*   **Alternativen**:
    - Verwendung von Wall-Clock timestamps (`SystemTime` / Unix Nanos). Verworfen, da `SystemTime` im gesamten Workspace für Sequenzierung strikt verboten ist (AGENTS.md).
    - Anlegen eines separaten `TemporalEdge`-Typs. Verworfen, um Typ-Explosion zu vermeiden und abwärtskompatible Deserialisierung Altdaten über `#[serde(default)]` zu sichern.
*   **Begründung**:
    - Ermöglicht präzise historische Wissensgraph-Abfragen ("was galt zum Zeitpunkt TxId X") ohne Breaking Changes bei bestehenden SSTable-Daten.
*   **Konsequenzen**:
    - `Edge`-Initialisierungen und Deserialisierung bleiben abwärtskompatibel.
    - CSR-Graph speichert und persistiert Validitätsbereiche.

---

---

# ADR-034: Runtime-Precondition Assertions in öffentlichen Low-Level-Distanzfunktionen (`memfuse-index`)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Behebung von Befund F-08 (`AGT-INDEX-005`). Die low-level Distanzfunktionen `cosine_distance`, `euclidean_distance` und `dot_product_distance` in `memfuse-index/src/distance.rs` sind `pub` exportiert. Bisher schützten sie Slice-Längengleichheiten nur via `debug_assert_eq!`, was in Release-Builds (`opt-level = 3`, LTO) entfernt wurde. Bei fehlerhaften Aufrufen mit ungleichen Slice-Längen drohte in den nachfolgenden `unsafe`-SIMD-Blöcken (AVX2/AVX512/NEON) ein stummer Out-of-Bounds Buffer-Overread (Undefined Behavior).
*   **Entscheidung**:
    - Ersetzung von `debug_assert_eq!(a.len(), b.len())` durch eine release-aktive Laufzeitprüfung `assert_eq!(a.len(), b.len(), "Vector lengths must match")` in allen drei öffentlichen Distanzfunktionen.
    - Dokumentation der Vorbedingung und des Panic-Vertrags in einer expliziten Rustdoc `/// # Panics` Sektion an jeder Funktion.
    - Autorisierung dieser Panic-Prüfung als explizit dokumentierte Ausnahme von der "No Panics in libraries"-Doktrin (CONSTITUTION.md), da es sich um die Durchsetzung von Verträgen bei low-level SIMD-Funktionen handelt, deren Signatur (`-> f32`) für Hot-Path-Performance erhalten bleiben muss.
*   **Alternativen**:
    - **Option A (Signaturänderung zu `-> Result<f32, ...>`)**: Verworfen, da dies signifikanten Overhead auf dem Hot-Path erzeugen und alle Aufrufer sowie Benchmarks brechen würde.
    - **Option B (Sichtbarkeit auf `pub(crate)` reduzieren)**: Verworfen/abgewogen gegen Option 1, da `cosine_distance`, `euclidean_distance` und `dot_product_distance` als public Utility-API des `memfuse-index`-Crates etabliert sind und in Benchmarks/Tests genutzt werden.
*   **Begründung**: Der O(1) Längen-Check ist gegenüber der O(n) SIMD-Berechnung vernachlässigbar. Die explizite Panic bei Vorbedingungsverletzung schützt zu 100% vor Undefined Behavior und Memory-Safety-Verstößen an den `unsafe` SIMD-Grenzen.

---

---

# ADR-035: Governance-System-Härtung — Prozessregeln gegen wiederkehrende Trait-Default-, Typ-Dopplungs- und Stale-Finding-Fehler


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Über mehrere Wochen wiederholte sich in unabhängigen Audit-Zyklen desselben Projekts dasselbe Muster von Fehlerursachen: (1) Trait-Default-Fallen, (2) Typ-/Namensdopplungen, (3) Unverifiziertes Weiterschleifen veralteter Befunde, (4) Rein informatives Environment-Skript ohne Hard-Gate bei Blocker-Tags, (5) Word-identische Copy-Paste-SAFETY-Kommentare.
*   **Entscheidung**:
    1. **Trait-Default-Pflichttest-Regel**: Für jedes `pub trait` mit einer Default-Methode MUSS im selben PR, der einen neuen Implementor hinzufügt, ein Integrationstest existieren, der beweist, dass die Default-Implementierung nicht still greift.
    2. **Zentrales Typ-Register (`docs/TYPE_REGISTRY.md`)**: Vor Anlegen eines neuen Typs/Traits muss das Typ-Register nach Kollisionen durchsucht werden.
    3. **Audit-Intake-Verifikationsprotokoll (`.jules/AUDIT_INTAKE_PROTOCOL.md`)**: Jeder Finding aus externen Audit-Dokumenten MUSS vor Implementierung am aktuellen Quellcode gegengelesen und bei Obsoleszenz als "entkräftet" markiert werden.
    4. **Hard-Gate für BLOCKER-Tags**: `.jules/setup/environment_script.sh` bricht bei offenen `BLOCKER`-Tags mit `exit 1` ab (sofern keine explizite Blocker-Fix-Ausnahme gesetzt ist).
    5. **SAFETY-Kommentar-Unikats-Pflicht**: SAFETY-Kommentare müssen die konkrete Invariante der spezifischen Funktion benennen; word-identische Duplikate sind unzulässig.
    6. **JULES_CONTEXT.md Frischegarantie**: Warnhinweis am Dateianfang verlangt Gegenprüfung mit `WORKING_STATE.md` und aktuellem Code.
*   **Alternativen**: Weiterhin rein vertrauensbasierte Regeln ohne harte Prozess-Gates und zentrale Typ-Register. Verworfen wegen nachgewiesener wiederkehrender Fehler in Multi-Agenten-Sessions.
*   **Begründung**: Prozessuelle Härtung verhindert das Einschleichen schleichender Regressionen und reduziert Kontext-Halluzinationen in zukunftigen Jules-Sitzungen.
*   **Konsequenzen**:
    - `AGENTS.md`, `CONSTITUTION.md`, `docs/SOURCE_OF_TRUTH.md`, `rules/simd_safety.md` und `.jules/setup/environment_script.sh` aktualisiert.
    - Neue Dateien `docs/TYPE_REGISTRY.md` und `.jules/AUDIT_INTAKE_PROTOCOL.md` angelegt.

---

---

# ADR-036: unsafe-Scope-Erweiterung für test-only crypto anti_tamper

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: AGENTS.md §4 wird um den test-only unsafe-Ausnahmefall in `memfuse-crypto/src/anti_tamper.rs` ergänzt (Zeroize-Drop-Semantik-Verifikation). Im Produktionsbuild bleibt `memfuse-crypto` vollständig unsafe-frei (`#![cfg_attr(not(test), forbid(unsafe_code))]`).
*   **Begründung**: AUD-01 aus Audit 2026-08-28 dokumentierte Doku-Drift zwischen tatsächlichem Code und AGENTS.md. Governance-Dokumente müssen Realität abbilden, nicht verbergen.

---

---

# ADR-037: VectorIndex-Generalisierung in Collection<S, V>


*   **Datum**: 2026-08-29
*   **Status**: ✅ Implementiert (2026-09-03)
*   **Entscheidung**: Die Datenstruktur `Collection<S: StorageEngine = LsmStorage>` in `crates/memfuse-db/src/collection.rs` wird generisch über den `VectorIndex`-Trait-Implementor erweitert: `Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex>`. Dadurch wird die starre Kopplung an `Arc<HnswIndex>` aufgehoben und die Nutzung alternativer Vektor-Indizes (wie z. B. `DiskAnnIndex` aus `memfuse-index`) ermöglicht. <!-- doc-ref-ignore -->
*   **Alternativen**:
    - **Option A (Dynamischer Trait-Object Trait-Dispatch `Arc<dyn VectorIndex>)`**: Verworfen, da `VectorIndex` in manchen Pfaden dynamischen Trait-Funktions-Dispatch mit Performance-Overhead auf dem Hot-Path verbindet und die Typensicherheit bei konkreter Vektorindex-Instanziierung einbüßt.
    - **Option B (Status Quo belassen)**: Verworfen, da `DiskAnnIndex` als out-of-core Vektorindex vollständig implementiert ist, aber wegen der harten `Arc<HnswIndex>`-Typisierung in `Collection` ungenutzte technische Schuld darstellte.
*   **Begründung**: Die Verwendung eines generischen Typparameters mit Standard-Typ `V = HnswIndex` garantiert 100%ige Abwärtskompatibilität für alle bestehenden Aufrufer und Typ-Signaturen (wie `Collection<LsmStorage>`). Gleichzeitig wird die Entkopplung von der konkreten HNSW-Implementierung im `memfuse-db`-Crate vollzogen.
*   **Konsequenzen**:
    - `Collection` kann jetzt auch mit `DiskAnnIndex` instanziiert und betrieben werden (`Collection<LsmStorage, DiskAnnIndex>`).
    - `Collection::new` nimmt `index: Arc<V>` als Parameter auf; die Convenience-Funktion `Collection::with_hnsw` kapselt die bisherige HNSW-Konstruktion.
    - **Implementierungsnotiz (2026-09-03)**: `Collection<S, V>` ist jetzt generisch. Alle bestehenden Aufrufer nutzen weiterhin den Default `V = HnswIndex` ohne Typ-Annotationsänderung. `Collection<LsmStorage, DiskAnnIndex>` ist hinter `#[cfg(feature = "experimental-diskann")]` verfügbar.

---

---

# ADR-038: Zettelkasten Memory Links (A-MEM) & Supersedes Displacement Logic

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Erweiterung von `ContextChunk` (`memfuse-core`) um `links: Vec<MemoryLink>` mit `#[serde(default)]`.
    2. Einführung von `LinkRelation` (`Elaborates`, `Contradicts`, `Supersedes`, `References`) und `MemoryLink` (`target: DocId`, `relation: LinkRelation`, `created_at_tx: TxId`).
    3. Implementierung der Methode `Collection::link_memories` (idempotent, interne `TxId` via `allocate_tx()`) und `Collection::traverse_links` (iterativer BFS mit `VecDeque`, zyklen-sicher, max `MAX_SEARCH_K`).
    4. Implementierung der Supersedes-Verdrängungslogik in `hybrid_search_with_query()`: Wenn `include_superseded = false` (Default), werden Chunks verdrängt, auf die ein anderes Treffer-Dokument einen `MemoryLink` der Relation `Supersedes` trägt.
*   **Alternativen**:
    - **Entity-to-Entity Verlinkung**: Verworfen, da CSR-Graph-Terrain (EntityId-zu-EntityId). Zettelkasten A-MEM operiert rein auf DocId-zu-DocId Ebene für ContextChunks.
*   **Begründung**:
    - Schafft explizite, benannte Querverweise zwischen ContextChunks zur Repräsentation geordneter Wissensnetze.
    - Automatisches Ausfiltern veralteter/ersetzter Chunks erhöht die Präzision des RAG-Retrievals, ohne Historie aus dem Speicher zu löschen.

---

---

# ADR-039: reqwest als Workspace-Dependency für memfuse-router

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: `reqwest` wird als zentrale Workspace-Dependency in `[workspace.dependencies]` im Root-`Cargo.toml` aufgenommen und für `memfuse-router` explizit freigegeben.
*   **Alternativen**: Ersetzung durch `memfuse-ollama`.
*   **Begründung**: `memfuse-router` nutzt `reqwest` in `dispatch_to_slm` für generische HTTP JSON-RPC 2.0 Aufrufe (`slm_process_context`) an frei konfigurierbare MCP-Endpunkte von Small Language Models (SLMs). `memfuse-ollama` deckt ausschließlich Ollama REST-API-Endpunkte ab und kann diese generische JSON-RPC-MCP-Dispatch-Funktionalität nicht bereitstellen.
*   **Sicherheitsbewertung**: Nutzung mit `default-features = false` und `rustls-tls` (kein `native-tls` / OpenSSL C-Dependency-Overhead, vollständig konform mit der Sovereign Core Policy aus ADR-004).
*   **Konsequenz**: `reqwest` ist fortan eine explizit genehmigte Workspace-Dependency ohne Version Drift zwischen Crates.

---

---

# ADR-040: collection.rs Modularisierung (God Object Auflösung) <!-- doc-ref-ignore -->

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: `collection.rs` wird in Submodule unter `crates/memfuse-db/src/collection/` aufgeteilt. <!-- doc-ref-ignore -->
*   **Alternativen**: Belassen von `collection.rs` als monolithischer ~2.900 LOC Crate-Teil. <!-- doc-ref-ignore -->
*   **Begründung**: Beseitigt AUD-08 ("God Object") und verbessert Lesbarkeit sowie Wartbarkeit. Öffentliche API und alle Typnamen bleiben exakt unverändert. Alle Re-Exports werden über `crates/memfuse-db/src/collection/mod.rs` bereitgestellt (identische öffentliche Oberfläche wie bisher).

---

---

# ADR-041: TOMBSTONE_BIT-Disziplin in Sequenznummer-Berechnungen und rollback_to_tx

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: In allen Pfaden der LSM-Storage-Engine (`rollback_to_tx`, WAL-Replay, SSTable-Recovery), in denen maximale Sequenznummern (`max_seq`) ermittelt werden, MUSS das `TOMBSTONE_BIT` (Bit 63, `1 << 63`) strikt maskiert werden (`seq & !TOMBSTONE_BIT`), bevor Vergleiche, Zuweisungen oder Hochzählungen für `next_seq_no` stattfinden.
*   **Alternativen**:
    - Unmaskierte Übernahme in `max_seq`: Verworfen, da Bit 63 in `next_seq_no` wandert und nachfolgende reguläre Inserts fälschlich als gelöscht (Tombstone) markiert.
    - Maskierung beim Schreiben der SSTable-Metadaten verändern: Verworfen, um bestehende Metadatenformate und Disk-Layouts nicht zu verändern.
*   **Begründung**: Bit 63 signalisiert ausschließlich das Lösch-Tombstone-Flag in Datenzeilen. Es stellt keinen numerischen Wertanteil der Sequenznummer dar. Maskierung an den Lesestellen schützt die Invariante "Bit 63 darf niemals in `next_seq_no` einfließen" vollständig vor stillem Datenverlust nach Rollbacks auf Delete-Operationen.

---

---

# ADR-042: Re-Integration von `memfuse-saos-agent`

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: Re-Integration der Funktionalitäten aus dem archivierten `memfuse-saos-agent` in das Hauptcrate `memfuse-agent`.
*   **Begründung**: Konsolidierung des Agenten-Loops und Vereinfachung der Crate-Struktur im Workspace.

---

---

# ADR-043: Aktualisierung von `last_committed_tx` vor der Sichtbarmachung von SSTables in `LsmStorage::flush`

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: In `LsmStorage::flush()` MUSS `last_committed_tx` aktualisiert werden, BEVOR die neu erstellte SSTable über den `sstables`-Vektor für Lesepfade (z. B. `get_at_seq`, `scan_prefix_at`) sichtbar gemacht wird (`last_committed_tx vor Datensichtbarkeit aktualisieren`).
*   **Alternativen**:
    - Beibehalten der bisherigen Reihenfolge (`sstables.push` vor `last_committed_tx` update): Verworfen, da hierbei ein Race-Fenster entsteht, in dem ein paralleler Reader die neue SSTable bereits im `sstables`-Vektor sieht, sein `snapshot_tx` aber noch vor der Erhöhung von `last_committed_tx` liest und dadurch Daten sieht, die jenseits seines Snapshots liegen.
    - Vollständige Umstellung auf exklusiven Schreib-Lock über den gesamten Reader-Öffnungs-Pfad: Verworfen, um I/O-Operationen (SSTable öffnen) nicht unter Lock zu halten.
*   **Begründung**: MVCC-Snapshot-Isolation erfordert, dass transaktionale Sichtbarkeit atomar oder streng monoton vor der Datensichtbarkeit fortschreitet. Die Aktualisierung von `last_committed_tx` vor `sstables.push()` eliminiert das Race-Fenster für parallele Reader vollständig, ohne Lock-Kontention durch I/O zu erhöhen.

---

---

# ADR-044: MCP Write-Authorization & Sandbox Policy (Default Read-Only)

*   **Datum**: 2026-08-30
*   **Status**: ✅ Final
*   **Entscheidung**: `memfuse-mcp` erzwingt eine strikte Sandbox-Policy für alle MCP Tool-Aufrufe. Datenbank-Schreibzugriffe (`DatabaseWrite` Tools wie `memfuse_insert`, `memfuse_delete`, `memfuse_upsert`, `memfuse_relate`, `memfuse_create_collection`, `memfuse_drop_collection`) sind standardmäßig GESPERRT (`allow_db_writes = false`). Schreibberechtigungen können ausschließlich explizit per Aufruf-Parameter/Server-Initialisierung (`McpServer::with_write_permission()`) bzw. Umgebungsvariable `MEMFUSE_MCP_ALLOW_WRITE=true` aktiviert werden. Vor jedem Tool-Dispatch prüft `call_tool` zentral `McpSandbox::validate_tool_call()`.
*   **Alternativen**:
    - Uneingeschränkter Schreibzugriff im Default: Verworfen aus Sicherheitsgründen (Zero-Trust/Least-Privilege Prinzipsschutz für LLM-MCP-Integrationen).
    - Einzelne Tool-Gefahrenstufen ohne zentrale Sandbox-Validierung: Verworfen, da dezentrale Prüfungen fehleranfällig und schwer zu auditieren sind.
*   **Begründung**: Schutz der lokalen Knowledge Base vor unbeabsichtigten oder böswilligen Schreib- und Löschoperationen durch extern gesteuerte MCP-Clients (R-01 Containment Protection).

---

---

# ADR-045: Entkopplung von `memfuse-router` und `memfuse-mcp` durch IPC JSON-RPC Typverschiebung

*   **Datum**: 2026-08-31
*   **Status**: ✅ Final
*   **Entscheidung**: Die generischen JSON-RPC 2.0 Protokolltypen (`JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`) werden aus `memfuse-mcp` nach `memfuse-core::ipc::jsonrpc` verschoben und in `memfuse-mcp::protocol` re-exportiert. `memfuse-router` importiert diese Typen fortan direkt aus `memfuse-core::ipc`. Die Abhängigkeit `memfuse-mcp` wird aus `crates/memfuse-router/Cargo.toml` sowie aus den Ausnahmeregeln in `.github/workflows/dag-check.yml` entfernt.
*   **Alternativen**:
    - Erstellung eines separaten `memfuse-jsonrpc`-Crates in Layer 1: Verworfen, um Crate-Explosion zu vermeiden; `memfuse-core::ipc` existiert bereits als zentrales IPC-Typ-Modul in Layer 0.
    - Beibehaltung der Layer-4-Dependency in Layer 3: Verworfen, da dies das 5-Layer-DAG-Modell verletzt und Zirkelbezüge zwischen Router und MCP verhindert.
*   **Begründung**: Beseitigt die Schichtgrenzenverletzung (Layer 3 → Layer 4) ohne Verhaltensänderung oder Breaking Changes für externe Konsumenten von `memfuse_mcp::protocol::*`.

---

---

# ADR-046: Wiederherstellung von `memfuse-agent` aus dem Archiv


- **Datum**: 2026-08-27
- **Status**: ✅ Final
- **Entscheidung**: Kernkomponenten aus `memfuse-saos-agent` (gelöscht in Commit 55a3464)
  werden als `memfuse-agent` wiederhergestellt: `AgentTool` Trait, `OrchestratorEngine`,
  `StateGraph`, `AuditLog`.
- **Was NICHT zurückgeholt wird**: `memfuse-cluster` (Raft — bleibt in ADR-005 Frozen Zone).
- **Begründung**: Die MCP-Sandbox ist zustandslos. Multi-Step Agent-Workflows über MCP
  verlieren bei Crash ihren State. Der `checkpoint → execute → commit → audit`-Loop aus dem
  alten Crate ist genau die fehlende Persistenzschicht.
- **API-Anpassungen**: `AuditLog.replay_task` nutzt `scan_prefix` statt sequenziellem
  Probing. `OrchestratorEngine.checkpoint` nutzt `CheckpointMeta`/`CheckpointRegistry`
  statt der alten `PersistentCheckpointStore::create_checkpoint`-Signatur.

---

---

# ADR-047: SIMD-Implementierungsstrategie — std::arch vs portable_simd (AGT-INDEX-002)


*   **Datum**: 2026-09-03
*   **Status**: ✅ Entschieden
*   **Kontext**: AGT-INDEX-002 dokumentierte, dass `std::simd` (portable_simd, Issue #86656) per
    September 2026 noch nicht auf stable Rust verfügbar ist. `memfuse-index/src/distance.rs` nutzt
    bereits korrekt `std::arch::x86_64` Intrinsics mit Runtime-Feature-Detection via
    `is_x86_feature_detected!` (AVX-512, AVX2, SSE4) und `is_aarch64_feature_detected!` (NEON).
*   **Entscheidung**: Status quo (`std::arch` + Runtime-Detection) ist der korrekte, stabile Pfad.
    Kein Refactoring auf `portable_simd` bis Issue #86656 auf stable Rust landet.
*   **Re-Evaluierungs-Trigger**: Wenn `portable_simd` in einer stable Rust-Version stabilisiert wird,
    soll `distance.rs` auf `std::simd::prelude::*` migriert werden (bessere Cross-Platform-Portabilität,
    weniger `unsafe`-Blöcke nötig).
*   **Konsequenzen**: AGT-INDEX-002 wird als RESOLVED geschlossen. WORKING_STATE.md zeigt danach 0 offene Tags.

---

---

# ADR-048: WAL Legacy-Key Feature-Gating & Downgrade Protection

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Die automatische Fallback-Entschlüsselung / Integritätsprüfung alter Write-Ahead-Logs mittels hartkodiertem `LEGACY_INTEGRITY_KEY` wird hinter das explizite Konfigurations-Flag `allow_legacy_integrity_key_fallback: bool` (Default: `false`) in `WalConfig` gestellt. Der Standardpfad in `Wal::open()` weist alte WAL-Dateien ohne explizites Opt-In als fehlerhaft zurück (`MemFuseError::wal_corruption`).
*   **Alternativen**:
    - Beibehaltung des automatischen Fallbacks: Verworfen, da ein Angreifer alte WAL-Dateien unterschieben und einen Silent Downgrade herbeiführen könnte.
    - Vollständiges Entfernen von `LEGACY_INTEGRITY_KEY`: Verworfen, um Migrationstools das Auslesen alter Logdateien weiterhin zu ermöglichen.
*   **Begründung**: Verhindert unbefugte Downgrade-Angriffe auf den WAL-Integritätsmechanismus, wahrt aber Abwärtskompatibilität bei expliziter Migration.

---

---

# ADR-049: Audit-Log Append-Only Enforcement via `put_kv_if_absent`

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: `Collection` wird um die atomare Methode `put_kv_if_absent(&self, id: &str, value: &serde_json::Value)` erweitert, die vor dem Schreiben eine tx-scoped Existenzprüfung durchführt und bei Treffer `MemFuseError::Conflict` zurückgibt. `AuditLog::append()` nutzt ausschließlich `put_kv_if_absent()`.
*   **Alternativen**:
    - Nutzung von `put_kv()` mit clientseitigem `get_kv()`-Check: Verworfen, da race-condition-anfällig bei parallelen `append()`-Aufrufen.
    - Schreibsperre auf Tabellenebene: Verworfen wegen unötigem Performance-Overhead für nicht-kollidierende Steps.
*   **Begründung**: Garantiert die deklarierte Invariante des Audit-Logs ("immutable append-only trail, zero overwrite/deletion paths").

---

---

# ADR-050: Router Single-Conformal Calibration & Lock Scope Consolidation

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Die veraltete Methode `recalibrate()` in `ProfileCalibrationState` wird ersatzlos entfernt. `recalibrate_conformal()` dient als einziger Kalibriermechanismus im Router.
    2. Profilselektion, Candidate Scoring und Kalibrierungs-Update in `RouterEngine::route()` werden innerhalb eines einzigen atomaren Schreib-Locks (`self.calibration.write()`) ausgeführt.
*   **Alternativen**:
    - Beibehaltung des dualen Kalibriersystems: Verworfen, da zwei konkurrierende Kalibriermethoden inkonsistente Schwellenwerte erzeugen.
    - Zweiphasiges Locking (Read Lock für Kaskade, Write Lock für Update): Verworfen wegen TOCTOU-Race-Condition zwischen Read und Write.
*   **Begründung**: Beseitigt TOCTOU-Races bei parallelen Routing-Anfragen und konsolidiert die Kalibrierung auf Conformal Prediction.

---

---

# ADR-051: Context Compaction Delete Error Propagation

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: In `ConsolidationSession::commit()` MUSS das Ergebnis der Quelldokument-Löschung (`delete_op`) zwingend mit `?` propagiert werden. Deserialisierungsfehler beim Lesen der Quelldokument-Metadaten geben `MemFuseError::Serialization` zurück. Nicht mehr auffindbare Quelldokumente werden geloggt und als Idempotenz-OK übergangen.
*   **Begründung**: Verhindert Datenverlust und stille Discards im Konsolidierungspfad.

---

---

# ADR-052: Synchronous PinGuard Drop Orphan Registration

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: `PinGuard::drop` registriert verwaiste Sequenznummern synchron via `OrphanRegistry` ohne asynchrone Tasks (`tokio::spawn`) abzuspalten.
*   **Begründung**: Verhindert verdeckte Space-Leaks und unvollständige Drops bei Prozess-Shutdowns.

---

---

# ADR-053: Instance-Scoped Orphan State in PersistentCheckpointStore

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Verwaiste Checkpoint- und Pin-Zustände werden instanzspezifisch in `PersistentCheckpointStore` verwaltet anstatt über prozessglobale statische Variablen (`ORPHANED_CHECKPOINTS`). Globale Hilfsfunktionen werden als `#[deprecated]` markiert.
*   **Begründung**: Stellt die Korrektheit in Multi-Session-Servern (MCP, Tauri) sicher, in denen mehrere unabhängige MemFuse-Instanzen parallel existieren.

---

---

# ADR-054: Unified Router Scoring & TOCTOU-Safe Calibration Scope

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. `SlmProfile::domain_communities` nutzt `HashSet<u64>` für O(1) Community-Lookups mit deterministischer `sorted_u64_set` Serde-Unterstützung.
    2. Candidate Scoring wird in der zentralen Hilfsfunktion `score_profile()` mit der benannten Konstante `COMMUNITY_RELEVANCE_BOOST = 1.2` konsolidiert.
    3. Die legacy `recalibrate()` Methode wird aus `ProfileCalibrationState` entfernt.
    4. Routing-Entscheidung, Scoring und Kalibrierungs-Updates in `RouterEngine::route()` erfolgen atomar innerhalb einer einzigen Schreib-Lock-Akquise.
*   **Begründung**: Schließt Race Conditions (TOCTOU) bei parallelem Routing, eliminiert redundante Scoring-Implementierungen und vereinheitlicht die Konformal-Kalibrierung.

---

---

# ADR-055: WAL Legacy Key Fallback Protection

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Der Fallback auf den statischen `LEGACY_INTEGRITY_KEY` beim Replay alter Write-Ahead-Logs erfordert das explizite Flag `allow_legacy_integrity_key_fallback: bool` (Default: `false`).
*   **Begründung**: Schützt vor unbefugten Downgrade-Angriffen auf den WAL-Integritätsmechanismus.

---

---

# ADR-056: Python FFI Panic Isolation via PyErr Exception Mapping

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Ersetzung aller `panic!()` Aufrufe in Nicht-Test-Quellcode von `memfuse-py` durch strukturierte PyO3 Exception-Returns (`PyValueError`, `PyRuntimeError`). Blockierende Aufrufe werden durch `run_blocking_ffi` mit `std::panic::catch_unwind` geschützt.
*   **Begründung**: Verhindert CPython-Prozessabstürze über die PyO3 FFI-Grenze hinweg.

---

---

# ADR-057: Lücken-Dokumentation (Umnummerierung / Ausgelassen)

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Entscheidung**: Die Nummer ADR-057 wurde im Zuge paralleler Audit-Sessions ausgelassen und ist nicht vergeben. (Anmerkung: ADR-048 in `docs/decisions/` wurde als ADR-059 neu nummeriert und in `DECISIONS.md` integriert).

---

---

# ADR-058: Error-Logging-Pattern für synchrones Orphan-State Persistieren in Checkpoint

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Entscheidung**: In `InstanceOrphanRegistry` und `register_pinned_seq_no_orphan` (`crates/memfuse-checkpoint/src/lib.rs`) werden Schreibfehler beim synchronen Persistieren des Orphan-Zustands (`persist_sync()`) nicht mehr mit `let _ =` verworfen, sondern explizit über `if let Err(e) = ... { tracing::error!(?e, "..."); }` kontextspezifisch geloggt.
*   **Alternativen**:
    - Ändern der Rückgabetypen auf `Result<()>`: Verworfen, da Aufrufer in `Drop`-Implementierungen und synchronen Legacy-Funktionen keinen `?`-Kontext besitzen und dies zu kaskadierenden API-Breaks führen würde.
*   **Begründung**: Erfüllt CONSTITUTION.md §2 (kein stilles Verwerfen von E/A-Fehlern auf Recovery-Persistenzpfaden) ohne API-Signaturen zu brechen.

---

---

# ADR-059: Python FFI Panic Isolation (ehemals docs/decisions/ADR-048)

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Alle `panic!()`-Aufrufe in `memfuse-py` außerhalb von `#[cfg(test)]` werden durch `Err(PyErr)` ersetzt.
*   **Begründung**: Ein Rust-Panic über die PyO3 FFI-Grenze hinweg führt zum Absturz von CPython. `catch_unwind` ist kein Ersatz für korrekte Fehlerbehandlung an Aufrufstellen.

---

---

# ADR-060: ADR-Governance — Konsolidierung auf DECISIONS.md als Einzel-Quelle

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Entscheidung**: `docs/decisions/` wird aufgelöst. `DECISIONS.md` im Root-Verzeichnis ist die einzige kanonische Quelle für Architecture Decision Records (ADRs).
*   **Begründung**: Einhaltung des MECE-Prinzips aus `CONSTITUTION.md` ("Jede Information lebt an genau EINEM Ort"). Das Dual-System (`DECISIONS.md` vs. `docs/decisions/`) erzeugte Nummernkollisionen und Verwirrung. Das xtask-Tooling kennt und prüft primär `DECISIONS.md`.

---

---

# ADR-061: 2-Phasen-Lock für HNSW Rebuild

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Kontext**: `HnswIndex::rebuild()` hielt bisher `write_mutex` über die gesamte Rebuild-Dauer (Snapshot, Offline-Index-Aufbau, Quantizer-Retraining, Re-Insert aller aktiven Nodes, Atomic Swap). Bei großen Indices blockierte dies Schreiboperationen (`insert`, `delete`, `commit`) für mehrere Sekunden bis Minuten.
*   **Entscheidung**: Umstellung von `rebuild()` auf ein 2-Phasen-Verfahren:
    - **Phase 1 (lock-frei bzgl. `write_mutex`)**: Erfassen eines Snapshot-TxId Watermarks (`last_tx_id`), Snapshot der aktiven Nodes unter kurzen Read-Locks, Aufbau des neuen Index inkl. Quantizer-Retraining komplett offline. Ingest-Schreibzugriffe laufen ungestört auf dem alten Index weiter.
    - **Phase 2 (kurzer exklusiver `write_mutex`-Scope)**: Erwerben des `write_mutex`, Ermittlung aller seit `snapshot_tx` getätigten Operationen via `SequenceLog::changes_since()`, Replay des Deltas auf den neuen Index und atomarer Swap der internen Datenstrukturen.
*   **Verworfene Alternativen**:
    - *Vollständig lock-freie Datenstruktur via `crossbeam-epoch`*: Verworfen, da dies ein komplettes Redesign der HNSW-Graphrepräsentation erfordern würde und mit hoher Komplexität verbunden ist.
*   **Konsequenzen**: Phase 2 skaliert mit $O(\Delta)$ (Anzahl Ingest-Operationen während Phase 1) statt $O(N)$ (Gesamtzahl Dokumente). Schreiblatenz während Rebuild sinkt von Sekunden auf Millisekunden.

---

---

# ADR-062: Fault-Injection-Testsuite für WAL V3/MVCC (adaptiert aus chimeraDB SPEC-035)

*   **Datum**: 2026-09-05
*   **Status**: ✅ Final
*   **Entscheidung**:
    - Die Fault-Injection-Testsuite wird ausschließlich als Test-only Integrationstests (`tests/`) sowie ein Hilfsbinary (`examples/chaos_writer.rs`) in `crates/memfuse-store` umgesetzt. <!-- doc-ref-ignore -->
    - Es wird KEIN neues Workspace-Crate angelegt und KEINE Änderung an Quellcode unter `crates/memfuse-store/src/**` vorgenommen.
- **Alternativen**:
    - *Eigenes `chimera-chaos`-artiges Crate mit Produktions-Hooks (`FaultInjector::inject_sync`)*: Verworfen, da dies ASK-pflichtige API- und Hot-Path-Änderungen erfordert hätte, ohne dass dafür ein belegter Bedarf existierte.
- **Explizit verworfene Szenarien**:
    - `IOLatency` und `NetworkDegradation`: Verworfen, da MemFuse keine Netzwerkschicht besitzt (ADR-010: stdio-only JSON-RPC) und kein belegter Slow-Disk-Use-Case vorliegt, der Hooks im Hot-Path rechtfertigen würde.
- **CI-Kadenz**:
    - Einzelne Fault-Injection-Tests laufen als reguläre Integrationstests in `cargo test --workspace`.
    - Die kombinierte Fault-Matrix (`chaos_matrix.rs`) läuft ausschließlich nightly, ist `#[ignore]`-gated und blockiert keine Pull Requests.
*   **Begründung**: Bietet gezielte Abdeckung verbleibender Crash-Resilienz-Lücken (SSTable Bit-Flips, echte Process-Kills, Task-Abbrüche, Memory-Pressure) ohne Beeinträchtigung der Produktionscode-Topologie oder der PR-Laufzeiten.

---

# ADR-063: F-02 Nucleation — Tombstone-Pruning-Variante vs. ursprüngliches Rebuild-Veto

*   **Datum**: 2026-09-07
*   **Status**: Eingeschränkt akzeptiert (mit hartem Gate)

## Kontext
Frühere Architekturanalysen (siehe externe Analyse-Session 2026-09-07) sprachen ein permanentes Veto gegen partiellen HNSW-Rebuild aus. Begründung: Delaunay-Nachbarschaftszerstörung bei aktivem Re-Wiring unter RwLock-Contention führt zu Recall-Kollaps.

Die tatsächliche Implementierung in `hnsw.rs:1812` (`rebuild_region()`) führt KEIN aktives Re-Wiring durch. Sie entfernt ausschließlich Referenzen auf tombstonierte Knoten aus Nachbarschaftslisten aktiver Knoten — ohne Ersatzkanten oder erneutes RNG-Pruning. Dies umgeht den im ursprünglichen Veto beschriebenen Worst Case (Lock-Contention durch aktive Neuverdrahtung), erzeugt aber ein separates, bislang ungetestetes Risiko: dauerhafter Grad-Verlust betroffener Knoten.

## Entscheidung
Die Tombstone-Pruning-Variante wird NICHT als generelles Veto-Verstoß behandelt, da sie technisch different von dem ist, wovor das ursprüngliche Veto warnte. Sie bleibt jedoch hinter `partial-rebuild-pruning` (non-default) UND darf erst dann in irgendeinem Kontext default-aktiviert werden, wenn:
1. `test_nucleation_recall_regression()` (siehe Test-PR) seit ≥ 30 Tagen stabil grün ist
2. Eine Grad-Wiederherstellungsstrategie evaluiert wurde (z.B. periodischer Vollrebuild als Sicherheitsnetz bei > X% Grad-Verlust in einer Region)

## Verworfene Alternativen
- Vollständige Entfernung von rebuild_region(): Verwirft Arbeit ohne technischen Grund, da das eigentliche Veto-Risiko (aktives Re-Wiring) nicht vorliegt.
- Sofortige Default-Aktivierung: Kein Recall-Nachweis vorhanden — abgelehnt.

## Konsequenzen
- `partial-rebuild-pruning` bleibt non-default bis Bedingungen erfüllt
- CI-Check (siehe Folge-PR VETOES.md) muss diesen ADR referenzieren können
- Grad-Verlust-Monitoring sollte in zukünftigem Observability-Sprint ergänzt werden

---

# ADR-064: memfuse-py als separater Cargo-Workspace (Panic-Strategie-Isolation)

* **Datum**: 2026-09-07
* **Status**: ✅ Angenommen (bereits implementiert, dieser ADR dokumentiert nachträglich eine bestehende, korrekte Entscheidung — siehe P6-Nachpflegepflicht).

## Kontext
Der Haupt-Workspace von MemFuse setzt im Release-Profil `panic = "abort"` (Begründung: Performance-Optimierung, binäre Minimalität und deterministischer Abbruch im Server-/DB-Engine-Betrieb).
`memfuse-py` exponiert PyO3-Bindings, die an der FFI-Grenze zu CPython `catch_unwind()` nutzen müssen, um Rust-Panics als Python-Exceptions abzubilden statt den gesamten Python-Interpreter per `SIGABRT` abstürzen zu lassen (siehe Kommentar in `crates/memfuse-py/src/lib.rs`, Zeilen 219–222).
Die Panic-Strategie ist in Cargo eine Workspace-weite Einstellung — sie kann nicht pro Crate innerhalb desselben Workspace überschrieben werden.

## Entscheidung
`crates/memfuse-py/Cargo.toml` definiert ein eigenständiges `[workspace]`-Manifest und wird dadurch bewusst NICHT Mitglied des Haupt-Workspace. Dies ist kein Versehen und keine technische Schuld.

## Konsequenzen
- `cargo build --workspace` im Wurzelverzeichnis baut `memfuse-py` NICHT mit. Dies ist beabsichtigt.
- CI deckt `memfuse-py` über separate `--manifest-path`-Aufrufe ab (`cargo clippy --manifest-path crates/memfuse-py/Cargo.toml`, `cargo test --manifest-path crates/memfuse-py/Cargo.toml`, `maturin build --manifest-path crates/memfuse-py/Cargo.toml`).
- Zukünftige Bearbeiter dürfen `memfuse-py` NICHT in die `members`-Liste der Root-`Cargo.toml` aufnehmen, ohne diesen ADR explizit zu widerrufen (P6).

## Alternativen (verworfen)
- Cargo-Profil-Override pro Crate: nicht möglich, Panic-Strategie ist workspace-weit in Cargo, nicht crate-weit überschreibbar.
- `#[panic_handler]`-Custom-Handler statt Workspace-Trennung: löst das `catch_unwind()`-Problem an der FFI-Grenze nicht, da die Panic-Strategie bereits zur Kompilierzeit workspace-weit fixiert wird.

---

# ADR-065: Duplicate Symbol CI-Gate zur Prävention von Merge-Kollisionen

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:** P0-Build-Blocker nach parallelen Commits (`307df50` und `eb0e3ef`), bei denen zwei unabhängige Branches dieselben Top-Level-Konstanten (`DISKANN_FOOTER_MAGIC`, `DISKANN_INTEGRITY_KEY`) in `crates/memfuse-index/src/diskann.rs` einfügten. Da die Diffs nicht überlappten, erzeugte Git keinen Merge-Konflikt, führte jedoch zu E0428-Kompilierfehlern.

## Entscheidung
Es wird ein leichtgewichtiges, regex-basiertes Pre-Build Gate `cargo run -p xtask -- check-duplicate-symbols` eingeführt und in die CI-Pipeline (`.github/workflows/rust-ci.yml` und `.github/workflows/context-gates.yml`) integriert.

## Funktionsweise
1. **Quelltext-Analyse ohne Compiler-Durchlauf:** Scannt `.rs`-Dateien (aus `git diff` oder Workspace) mit Regex-Mustern auf Zeilenebene nach Top-Level-Deklarationen (`const`, `static`, `struct`, `enum`, `fn`, `trait`, `type`).
2. **Top-Level Isolation:** Berücksichtigt nur Deklarationen ohne führende Einrückung und außerhalb von `impl`- / Struct-Blöcken (`brace_depth == 0`), um False Positives bei gleichnamigen Methoden unterschiedlicher Typen zu vermeiden. Wildcards (`const _`) werden ignoriert.
3. **Feature-Gate Sensitivität:** Unterscheidet Symbole unter abweichenden `#[cfg(...)]`-Attributen.
4. **Fast Pre-Build Gate:** Läuft in CI **vor** `cargo check` / `cargo build`, um Fehler unmittelbar mit präzisen Datei:Zeile-Angaben zu melden.

## Grenzen & Einschränkungen
- Kein Ersatz für `cargo check` / `cargo build`, sondern schnelles Vorab-Gate.
- Makro-generierte Symbole werden nicht expandiert (bewusste Entscheidung gegen Voll-Parsing via `syn` für maximale Ausführungsgeschwindigkeit).

---

# ADR-066: Activation of Feature Flag physio-resonance-fusion for Feature F-09

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:** In `crates/memfuse-db/src/fusion.rs` war der Resonanz-Kohärenz-Bonus (Feature F-09) vollständig hinter `#[cfg(feature = "physio-resonance-fusion")]` implementiert (`apply_resonance_bonus`, `ResonanceConfig` und zugehörige Unit-Tests). Das Feature-Flag `physio-resonance-fusion` fehlte jedoch im `[features]`-Block von `crates/memfuse-db/Cargo.toml`. Der Code war somit in allen Feature-Kombinationen unerreichbar (toter Code aufgrund einer Governance-Lücke).

## Entscheidung
1. Das Feature-Flag `physio-resonance-fusion = []` wird in `crates/memfuse-db/Cargo.toml` unter `[features]` ergänzt.
2. Gemäß Invariante P12 ("Physio-Feature-Default-Unsichtbarkeit") verbleibt `physio-resonance-fusion` standardmäßig inaktiv (Zero-Config-Setup).
3. F-09 gilt nach der Deklaration und verifizierten grünen Unit-Tests als **aktivierbar**, jedoch **nicht automatisch als produktiv kalibriert** (Kalibrierung von Exponent β und Gamma γ erfolgt in nachgelagerten Experimenten).

## Konsequenzen
- **Kompilierung & Verifikation:** `cargo check -p memfuse-db --features physio-resonance-fusion` und die Test-Suite laufen unter dem aktivierten Flag vollständig grün ab.
- **Default-Verhalten:** Ohne das Flag bleibt das Verhalten der Reciprocal Rank Fusion (RRF) exakt unverändert.
- **Governance:** Behebt die Governance-Lücke durch konsistente Deklaration in `Cargo.toml`.

---

# ADR-069: Standard-Terminologie statt biologischer Metaphern und Anbieter-Branding

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Autoren:** Principal Architect / Core Engineering Team
* **Kontext / Referenzen:** Refactoring-Phase P1–P5, ADR-005, ADR-020, ADR-066

## Kontext

Biologische Metaphern (z. B. "physio", "synaptic", "immune", "thermostat", "sleep cycle", "nucleation") führten in der Vergangenheit zu unnötiger kognitiver Last für neue Entwickler und erweckten den unzutreffenden Eindruck bionischer oder neuromorpher Systeme, obwohl es sich um mathematisch und algorithmisch präzise definierte Retrieval-, Indexierungs- und Datenbank-Komponenten handelt.

Gleichzeitig führt herstellerbezogenes Branding in Typnamen, Modulbezeichnungen oder Architekturbezeichnungen zu sachlich unzutreffenden Zuschreibungen, Verwirrung bezüglich Systemgrenzen und unbewussten Vendor-Lock-in-Assoziationen.

Im Rahmen der Refactoring-Phasen P1–P5 wurden diese Bezeichnungen systematisch bereinigt. Dieses ADR fixiert die resultierende Namens-Norm als verbindliche Vorgabe für alle zukünftigen Code-Beiträge, Architektur-Dokumente und Pull Requests.

## Entscheidung

Sämtliche zukünftige Beiträge im MemFuse-Workspace müssen sich an die nachfolgenden Normen für Typen, Feature-Flags und Architektur-Labels halten.

### 2.1 Typen & Schnittstellen

| Vorher (Metapher / Branding) | Nachher (MemFuse Norm) | Beschreibung / Funktion |
|---|---|---|
| `SynapticEdge` | `WeightedEdge` | Gewichtete Graph-Kante mit Vertraulichkeits- und Relevanz-Scores |
| `ThermostatDecay` / `FreeEnergyThermostat` | `AdaptiveDecay` | Dynamischer Abklingmechanismus für Speicherpunkte basierend auf Zugriffsintervallen |
| `ImmuneSuppression` / `ImmunMemory` | `GraphEdgeFilter` / `NodeSuppression` | Filterung und temporäre Unterdrückung widersprüchlicher Wissensgraphen-Kanten |
| `SleepCycleEngine` | `ConsolidationEngine` | Periodische Hintergrund-Konsolidierung, Index-Schnitt und Community-Synthese |
| `CognitiveOS` | `MemFuse Agentic Memory Engine` | Orchestrierung von Kontext, Langzeitspeicher und Werkzeugschnittstellen |
| `NucleationPruning` | `TombstonePruning` | Bereinigung gelöschter HNSW-Vektorknoten während Index-Rebuilds |

### 2.2 Feature-Flags

| Alt / Veraltet (`physio-*`) | Neues Norm-Feature-Flag | Verwendungsbereich |
|---|---|---|
| `physio-features` | `adaptive-decay-control` / `adaptive-decay` | Steuerung dynamischer Abklingungsfunktionen in `memfuse-db` |
| `physio-replicator-weights` | `adaptive-rrf-weights` | Kalibrierung von Multiplicative-Weights für RRF Fusion |
| `physio-synaptic-edges` | `weighted-graph-edges` | Aktivierung gewichteter Kanten im Wissensgraphen |
| `physio-percolation` | `graph-percolation` | Aktivierung von Graph-Perkolations-Algorithmen |
| `physio-resonance-fusion` | `resonance-fusion` | Resonanz-Kohärenz-Bonus bei Hybrid-Retrieval |
| `physio-nucleation` | `partial-rebuild-pruning` | Experimentelles Pruning gelöschter Vektorknoten |

### 2.3 Architektur-Labels & Muster

| Anbieter-Branded / Metaphorisches Label | MemFuse Norm-Bezeichnung | Anwendungsfall |
|---|---|---|
| Provider-Branded Retrieval / Anthropic Contextual Retrieval | MemFuse Context-Prefix Retrieval Pattern | Anreicherung von Dokumentenchunks mit Kontext-Präfixen vor Embedding |
| Provider-Branded Routing / Conformal Cascade | MemFuse Conformal SLM Routing Pattern | Kalibrierte Modell-Auswahl und Kaskaden-Routing basierend auf Konfidenzen |
| Cognitive Memory Architecture | MemFuse Agentic Memory Engine | Multi-Layer-Architektur für lokale KI-Agenten-Speicherverwaltung |
| Bi-Temporal Knowledge Graph | MemFuse Bi-Temporal Graph Pattern | Zeitreihen- und Erfassungszeit-Tracking in Wissensgraphen |
| Unlearning / Deletion Proof | MemFuse Deletion Proof Pattern | GDPR Art. 17 konforme, kryptographisch nachweisbare Datenlöschung |

## Konsequenzen

1. **Verbot neuer `physio-*`-Feature-Flags:** Neue Beiträge dürfen unter keinen Umständen neue `physio-*`-Feature-Flags in `Cargo.toml`-Dateien oder bedingten Kompilierungsattributen (`#[cfg(feature = "...")]`) einführen. Bestehende historische Flags werden schrittweise gemäß Deprecation-Prozess migriert.
2. **Standardisierte Musterbezeichnungen:** Neue Architektur-Patterns, Dokumentationsabschnitte und Entwurfsmuster werden ausschließlich in der Form `"MemFuse [Funktion] Pattern"` bezeichnet. Anbieter-Namen oder vergleichendes Provider-Branding dürfen nicht als Namenspräfix für Repositorium-eigene Muster verwendet werden.
3. **Ausnahme für faktische Integrationsreferenzen:** Ausdrücklich von dieser Norm ausgenommen sind faktische, technisch erforderliche Schnittstellen- und Integrationsbezeichner. Dazu zählen:
   - Reale MCP-Client-Identifikatoren (z. B. `"Claude Desktop"`, `"VS Code MCP Host"`),
   - Reale Modell-IDs und Gewichts-Referenzen (z. B. `"bge-reranker-base"`, `"nomic-embed-text"`),
   - Protokoll-Standard-Spezifikationen (z. B. Model Context Protocol / MCP, JSON-RPC 2.0).
   Diese stellen keine herstellerbezogene Attribution von MemFuse-Architektur-Mustern dar, sondern sind funktionale Notwendigkeiten für Interoperabilität.

---

# ADR-070: F-02 Scope-Abgrenzung — Reines Tombstone-Pruning vs. Ursprüngliches Veto

## Status
Final (2026-09-08)

## Kontext
Im Feature-Veto-Register (`VETOES.md`) verbietet VETO-F02 das partielle Rebuilding von HNSW-Teilgraphen ("Partial HNSW Rebuild" / "Nucleation"). Die Rationale des ursprünglichen Vetos stützt sich auf zwei Hauptrisiken:
1. **Recall-Kollaps durch aktives Re-Wiring:** Die algorithmische Neuverdrahtung von Nachbarschaftskanten in einem lokalen Teilgraphen ohne globale Delaunay-Neukalibrierung zerstört die Navigierbarkeit zu entfernten Randknoten.
2. **RwLock-Contention:** Aktive Graphmodifikationen (Hinzufügen neuer Kanten, Kanten-Heuristiken) unter hoher Last führen zu Sperrkonflikten auf Knotenebene.

In `crates/memfuse-index/src/hnsw.rs` existiert die Funktion `rebuild_region()`. Es bestand eine offene Prozesslücke bezüglich der Frage, ob `rebuild_region()` gegen VETO-F02 verstößt. Eine genaue Code-Analyse zeigt:
`rebuild_region()` (Zeilen 1812–1865) führt **ausschließlich reines Tombstone-Pruning** durch:
- Es werden lediglich existierende Referenzen auf als gelöscht markierte Knoten aus den Kantenlisten aktiver Nachbarn entfernt (`conns.retain(|neighbor_id| !tombstoned_set.contains(neighbor_id))`).
- Es findet **keinerlei aktives Re-Wiring** (Suche neuer Ersatznachbarn oder Einfügen neuer Delaunay-Kanten) statt.

## Entscheidung
1. **Formale Scope-Abgrenzung:** Reines Tombstone-Pruning (Entfernen toter Referenzen ohne Neuverdrahtung von Nachbarschaftskanten) ist **NICHT** vom ursprünglichen VETO-F02 erfasst, da es keine Kantenverbindungen verändert oder neu aufbaut, sondern lediglich ungültige Speicherzeiger/IDs bereinigt.
2. **Feature-Gating & Safety-Guard:** Obwohl reines Tombstone-Pruning algorithmisch sicher bezüglich RwLock-Mutationen ist, birgt das Entfernen von Kanten ohne Ersatz das verbleibende Risiko eines Grad-Verlusts (Reduzierung der Kantenanzahl pro Knoten). Daher bleibt das Feature `partial-index-rebuild` (sowie die Nucleation-Steuerung `partial-rebuild-pruning`) **non-default** und darf erst für den Produktionseinsatz freigegeben werden, wenn die Stabilität der Recall-Werte nachgewiesen ist.

## Konsequenzen
- **Technischer Nachweis der Recall-Stabilität:** Der Nachweis, dass `rebuild_region()` den Recall nicht unzulässig degradiert, wird automatisiert über den Regressionstest `crates/memfuse-index/tests/partial_rebuild_recall_regression.rs` (`test_nucleation_recall_regression`) geführt.
- Der Test verifiziert, dass:
  1. `rebuild_region()` den Recall@10 gegenüber reinem Tombstone-Markieren um nicht mehr als 5 Prozentpunkte (5pp) verschlechtert.
  2. Der absolute Recall-Verlust gegenüber dem unveränderten Index unter 15 Prozentpunkten (15pp) bleibt.
- Das Feature bleibt hinter dem Cargo-Feature-Gate `partial-index-rebuild` isoliert.

## enforced_by
- `crates/memfuse-index/src/hnsw.rs:1812` (`pub async fn rebuild_region`)
- `crates/memfuse-index/Cargo.toml` (`[features] partial-index-rebuild = []`)
- `crates/memfuse-index/tests/partial_rebuild_recall_regression.rs` (`test_nucleation_recall_regression`)

---

# ADR-071: Additive Härtung der TenantId-Konstruktoren zur Erzwingung von INV-TENANT-1

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** K12 aus Gesamtspezifikation v7.0 (Sicherheitsinvariante INV-TENANT-1)

## Kontext & Problemstellung
In `crates/memfuse-core/src/types/domain.rs` ist die Invariante **INV-TENANT-1** definiert:
> `TenantId(0)` ist ausschließlich für `TenantId::SYSTEM` reserviert. `TenantId::try_new(0)` liefert `Err(MemFuseError::InvalidInput)`.

Bisher existierten jedoch die ungeschützte `const fn` `TenantId::new(id: u64)` sowie `impl From<u64> for TenantId`, die den Parameter `id` direkt in `Self(id)` verpackten ohne den Guard aus `try_new()` auszuführen. Dadurch konnten Aufrufer im Workspace `TenantId::new(0)` oder `TenantId::from(0u64)` nutzen und so die Sicherheitsinvariante INV-TENANT-1 unterlaufen (K12). Zudem bestanden `TenantId::DEFAULT` und `TenantId::INVALID` als Aliase für `Self(0)`, was zu semantischer Mehrdeutigkeit führte.

Ein sofortiger Umbau aller Call-Sites oder das Entfernen von `new()` / `From<u64>` würde jedoch ein Breaking Change bedeuten und zu Merge-Konflikten mit parallel laufenden Tasks in anderen Crates führen.

## Entscheidung
Wir wählen eine **additive, zweistufige Härtungsstrategie**:

### Stufe 1 (Dieser PR): Additive Deprecation & Typ-Erweiterung
1. **`TenantId::new()` Deprecation:** `TenantId::new()` wird mit `#[deprecated(since = "0.1.0", note = "...")]` markiert.
2. **Sentinel-Konstanten Deprecation:** `TenantId::DEFAULT` und `TenantId::INVALID` werden mit `#[deprecated(note = "Identisch zu TenantId::SYSTEM — nutze SYSTEM für Klarheit.")]` markiert.
3. **Additive `TryFrom<u64>` Implementierung:** `impl TryFrom<u64> for TenantId` wird als normativer, fehlerbehafteter Konvertierungspfad eingeführt (`Self::try_new(id)`).
4. **`From<u64>` Deprecation:** `impl From<u64> for TenantId` bleibt ohne Breaking Change bestehen, wird jedoch mit `#[deprecated(note = "...")]` markiert.
5. **Call-Site-Inventarisierung:** Alle Deprecation-Warnungen im Workspace werden inventarisiert, um die Grundlage für die spätere Migration zu schaffen.

### Stufe 2 (Folge-PR): Call-Site Migration & API-Entfernung
In einem separaten Task werden alle inventarisierten Aufrufer im Workspace auf `TenantId::try_new()`, `TenantId::try_from()` oder `TenantId::SYSTEM` umgestellt. In einer künftigen Major-Version werden `new()` und `From<u64>` vollständig entfernt.

## Konsequenzen & Sicherheitsgarantien
- **Rückwärtskompatibilität:** Bestehender Code kompiliert weiterhin ohne Breaking Changes.
- **Sicherheits-Transparenz:** Neue oder geänderte Aufrufe lösen Compiler-Warnungen aus und machen unsichere Konstruktionen sofort sichtbar.
- **Isolationsgarantie:** Ermöglicht schrittweise Migration aller Workspace-Crates ohne Risiko paralleler Merge-Konflikte.

---

# ADR-072: KV-Bridge Increment 2 — KvSegment-Verschlüsselung, ModelFingerprint und RoPE-Offset (K14)

- **Status:** Akzeptiert
- **Datum:** 2026-09-08
- **Autoren:** Jules (Senior Software Engineer)
- **Kontext / Referenzen:** Gesamtspezifikation v7.0 §7.3, K14, P9 ("Kein Klartext-Sensitivspeicher"), P12 ("Kein sichtbares Verhalten im Zero-Config-Default").

## Kontext und Problemstellung

In Increment 1 der `memfuse-kv-bridge` (Prompt 3) wurde das In-Memory-Zeroize-Sicherheitsfundament gelegt (`KvSegment` mit `ZeroizeOnDrop`, atomare Logical-Clock für echtes LRU, dedizierter `EvictionWorker`). Segmente wurden jedoch rein als Klartext-Tensorbytes (`data: Vec<u8>`) im RAM gehalten.

Gemäß Gesamtspezifikation v7.0 (K14 / §7.3) erfordert Increment 2 die Möglichkeit, KV-Cache-Segmente optional mittels AES-256-GCM-SIV (`memfuse_crypto::KvSegmentCipher`) zu verschlüsseln, ohne die bestehende öffentliche API im Zero-Config-Default zu brechen (P12). Zusätzlich sollen `model_fingerprint: Option<ModelFingerprint>` und `rope_offset: Option<usize>` strukturell in `KvSegment` verankert werden.

## Entscheidungen

1. **Feature-Flag `kv-encryption` & Zero-Config-Default (P12):**
   - Das Crate `memfuse-kv-bridge` führt ein Feature-Flag `kv-encryption = ["dep:memfuse-crypto"]` ein.
   - Im Zero-Config-Default (Feature inaktiv) verhält sich `KvSegment` exakt wie bisher (Klartext-Speicherung, Zeroize-on-Drop, keine zusätzliche Laufzeit-Crypto-Overheads).

2. **Kryptographische Mandanten- und Modell-Isolation (K14 / P9):**
   - Bei aktivem Feature `kv-encryption` bietet `KvSegment` Konstruktoren `new_encrypted()` sowie `TenantIsolatedKvStore::insert_encrypted_segment()` und `get_decrypted_segment()`.
   - Die Verschlüsselung nutzt `KvSegmentCipher` aus `memfuse-crypto` mit AES-256-GCM-SIV und frischen `OsRng`-Nonces.
   - In die Sub-Schlüsselableitung (HKDF-SHA256 via `KeyManager`) fließen `tenant_id` und `model_fingerprint` ein, womit Vertraulichkeit und strikte Isolation auf Modell- und Mandantenebene durchgesetzt werden.

3. **Einbindung von `rope_offset`:**
   - `KvSegment` erhält das Feld `rope_offset: Option<usize>`.
   - Sofern ein Aufrufer (z.B. `memfuse-mcp`) diesen Offset noch nicht liefert, wird `None` übergeben. Dies wird als expliziter Folgepunkt dokumentiert, anstatt einen erfundenen Platzhalterwert vorzutäuschen.

4. **Kombinierte Zeroize- und Speicherabbild-Garantie (Integrationstest):**
   - Ein Integrationstest (`tests/kv_encryption_integration.rs`) simuliert eine In-Memory-Prozessabbild-Inspektion und weist nach, dass der Rohspeicher des Segments zu keinem Zeitpunkt den Klartext-Tensor enthält. <!-- doc-ref-ignore -->
   - Der Test bestätigt zudem, dass `Zeroize::zeroize` nach dem Entschlüsseln alle Puffer im Speicher rückstandslos wischt.

## Konsequenzen

### Positiv
- K14 aus der Gesamtspezifikation v7.0 ist als "Increment 2 abgeschlossen" erfüllt.
- P9 ("Kein Klartext-Sensitivspeicher") ist nun auch für den optional verschlüsselten In-Memory- und Persistenzpfad garantiert.
- Vollständige Abwärtskompatibilität ohne Breaking Changes an bestehenden Downstream-Crates.
- Sämtliche Tests aus Prompt 3 (LRU-Eviction, Zeroize-on-Drop, Tenant-Isolation) bleiben zu 100 % grün.

### Folgepunkte
- Sobald `memfuse-mcp` RoPE-Positioning verarbeitet, kann der übergebene `rope_offset`-Wert direkt an `KvSegment::new_with_metadata()` bzw. `new_encrypted()` durchgeschleift werden.

---

# ADR-073: GASP Post-Hoc Halluzinations-Validator (Initiale Implementierung K19)

* **Status:** Akzeptiert (Schließung von K19 als "H2 — initiale Implementierung")
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** K19 aus Gesamtspezifikation v7.0, P8-Kalibrierungsregel, P10-Reuse-Prinzip, P12-Default-Feature-Gating.

## Kontext & Problemstellung
GASP (Grounding-Aware Sensitivity by Perturbation / Post-Hoc-Validator) wurde in der Produktvision und der Gesamtspezifikation (K19) als essenzielle Verteidigungslinie gegen LLM-Halluzinationen konzipiert.
Bisher existierte im Workspace nur ein präventiver Halluzinations-Guard in `crates/memfuse-ollama/src/client.rs`, der das Sprachmodell vorab via Prompt-Constraints zur Kontexttreue instruiert.

Ein präventiver Guard kann jedoch nicht post-hoc verifizieren, ob eine bereits generierte LLM-Antwort tatsächlich durch die abgerufenen Kontext-Chunks belegt ist. Es fehlte ein eigenständiges Modul `gasp.rs`, das nachgelagert Antworten auf Tatsachenbehauptungen (insbesondere Zahlen und Fakten) prüft und bei unzureichender Belegung kontrolliert absteniert.

## Entscheidung
Wir implementieren das neue Modul `crates/memfuse-candle/src/gasp.rs` mit der Struktur `GaspValidator` unter folgenden Architektur- und Entwurfsentscheidungen:

1. **Klare Trennung der Verteidigungslinien (Prevention vs. Post-Hoc):**
   - Der bestehende präventive Guard in `memfuse-ollama` bleibt unverändert bestehen.
   - `GaspValidator` ergänzt die Pipeline als unabhängiger, nachgelagerter Post-Hoc-Check.

2. **Entkoppelte Trait-Grenze in `memfuse-core`:**
   - In `crates/memfuse-core/src/traits/mod.rs` wird der Trait `GroundingValidator` sowie die Datenstruktur `GroundingAssessment` definiert.
   - `GaspValidator` implementiert `GroundingValidator` und hat keine direkte Abhängigkeit von Layer-2/3-Fachcode (`memfuse-db`).

3. **P8-Konforme Kalibrierung & Wiederverwendung:**
   - `GaspValidator` nutzt den bestehenden `IsotonicCalibrator` und `ConfigFingerprint` aus `memfuse-calibration` (P8/P10).
   - Bei Konfigurationsänderungen (z.B. Modell- oder Quantisierungswechsel) wird die Kalibrierung via `invalidate_on_config_change` zurückgesetzt.

4. **Explizites Abstention-Muster:**
   - Fällt der Konfidenz-Score unter den Schwellenwert (`threshold`, Default: 0.70), löst `GaspValidator` einen Abstention-Pfad aus (`Err(MemFuseError::PolicyViolation(...))` mit `LowConfidenceGrounding`).
   - Leerer Kontext (Zero-Shot) liefert ein definiertes Fehlersignal (`Err(MemFuseError::InvalidInput(...))`) ohne Panic.

5. **P12-Feature-Gating:**
   - `gasp.rs` wird in `crates/memfuse-candle/Cargo.toml` hinter das Feature `candle` ge-gated (`#[cfg(feature = "candle")]`). Ohne Opt-in bleibt das Modul unsichtbar.

## Verbleibender Weg zur vollen Produktionsreife
Mit dieser Implementierung wird **K19 als "H2 — initiale Implementierung"** geschlossen. Für die vollständige Produktionsreife (H3 / Produktionsstufe) sind folgende weitere Schritte erforderlich:

1. **Benchmark-Validierung:** Evaluation des `GaspValidator` gegen reale Halluzinations-Benchmark-Datensätze (z.B. LongMemEval, HaluEval).
2. **Log-Likelihood Integration:** Erweiterung um direkte Token-Logit / Perplexitäts-Vergleiche, sobald Candle KV-Cache / Log-Likelihood Expose-APIs vollständig angebunden sind.
3. **End-to-End Orchestrierung:** Anbindung an `memfuse-mcp` Serving-Pipelines als konfigurierbare Post-Processing Middleware.

## Konsequenzen & Garantien
- **K19 geschlossen:** Das Fehlen von `gasp.rs` ist behoben.
- **Null-Regression:** Keine Änderungen an `memfuse-ollama` oder bestehenden Inferenzpfaden.
- **Typen-Integrität:** Vollständige Testabdeckung für unterstützte, halluzinierte und leere Kontext-Szenarien.

---

# ADR-074: Normative Kalibrierung des PathRAG Sufficiency-Gate Thresholds

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:**
  Ein Codebase-Audit deckte eine ungeklärte Diskrepanz des PathRAG Sufficiency-Gate-Schwellenwerts (`sufficiency_threshold`) auf. In `crates/memfuse-graph/src/path_rag.rs:52` war der Default-Preset in `PathRAGEngine::with_defaults()` auf `0.01` gesetzt, während im Typen-Modul `crates/memfuse-core/src/types/saos.rs:29` sowie in mehreren Testfixtures in `memfuse-db` Werte von `0.1` bzw. `0.5` angegeben waren. Anerschwert wurde die Lage dadurch, dass ein zu niedriger Schwellenwert (z.B. 0.01) laut Forschungsergebnissen zu MemGraphRAG (arXiv:2506.00610) das Risiko birgt, dass minderwertige Multi-Hop-Pfade ungefiltert in die RRF-Signal-Fusion einfließen und einen Precision-Kollaps auslösen.

## Empirische Messergebnisse (Parameter-Sweep via `memfuse-bench`)

Zur fundierten Entscheidung wurde mit `cargo run -p memfuse-bench -- pathrag-sweep` ein Parameter-Sweep über `sufficiency_threshold ∈ {0.01, 0.1, 0.3, 0.6}` auf den Benchmark-Suiten LongMemEval (31 Szenarien) und LoCoMo gefahren.

### LongMemEval Results (31 Szenarien)
| Threshold | Recall@5 | Recall@10 | Precision@5 | Precision@10 |
|-----------|----------|-----------|-------------|--------------|
| **0.01**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.10**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.30**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.60**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |

### LoCoMo Results (2 Szenarien)
| Threshold | Recall@5 | Recall@10 | Precision@5 | Precision@10 |
|-----------|----------|-----------|-------------|--------------|
| **0.01**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.10**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.30**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.60**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |

## Entscheidung
1. **Normativer Default-Wert:** `DEFAULT_SUFFICIENCY_THRESHOLD` wird normativ auf **`0.1`** (10% minimale Pfad-Konfidenz) in `crates/memfuse-graph/src/path_rag.rs` festgelegt.
2. **Konstruktor-Preset:** `PathRAGEngine::with_defaults()` verwendet `DEFAULT_SUFFICIENCY_THRESHOLD` (0.1) statt bisher `0.01`.
3. **Risikovermeidung:** Obwohl in synthetischen Testkorpora hohe Kantengewichte den Recall über alle Thresholds konstant halten, schützt der Wert `0.1` im Realeinsatz auf dichten Graphen wirksam vor Rauschen und Precision-Einbußen durch schwache Multi-Hop-Pfade (arXiv:2506.00610).
4. **Regressionstest:** Ein automatisierter Invarianten-Test (`test_default_sufficiency_threshold_meets_minimum_bound`) garantiert, dass `DEFAULT_SUFFICIENCY_THRESHOLD` künftig nicht unter 0.10 fällt.

## Wissenschaftlicher & Spezifikationskontext
- **PathRAG (arXiv:2502.14902, AAAI 2026):** Bidirektionale Pfadsuche mit Sufficiency-Gate.
- **MemGraphRAG (arXiv:2506.00610):** Precision-Kollaps-Vermeidung durch strenge Relevanzschwellen in Graph-Multi-Hop-Traversierungen.

---

# ADR-075: PID-Regler min_pool_size Kalibrierung und Default-Konsolidierung

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Anforderung / Referenz:** Gesamtspezifikation v7.0 §B.4, Technische Schulden A.8, arXiv:2604.01733

## Kontext & Problemstellung

In `crates/memfuse-calibration/src/pid.rs` steuert `PidController` dynamisch die Kandidatenpool-Größe für das Reranking zur Einhaltung des Latenzbudgets. Das Feld `min_pool_size` besaß im Quellcode unvollständig dokumentierte Werte und eine Inkonsistenz:

1. `PidController::default()` definierte `min_pool_size: 10`.
2. In Teststrukturen und partiellen Overrides existierten unbegründete Magic Numbers (`min_pool_size: 20`).

Keiner dieser Werte verfügte über eine dokumentierte empirische Grundlage. Die Gesamtspezifikation v7.0 §B.4 und die Studie arXiv:2604.01733 berichten jedoch, dass stabile Recall@5-Werte (0.888) erst ab einer Kandidatenpool-Größe von mindestens 100 erreicht werden. Ein zu kleiner Pool (10 oder 20) beeinträchtigt die Retrieval-Qualität drastisch, während ein zu großer Pool das p95-Latenzbudget überschreiten kann. Die Diskrepanz zwischen Quellcode-Defaults und Literaturbefunden stellte eine unzureichend dokumentierte Abweichung dar (Schuld A.8).

## Entscheidung

1. **Konsolidierung des Produktions-Defaults auf `PID_MIN_POOL_SIZE_DEFAULT = 50`:**
   Wir setzen den Default-Wert für `min_pool_size` in `PidController::default()` auf einen konservativen Mittelwert von 50 über die explizit publizierte Konstante `pub const PID_MIN_POOL_SIZE_DEFAULT: usize = 50;`.
2. **Begründung für den Übergangswert 50:**
   - **Verbesserung gegenüber 10/20:** Der Wert 50 liegt deutlich näher an der Literatur-Empfehlung ($\ge 100$) und verhindert drastische Recall-Einbrüche bei niedrigen Latenzen.
   - **Latenz-Schutz:** Der Wert bleibt vorerst unter 100, um eine Überlastung der p95-Reranking-Latenz auf ressourcenbeschränkten Systemen zu vermeiden, bis empirische Messungen auf MemFuse-Korpora vorliegen.
   - **Geltung bis Benchmark-Sweep (B.6):** Dieser ADR fixiert den Übergangsdefault. Ein anstehender Benchmark-Sweep via `memfuse-bench` (LongMemEval) über $min\_pool\_size \in \{10, 20, 50, 100\}$ wird die finale Pareto-Front zwischen Recall@5 und p95-Latenz ermitteln und den Default bei Bedarf via Folge-ADR anpassen.
3. **Beseitigung von Magic-Number-Literalen:**
   Der Default in `PidController::default()` nutzt ausschließlich `PID_MIN_POOL_SIZE_DEFAULT` und `PID_MAX_POOL_SIZE_DEFAULT`. Test-Overrides in Unit-Tests wurden explizit als solche kommentiert.

## Konsequenzen

- `PidController::default().min_pool_size` ist nun einheitlich 50.
- Im Crate `crates/memfuse-calibration` existieren keine undokumentierten Magic-Number-Produktions-Defaults für `min_pool_size`.
- **Follow-up (B.6):** Ein empirischer LongMemEval-Benchmark-Sweep zur Bestimmung des exakten Pareto-Optimums ($min\_pool\_size \in \{10, 20, 50, 100\}$) ist für das nächste Ingestion/Retrieval-Release einzuplanen.

---

# ADR-076: Studie zur DiskANN PENDING_FLUSH_THRESHOLD Write-Amplification und Empfehlung für adaptiven Schwellenwert

* **Status:** Akzeptiert (Empfehlung normativ festgehalten, Implementierung folgt in separatem Task)
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** Gesamtspezifikation v7.0 §4.2, Technische-Schulden-Dokument A.9

## Kontext & Problemstellung

In `crates/memfuse-index/src/diskann.rs` legt die Konstante `PENDING_FLUSH_THRESHOLD: u64 = 50` fest, nach wie vielen uncommitted Vektoreinfügungen im WAL/RAM automatisch ein DiskANN `persist_delta()` ausgelöst wird. Dieser Wert wurde ohne begleitenden ADR von einem früheren Wert (1.000) auf 50 gesenkt (Faktor 20 häufigeres Background-Persist bei kleinen Collections).

Die Auswirkung dieser Frequenzänderung auf die Schreibverstärkung (Write-Amplification) und die I/O-Belastung von NVMe/SSD-Speichermedien war bislang undokumentiert und unquantifiziert, was eine Dokumentationslücke gemäß v7.0 §4.2 und Technischen Schulden A.9 darstellte.

## Messmethodik & Empirische Ergebnisse

Über den dedizierten Benchmark `crates/memfuse-index/benches/flush_threshold_amplification.rs` wurden DiskANN-Collections der Größen $N \in \{100, 1.000, 10.000, 100.000\}$ mit Insert-Workloads unter Schwellenwerten $T \in \{50, 200, 1.000\}$ vermessen. Die Ergebnisse sind in `crates/memfuse-index/benches/results/flush_threshold_amplification.md` abgelegt.

### Wichtigste Messergebnisse:
1. **Write Amplification (WA) skaliert direkt proportional zur Collection-Größe $N$ und umgekehrt proportional zum Threshold $T$:**
   - Bei $N=100.000$ führt ein statischer Threshold von $T=50$ zu einer exzessiven Write Amplification von **32.025x** (781,87 MB Festplattenschreiben für 0,02 MB Vektordaten).
   - Eine Erhöhung des Schwellenwerts auf $T=200$ bzw. $T=1.000$ senkt das Schreibvolumen um den Faktor **4,0x bis 20,0x**.
2. **Latenz-Verhalten (p95):**
   - Die p95-Latenz einzelner Inserts wird primär vom WAL-`fsync()` dominiert (~7,1 ms bis 7,7 ms).
   - Bei kleinen Thresholds ($T=50$) erzeugen extrem häufige Hintergrund-Flushes permanente I/O-Konkurrenz und Dateisystem-Renames, was auf I/O-begrenzten Systemen zu Latenzspitzen führt.

## Entscheidung & Normative Empfehlung

Auf Basis der Messdaten lehnen wir einen rein statischen Schwellenwert (weder fest 50 noch fest 1.000) ab und beschließen normativ die Einführung eines **adaptiven, von der Collection-Größe $N$ abhängigen Flush-Thresholds**:

$$\text{PENDING\_FLUSH\_THRESHOLD}(N) = \max\left(50, \min\left(1.000, \left\lfloor N \times 0,05 \right\rfloor\right)\right)$$

### Stufenregelung:
1. **Kleine Collections ($N \le 1.000$):** Threshold = **50**
   - Garantiert minimale Uncommitted-WAL-Länge, schnelle Crash-Recovery und minimale Sichtbarkeitsverzögerung bei geringem absolutem Schreibvolumen.
2. **Mittlere Collections ($N = 10.000$):** Threshold = **500**
   - Reduziert die Write-Amplification von 3.241x auf 324x bei weiterhin überschaubarem Recovery-Fenster.
3. **Große Collections ($N \ge 20.000$):** Threshold = **1.000**
   - Deckelt die Write-Amplification bei großen Vektormengen und schont SSD-/NVMe-Speichermedien vor I/O-Sättigung.

*Hinweis:* Die eigentliche Implementierung der adaptiven Funktion in `crates/memfuse-index/src/diskann.rs` ist bewusst Gegenstand eines separaten Folge-Tasks mit eigenem Code-Review.

## Konsequenzen & Dokumentationsabschluss

- **Schließung der Dokumentationslücke:** Erfüllt die Anforderungen aus Gesamtspezifikation v7.0 §4.2 und beseitigt Technische Schulden A.9.
- **Nachvollziehbarkeit:** Der Benchmark `cargo bench -p memfuse-index --bench flush_threshold_amplification --features experimental-diskann` steht als reproduzierbare Messgrundlage im Repository bereit.

---

# ADR-077: Produktvision PyPI-Library Fokus und Tauri Deprecation

* **Status:** Umgesetzt (physisch entfernt am 2026-09-12)
* **Datum:** 2026-09-08 (Umsetzung: 2026-09-12)
* **Target Path:** crates/memfuse-tauri
* **Kontext / Auslöser:** Zielarchitektur v8.0 §6 & Entscheidungsdokumentation v1.0. Das Projekt führte zuvor drei unentschiedene Produktvisionen parallel (PyPI-Library, Desktop-Enterprise-App, Voice-Assistant).

## Entscheidung
1. **Verbindliche Fokussierung auf Option 1: PyPI-Library (Position A/B, ADR-007-Richtung)**. MemFuse wird primär als hochperformante, kryptographisch isolierte Embedded AI Memory Library für Python (`memfuse-py`) und Rust entwickelt.
2. **ADR-018 (Doppelstrategie) wird explizit durch diese ADR abgelöst (`superseded`)**.
3. **`memfuse-tauri` wird als `deprecated` eingestuft** und im Rahmen des Crate-Konsolidierungs-Fahrplans physisch aus dem Repository entfernt.

## Begründung
- Die Entwicklungsdynamik (Schwarm-Entwicklung, Solo-Architekt) erfordert maximale Fokussierung auf die Kernstärke: kaskadierende Retrieval-Qualität und kryptographische Mandantenisolation.
- Eine Desktop-Enterprise-App bindet erhebliche Ressourcen in UI/Desktop-Packaging (Tauri/GTK), ohne direkten Beitrag zur Inferenz- und Gedächtnisleistung.

## Konsequenzen
- `memfuse-py` bildet die primäre FFI-Grenzschicht.
- `memfuse-tauri` wird in Folgeschritten aus der Cargo-Workspace-Topologie entfernt.
- Doku-Artefakte und README/Architecture-Guides werden entsprechend aktualisiert.

---


# ADR-078: Konsolidierung aller NEW_STRATEGY-Dokumente in GESAMTSPEZIFIKATION_v10.0

* **Datum:** 2026-09-08
* **Status:** ✅ Final
* **Kontext:** `docs/NEW_STRATEGY/` enthielt 12 Strategiedokumente (v4.0–v9.0, Governance-Audit, Entscheidungsdokumente, Kritik) mit partiellen Widersprüchen und einer Gesamtgröße von ~6.000 Zeilen. Kein Dokument war alleine normativ.
* **Entscheidung:** Alle Strategiedokumente werden in `docs/GESAMTSPEZIFIKATION_v10.md` synthetisiert. Das neue Dokument ist die einzige normative Wahrheitsquelle. `docs/NEW_STRATEGY/` verbleibt im Repository als Archiv (read-only, kein Jules-Schreibzugriff), wird aber nicht mehr als Referenz in Prompts verwendet.
* **Konsequenzen:**
  1. Neue Jules-Sessions lesen `AGENTS.md` + `GESAMTSPEZIFIKATION_v10.md` statt `docs/NEW_STRATEGY/*.md`.
  2. `GESAMTSPEZIFIKATION_v10.md` wird bei Änderungen an Crate-Topologie oder Feature-Status per `cargo xtask sync-docs` aktualisiert (nicht manuell).
  3. `docs/NEW_STRATEGY/` bekommt eine `README.md`: "ARCHIV — nicht für neue Sessions verwenden. Normative Spezifikation: docs/GESAMTSPEZIFIKATION_v10.md"
* **Alternativen verworfen:** Einzelne Dokumente updaten statt neu synthetisieren (führt zum selben Versions-Drift-Problem, das zur Notwendigkeit dieser ADR geführt hat).
