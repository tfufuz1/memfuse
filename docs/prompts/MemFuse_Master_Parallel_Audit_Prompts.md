# MemFuse — Master Parallel Audit & Remediation Prompts for Google-Jules

Dieses Dokument stellt ein vollständiges, hochstrukturiertes Set an **modularen, parallel ausfüherbaren Prompts** für Google-Jules (sub-agents) bereit. Jeder Prompt deckt exakt definierte Komponenten und Fehlerklassen aus den Audit-Dokumenten (`docs/audits/*.md`, `docs/audits/round2/*.md`) und der Systemarchitektur ab.

---

## Inhaltsverzeichnis Prompts

1. [Prompt 1: Concurrency, State, Race Conditions & MVCC (`memfuse-core`, `memfuse-db`)](#prompt-1-concurrency-state-race-conditions--mvcc)
2. [Prompt 2: Storage Engine, LSM-Trees & Transaktionen (`memfuse-store`)](#prompt-2-storage-engine-lsm-trees--transaktionen)
3. [Prompt 3: KI-Inferenz, Graph & Vektorsuche (`memfuse-db`, `memfuse-router`, `memfuse-embed`)](#prompt-3-ki-inferenz-graph--vektorsuche)
4. [Prompt 4: Low-Level, Memory, SIMD & OS-Interaktion (`memfuse-index`, `memfuse-store`)](#prompt-4-low-level-memory-simd--os-interaktion)
5. [Prompt 5: Security, Kryptografie & Privacy (`memfuse-crypto`, `memfuse-ollama`)](#prompt-5-security-kryptografie--privacy)
6. [Prompt 6: Architektur, DAG-Constraints & Code Smells (`memfuse-core`, `memfuse-db`, `memfuse-agent`)](#prompt-6-architektur-dag-constraints--code-smells)
7. [Prompt 7: FFI, PyO3 & Cross-Language Interoperability (`memfuse-py`)](#prompt-7-ffi-pyo3--cross-language-interoperability)
8. [Prompt 8: Frontend, IPC & Ingestion-Parser-Robustheit (`memfuse-tauri`)](#prompt-8-frontend-ipc--ingestion-parser-robustheit)
9. [Prompt 9: Suchalgorithmen, RRF & Indizes (`memfuse-text`, `memfuse-index`, `memfuse-graph`)](#prompt-9-suchalgorithmen-rrf--indizes)
10. [Prompt 10: Advanced Concurrency, MVCC & Lock-Management (`memfuse-db`, `memfuse-checkpoint`)](#prompt-10-advanced-concurrency-mvcc--lock-management)
11. [Prompt 11: Kryptografie, Anti-Tamper & Durability (`memfuse-crypto`, `memfuse-store`)](#prompt-11-kryptografie-anti-tamper--durability)
12. [Prompt 12: Testing, CI/CD, Anti-Leakage & Governance (`memfuse-agent`, `memfuse-checkpoint`, `xtask`)](#prompt-12-testing-cicd-anti-leakage--governance)

---

### Prompt 1: Concurrency, State, Race Conditions & MVCC

```markdown
ROLLE
Du bist Principal Senior Rust Architect für Storage- & Concurrency-Engineering mit Spezialisierung auf MVCC, Lock-Granularität, Lock-freie Datenstrukturen, Sharded Transaction Staging und Deadlock-Prävention.

KOMPETENZEN
- Tiefes Verständnis von Rust Borrow Checker, `Sync`/`Send`, `std::sync::atomic`, `parking_lot`, Tokio Mutex/RwLock und `loom`-Interleaving-Testing.
- Formalisierung von Transaktions-Isolationsgarantien und Snapshot-Registry-Pinning.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-core/src/tx_buffer.rs`
- `crates/memfuse-core/src/snapshot.rs`
- `crates/memfuse-db/src/collection/tx.rs`
- `crates/memfuse-db/src/transaction.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Race Condition / Data Race: Undefiniertes oder unkoordiniertes Interleaving bei parallelem Schreib-/Lesezugriff im `TxBuffer`.
2. Lost Update / Phantom Erasure: Gleichzeitiges Überschreiben von Staged Transactions oder fälschliches Löschen geänderter Dokumente durch unkoordinierte Orphan-Reaper-Läufe.
3. Stale Read: Lesen veralteter Datenzustände aus nicht korrekt unpinned Snapshots oder veralteten In-Memory-Caches.
4. Deadlock / Livelock / Starvation: Blockieren von Threads durch falsche Lock-Erwerbsreihenfolge in `transaction.rs`.
5. Lock Poisoning: Kaskadierende Ausfälle nach Panic innerhalb ge-lockter Bereiche.
6. Unbounded Queue: Fehlen von Backpressure im `TxBuffer`, was bei hoher Ingestion-Last zu OOM führt.
7. ABA-Problem: Unbemerkte Zustandsänderung zwischen Prüfen und Ausführen (Check-Then-Act) bei Transaktions-ID-Allokation oder Snapshot-Unpinning.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `tx_buffer.rs` und `snapshot.rs` in `memfuse-core`. Stelle sicher, dass `TxBuffer` Bounded-Backpressure (Obergrenze für Staged Transactions) erzwingt und `MemFuseError::InvalidInput` oder `TransactionAborted` bei Kapazitätsüberlauf zurückgibt.
2. Untersuche `snapshot.rs` auf Stale-Read-Gefahren: Verifiziere, dass `SnapshotRegistry` atomic reference counting nutzt und unpinned Snapshots nicht zu vorzeitigem GC von SSTables führen.
3. Überprüfe die Lock-Hierarchie in `memfuse-db/src/transaction.rs`:
   - Durchsetzung der Reihenfolge: `collections` RwLock -> `embedder` RwLock -> `insert_lock` Mutex.
   - Stelle sicher, dass kein Lock über `.await`-Punkte gehalten wird.
4. Erstelle oder erweitere Concurrency-Stresstests mit 100+ parallelen Tokio-Tasks, die gleichzeitig Transaktionen erzeugen, committen, abbrechen und den Orphan Reaper auslösen.
5. Führe Verifikationstests durch:
   `cargo test -p memfuse-core --lib tx_buffer`
   `cargo test -p memfuse-db --lib transaction`

ABNAHMEKRITERIEN
- Kein Lock wird über `.await`-Punkte hinweg gehalten.
- Bounded Queue/Backpressure im `TxBuffer` ist strikt durchgesetzt.
- Alle Transaktions-Rollbacks hinterlassen byte-identisch saubere Zustände.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_concurrency_race_conditions.md`.
```

---

### Prompt 2: Storage Engine, LSM-Trees & Transaktionen

```markdown
ROLLE
Du bist Principal Senior Rust Storage Engine Architect mit Spezialisierung auf LSM-Trees, Write-Ahead Logs (WAL), SSTables, Compaction, Atomic Commits und Crash Recovery.

KOMPETENZEN
- Tiefgehende Expertise in RocksDB/LevelDB-Architekturen, POSIX-File-I/O, `fsync`-Disziplin, CRC32/BLAKE3 Checksumming, Tombstone Scrubbing und SSTable Index Layout.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-store/src/wal.rs`
- `crates/memfuse-store/src/memtable.rs`
- `crates/memfuse-store/src/sstable.rs`
- `crates/memfuse-store/src/compaction.rs`
- `crates/memfuse-store/src/lsm.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Dirty Read / Non-Repeatable Read: Lesen von uncommitted MemTable-Daten außerhalb des aktiven Transaktions-Snapshots.
2. Atomicity Failure / Missing Rollback: Partieller Schreibzugriff auf SSTables oder WAL bei Prozessabbruch.
3. WAL Corruption / Broken Hash Chain: Beschädigung der WAL-Segment-Checksummen oder unbemerkt gekürzte Records am Log-Ende.
4. Write / Read / Space Amplification: Unverhältnismäßig hoher I/O-Overhead durch ineffiziente Compaction-Triggering.
5. Tombstone Leak / Accumulation: Fehlendes Tombstone-Scrubbing während Level-Compaction führt zu unbegrenztem Disk-Wachstum.
6. Truncation Crash: Absturz durch unvollständig geschriebene SSTables oder leere Datensegmente beim Neustart.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `wal.rs` in `memfuse-store`: Verifiziere das Append-Format, CRC32/BLAKE3-Checksum-Validierung pro Record und die strikte WAL-First-Regel (WAL commit + `fsync` vor MemTable Write).
2. Verifiziere, dass `fsync` (`sync_all()`) bei jedem Commit aufgerufen und Fehler ordnungsgemäß via `Result<T, MemFuseError>` propagiert werden (kein `let _ = sync_all()`).
3. Analysiere `compaction.rs`: Implementiere bzw. verifiziere Tombstone-Garbage-Collection. Stelle sicher, dass Tombstones, die älter als die älteste aktive Snapshot-Pin-TxId sind, bei Compaction physisch entfernt werden.
4. Implementiere Fault-Injection-Tests: Simuliere Bit-Flips und partielle File-Truncation in WAL- und SSTable-Dateien. Bestätige, dass `LsmStorage::repair_on_open()` beschädigte Tails sicher kappt und saubere Daten wiederherstellt.
5. Führe Verifikationstests durch:
   `cargo test -p memfuse-store --all-features`

ABNAHMEKRITERIEN
- WAL-First-Rule und `fsync`-Propagation vollständig nachgewiesen.
- Crash-Recovery-Tests nach File-Truncation und Bit-Flips verlaufen ohne Panics.
- Tombstone-Scrubbing ist durch Unit-Tests abgesichert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_storage_lsm_recovery.md`.
```

---

### Prompt 3: KI-Inferenz, Graph & Vektorsuche

```markdown
ROLLE
Du bist Principal Senior AI Systems & Information Retrieval Architect mit Spezialisierung auf kalibrierte Routing-Systeme, Conformal Prediction, Hybrid Fusion (RRF), Cross-Encoder Reranking und Vektorsuche.

KOMPETENZEN
- Mathematisch präzise Beherrschung von Conformal Calibration, Expected Calibration Error (ECE), RRF Score Normalization, Hybrid Query Building, Graph-Traversierung und RAG-Prompt-Sicherheit.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-router/src/router.rs`
- `crates/memfuse-db/src/fusion.rs`
- `crates/memfuse-db/src/collection/query_builder.rs`
- `crates/memfuse-embed/src/reranker.rs`
- `crates/memfuse-ollama/src/importance.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Truncation-before-Filter: Logikfehler durch Abschneiden der Vektor-/Text-Ergebnismenge *vor* dem Anwenden von Metadaten-Filtern.
2. Recall Collapse / Precision Loss: Einbruch der Genauigkeit durch unpassende Vektor-Distanz-Metriken oder fehlerhafte Scorer-Gewichtung.
3. ECE Spikes / Uncalibrated Confidence: Überkonfidente Router-Scores führen zu Falsch-Escalation oder Oszillation zwischen SLM/LLM.
4. Covariate Shift / Drift-Blindheit: Router reagiert nicht auf veränderte Query-Verteilungen mangels Kalibrierungs-Warmup.
5. Graph Cycle / Traversal Explosion: Unbeschränkte BFS/DFS-Suche oder PPR-Iterationen führen zu Endlosschleifen oder Speicher-Explosion.
6. Signal Loss / Kausalitätsverlust: Fusion verwischt Herkunft, Scores und Metadaten-Subsumtion (Supersedes-Displacement).

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Überprüfe `query_builder.rs` und `fusion.rs` in `memfuse-db`: Stelle sicher, dass Metadaten-Filter *vor* Top-K-Truncation angewendet werden (Filter-before-Truncation Invariante).
2. Analysiere `router.rs` in `memfuse-router`: Verifiziere, dass Conformal Calibration erst nach `CALIBRATION_WARMUP_WINDOW >= 30` Samples aktiviert wird und `min_relevance_score.is_finite()` erzwingt.
3. Überprüfe Post-RRF Supersedes Displacement (ADR-038): Wenn `include_superseded == false`, müssen ersetzte Dokumente sauber gefiltert werden.
4. Überprüfe Static Regex Helpers in `importance.rs`: Verifiziere die Nutzung von `OnceLock` mit `.ok_or_else` zur Einhaltung der Zero-Panic Policy.
5. Führe Verifikationstests aus:
   `cargo test -p memfuse-router`
   `cargo test -p memfuse-db`

ABNAHMEKRITERIUM
- Metadaten-Filterung vor Truncation nachgewiesen.
- Routersystem verarbeitet nur endliche Float-Scores ohne NaN/Inf Propagation.
- Post-RRF Supersedes Filtering verifiziert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_ai_inference_retrieval.md`.
```

---

### Prompt 4: Low-Level, Memory, SIMD & OS-Interaktion

```markdown
ROLLE
Du bist Principal Senior Systems Engineer für Low-Level Memory Safety, OS-Kernel Interaktion, SIMD Vectorization und Memory-Mapped Files.

KOMPETENZEN
- Tiefes Wissen über `unsafe` Rust Invarianten, Alignment, `std::arch`, mmap Slices (`memmap2`), SIGBUS/SIGSEGV Handler, Bounds Checking und Memory Leak Prevention.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-index/src/distance.rs`
- `crates/memfuse-index/src/diskann.rs`
- `crates/memfuse-index/src/persistence.rs`
- `crates/memfuse-store/src/mmap.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. SIGBUS / SIGSEGV: Memory-Access-Violation bei verkürzten oder entladenen mmap-Dateien.
2. Undefined Behavior (UB): Verletzung der Memory-Safety-Garantien in `unsafe`-Blöcken (Alignment, Aliasing, Slicing Out-of-Bounds).
3. Buffer Overflow / Underflow: Out-of-Bounds Zugriffe auf Vektor-Slices oder SIMD-Buffer.
4. File Descriptor / Resource Leak: Fehlende Schließung von File-Handles bei Compaction oder Index-Reload.
5. SIMD Precision Loss: Numerische Instabilitäten durch Unaligned Float-Arrays oder Subnormal-Floats.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `distance.rs` in `memfuse-index`: Überprüfe alle SIMD-Intrinsics (AVX-512, AVX2, NEON, Skalar) auf strikte Vektor-Längen-Validierung.
2. Stelle sicher, dass `cosine_distance`, `euclidean_distance` und `dot_product_distance` bei Vektor-Längen-Fehlanpassung `MemFuseError::EmbeddingDimensionMismatch` zurückgeben und niemals paniken.
3. Überprüfe `diskann.rs` und `persistence.rs`: Ersetze direkte Slice-Indexierungen auf mmap-Regionen durch sichere `.get()`-Checks.
4. Verifiziere den 36-Byte Footer (`b"DONE"` Magic + 32-Byte HMAC-SHA256) beim Laden von DiskANN-Indizes.
5. Führe Verifikationstests aus:
   `cargo test -p memfuse-index --all-features`

ABNAHMEKRITERIEN
- Kein `panic!` oder Out-of-Bounds-Panic bei Dimension-Mismatch oder korrupten mmap-Dateien.
- Unaligned SIMD-Pfade und Safe-Fallback-Routinen vollständig verifiziert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_low_level_memory_simd.md`.
```

---

### Prompt 5: Security, Kryptografie & Privacy

```markdown
ROLLE
Du bist Principal Senior Cryptographic & Systems Security Engineer mit Spezialisierung auf Authenticated Encryption (AES-GCM-SIV), Constant-Time Discipline, Anti-Tamper Chain Validation, GDPR Art. 17 DeletionProofs und Prompt Injection Hardening.

KOMPETENZEN
- Experte für AES-256-GCM-SIV, HKDF Key Derivation, HMAC-SHA256, Constant-Time Comparison (`subtle::ConstantTimeEq`), Zeroization von Secrets und Prompt Escaping.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-crypto/src/crypto.rs`
- `crates/memfuse-crypto/src/anti_tamper.rs`
- `crates/memfuse-crypto/src/deletion_proof.rs`
- `crates/memfuse-ollama/src/client.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Dangling Payload / Incomplete Erasure: Verbleiben von Klartextdaten nach Löschbefehlen (GDPR Art. 17 Verstoß).
2. Timing Attack: Schwachstellen durch nicht-konstante Ausführungszeiten bei HMAC- oder Secret-Vergleichen.
3. Prompt Injection / Output Sanitization Gap: Ungeschützte Durchreichung von KI-Ausgaben oder un-escapten XML-Tags an LLMs.
4. Sandbox Escape: Ausbruch aus isolierten MCP Tool Executions.
5. Broken Chain of Trust: Fehlerhafte Validierung von HMACs oder Key-Derivations in WAL/Index.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `crypto.rs` und `anti_tamper.rs` in `memfuse-crypto`: Verifiziere, dass alle Vergleiche von Keys, Tags und HMACs die Trait-Methode `subtle::ConstantTimeEq` verwenden.
2. Überprüfe `deletion_proof.rs`: Verifiziere INV-DELETION-1 (`LayerCleanupProof::new_after_verified_empty`), das strikt verlangt, dass `remaining_live_entries == 0` ist.
3. Analysiere `build_rag_prompt` und `xml_escape` in `memfuse-ollama/src/client.rs`: Stelle sicher, dass XML-Sonderzeichen (`<`, `>`, `&`, `"`, `'`) konsequent maskiert werden.
4. Prüfe, dass secret Keys und Plaintext-Buffers nach Nutzung via `zeroize` genullt werden.
5. Führe Verifikationstests aus:
   `cargo test -p memfuse-crypto`
   `cargo test -p memfuse-ollama`

ABNAHMEKRITERIEN
- Alle Hash- und Key-Vergleiche sind nachweislich constant-time.
- GDPR Deletion Proof erzwingt physikalische Leere.
- XML-Escaping schützt vor Prompt Injection.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_security_crypto_privacy.md`.
```

---

### Prompt 6: Architektur, DAG-Constraints & Code Smells

```markdown
ROLLE
Du bist Principal Senior Software Architect mit Spezialisierung auf Layered DAG Architecture, Bi-Temporale Datenmodelle, Error Propagation Discipline und Redundanz-Design.

KOMPETENZEN
- Tiefgehende Expertise in Layered DAG Enforcement, Bi-Temporal Axes (Transaction Time vs. Business Time), Centralized Error Mapping und Systemic Refactoring.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-core/src/types.rs`
- `crates/memfuse-core/src/error.rs`
- `crates/memfuse-core/src/error_dto.rs`
- `crates/memfuse-graph/src/csr.rs`
- `crates/memfuse-db/src/lib.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Error Swallowing / Defensive Nulling: Stille Fehlerunterdrückung durch unberechtigte Fallbacks.
2. Layer Leakage / Structural Violation: Verletzung von Layer-Abstraktionen (z.B. Aufwärts-Importe im DAG).
3. Bi-Temporale Achsenverwechslung: Fehlinterpretation von logischen Sequenzen (`TxId`) als physikalische Zeit (`SystemTime`).
4. Premature Optimization: Komplexitätsaufbau ohne empirische Messung.
5. Single Point of Failure (SPOF): Fehlende Redundanz an kritischen Systemknoten.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Überprüfe die DAG-Layer-Architektur (Layer 0 bis Layer 4): Stelle sicher, dass keine zirkulären Abhängigkeiten oder Aufwärts-Importe existieren.
2. Analysiere `Edge` in `memfuse-core` und `memfuse-graph`: Verifiziere die strikte Trennung von Transaction Time (`tx_valid_from`, `tx_valid_to`: `Option<TxId>`) und Business Time (`business_valid_from`, `business_valid_to`: `Option<i64>`).
3. Verifiziere, dass Transaktions-IDs *niemals* von `SystemTime` abgleitet werden, sondern stets via `collection.allocate_tx()` zugewiesen werden.
4. Überprüfe `MemFuseError`: Verifiziere die Zero-Panic-Doktrin (kein `.unwrap()` oder `.expect()` außerhalb von `#[cfg(test)]`).
5. Führe Verifikationstests aus:
   `cargo test -p memfuse-core`
   `cargo test -p memfuse-graph`

ABNAHMEKRITERIEN
- Layer-DAG strikt eingehalten.
- Bi-temporale Achsen vollständig getrennt.
- Zero-Panic Policy in allen Nicht-Test-Codepfaden erfüllt.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_architecture_dag_bitemporal.md`.
```

---

### Prompt 7: FFI, PyO3 & Cross-Language Interoperability

```markdown
ROLLE
Du bist Principal Senior FFI & PyO3 Integration Architect mit Spezialisierung auf Rust/Python Bindings, GIL Lifecycle Management, Multi-Threaded Executor Integration und Cross-ABI Safety.

KOMPETENZEN
- Experte für PyO3, Python Sub-Interpreter, Zero-Copy Buffers, GIL Deadlock Prevention und Safe Exception Mapping (`MemFuseError` -> `PyErr`).

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-py/src/lib.rs`
- `crates/memfuse-core/src/error_dto.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Panic Across FFI Boundary: Unaufgefangener Rust-Panic, der in die Python-Runtime überspringt und den Prozess schlagartig crasht.
2. GIL Deadlock: Blockieren des Rust-Worker-Threads während das Python GIL gehalten wird.
3. Sub-Interpreter State Corruption: Unzulässiges Teilen von globalem Zustand (`OnceLock`) über isolierte Python Sub-Interpreter hinweg.
4. Zero-Copy Lifetime Violation: Use-After-Free durch fehlerhafte Referenzverarbeitung von String- oder Vector-Slices.
5. Type Impedance Mismatch: Präzisionsverlust bei Konvertierung von Rust `u64` IDs zu Python Numbers.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `crates/memfuse-py/src/lib.rs`: Stelle sicher, dass jede öffentlich exponierte PyO3-Funktion mit `pyo3::panic::catch_unwind` oder sauberer `Result<T, PyErr>`-Rückgabe geschützt ist.
2. Überprüfe das GIL-Release-Verhalten: FFI-Methoden, die intensive I/O- oder Such-Operationen durchführen, müssen `py.allow_threads(...)` nutzen, um das GIL freizugeben.
3. Verifiziere, dass `OnceLock`-geteilte Tokio-Runtimes in `memfuse-py` gegen Re-Initialization in Sub-Interpretern abgesichert sind.
4. Überprüfe den Vektor- und ID-Austausch: IDs (`DocId`, `TxId`) müssen als Python `Int` (64-bit unsigned) übertragen werden.
5. Führe Verifikationstests aus:
   `cargo test -p memfuse-py`

ABNAHMEKRITERIEN
- FFI-Grenze garantiert frei von unmitigierten Rust Panics.
- GIL-Release bei async/I/O Operationen nachgewiesen.
- Sauberer Error Mapping Testsuite für Python Exceptions.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_ffi_pyo3_interop.md`.
```

---

### Prompt 8: Frontend, IPC & Ingestion-Parser-Robustheit

```markdown
ROLLE
Du bist Principal Senior Desktop & Ingestion Security Architect mit Spezialisierung auf Tauri IPC, Path Traversal Defense, Resource-Bounded File Parsing und Async Executor Non-Blocking Guarantees.

KOMPETENZEN
- Tiefes Wissen über Tauri Commands, Tokio Blocking Threads, PDF/DOCX Parsing Security, Zip Bomb Defense, XML Entity Expansion Defense und Path Normalization.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-tauri/src/commands/mod.rs`
- `crates/memfuse-tauri/src/ingestion/pdf.rs`
- `crates/memfuse-tauri/src/ingestion/docx.rs`
- `crates/memfuse-tauri/src/ingestion/email.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Blocking the Async Executor: CPU-intensive Dokumenten-Extraktion directly auf Tokio Worker-Threads statt via `spawn_blocking`.
2. Zip Bomb / XML Entity Expansion (Billion Laughs): Verschachtelte/hochkomprimierte Ingestion-Files erzwingen OOM.
3. Path Traversal / Directory Climbing: Ausbrechen aus dem vorgesehenen App-Workspace via `../../` Pfaden.
4. IPC Event Flooding: Frontend-Überlastung durch ungepufferte Progress-Events pro Ingestion-Chunk.
5. Malicious Payload Execution: Extraktion und unbeabsichtigte Interpretation von embedded JS oder Macros.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Überprüfe `validate_path_within_base` in `crates/memfuse-tauri/src/commands/mod.rs`: Stelle sicher, dass Pfad-Traversal absolut verhindert wird.
2. Analysiere `pdf.rs`, `docx.rs` und `email.rs`:
   - Verifiziere `MAX_INGEST_FILE_SIZE_BYTES = 100 MB` Obergrenze.
   - Stelle sicher, dass Extraktionsprozesse in `std::panic::catch_unwind` und `tokio::task::spawn_blocking` gekapselt sind.
3. Verifiziere, dass alle `#[tauri::command]` Funktionen Fehler als `Result<T, MemFuseErrorDto>` zurückgeben und nicht als `Result<T, String>`.
4. Führe Verifikationstests aus:
   `cargo test -p memfuse-tauri`

ABNAHMEKRITERIEN
- Kein Blocking auf Async Tokio Executoren.
- Safe Path Traversal Guards bei allen Tauri File Commands.
- Bounded Memory Ingestion und Error DTO Serialization.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_frontend_ipc_ingestion.md`.
```

---

### Prompt 9: Suchalgorithmen, RRF & Indizes

```markdown
ROLLE
Du bist Principal Senior Search Algorithms & Index Architect mit Spezialisierung auf HNSW Vector Indexes, BM25 Text Search, CSR Graph Retrieval, Hybrid Reciprocal Rank Fusion (RRF) und Numerical Clamping.

KOMPETENZEN
- Experte für Ranking Math, Score Clamping (NaN/Inf Defenses), BM25 $df > N$ Safeguards, HNSW Dimension Handling, Graph Super-Node Traversal Limits und Deterministic Tie-Breaking.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-text/src/bm25.rs`
- `crates/memfuse-index/src/hnsw.rs`
- `crates/memfuse-graph/src/csr.rs`
- `crates/memfuse-db/src/fusion.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. NaN/Inf Propagation (Score Clamping Failure): Mathematische Randfälle (z.B. $df > N$ bei BM25), die Not-a-Number erzeugen und RRF korrumpieren.
2. Dimensionality Mismatch: Vektoren mit abweichender Dimension in den Index fügen und Memory Faults auslösen.
3. Tie-Breaker Non-Determinism: Flackernde Suchergebnisse bei identischen RRF-Scores mangels sekundärer Sortierung.
4. Hub Node Explosion (Super-Node Traversal): Unbeschränkte Graph-Suchen auf dicht vernetzten Knoten konsumieren exponentiell RAM/CPU.
5. Dangling Edge / Phantom Node: Kanten verweisen auf per Tombstone gelöschte IDs.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `bm25.rs`: Stelle sicher, dass IDF-Berechnungen gegen negative Werte oder NaN ge-clampt werden (`score.is_finite()`).
2. Überprüfe `fusion.rs`: Verifiziere das deterministic Tie-Breaking (sekundäre Sortierung nach `DocId` bei Score-Gleichstand).
3. Analysiere `csr.rs` in `memfuse-graph`:
   - Enforce `max_hops <= 100` und `MAX_VISITED_NODES` Obergrenze bei Traversierungen.
   - Verifiziere, dass gelöschte Entitäten aus Tombstones (`GRAPH_ENTITY_DELETED_PREFIX`) gefiltert werden.
4. Führe Verifikationstests aus:
   `cargo test -p memfuse-text`
   `cargo test -p memfuse-index`
   `cargo test -p memfuse-graph`
   `cargo test -p memfuse-db`

ABNAHMEKRITERIEN
- RRF-Pipeline absolut NaN/Inf-resistent.
- Deterministische Tie-Breaking Sortierung garantiert.
- Hub Node Traversal durch Caps geschützt.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_search_algorithms_rrf.md`.
```

---

### Prompt 10: Advanced Concurrency, MVCC & Lock-Management

```markdown
ROLLE
Du bist Principal Senior Concurrency & MVCC Database Architect mit Spezialisierung auf Snapshot Isolation, Lock Hierarchy Enforcement, Split-Brain Prevention und Multi-Version Read/Write Concurrency.

KOMPETENZEN
- Tiefes Wissen über MVCC Transaction Pinned Snapshots, Write Skew Detection, TxId Regression Defenses, AppState Lock Order und Anti-Deadlock Patterns.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-db/src/collection/search.rs`
- `crates/memfuse-db/src/fusion.rs`
- `crates/memfuse-checkpoint/src/lib.rs`
- `crates/memfuse-core/src/snapshot.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Lock Inversion / Hierarchy Violation: Anfordern von Locks in falscher Reihenfolge führt zu Deadlocks unter Last.
2. Split-Brain Reads (Isolation Violation): Gleichzeitige Suchen sehen inkonsistente Zwischenstände über die 4 Signale.
3. Write Skew: Parallele Transaktionen modifizieren disjunkte Daten basierend auf überschneidenden Reads.
4. Time-Travel State Pollution: Rollback in Session A korrumpiert unbeabsichtigt Zustand in Session B.
5. TxId Regression (Non-Monotonicity): Transaktions-ID-Allokator vergibt IDs doppelt oder springt rückwärts.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Überprüfe `search.rs` und `fusion.rs` in `memfuse-db`: Stelle sicher, dass während einer 4-Signal-Suche ein einheitliches `SnapshotRegistry`-Handle an alle Sub-Engines durchgereicht wird.
2. Überprüfe `PersistentCheckpointStore` in `memfuse-checkpoint`: Verifiziere Session-Isolation bei parallelem Time-Travel.
3. Analysiere den TxId-Allocator: Verifiziere, dass Transaktions-IDs strikt monoton steigend aus `collection.allocate_tx()` zugewiesen werden.
4. Führe Verifikationstests aus:
   `cargo test -p memfuse-db`
   `cargo test -p memfuse-checkpoint`

ABNAHMEKRITERIEN
- Cross-Signal Snapshot Isolation nachgewiesen.
- Monotonie aller Transaktions-IDs garantiert.
- Time-Travel isolation zwischen Session-Kontexten gesichert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_advanced_concurrency_mvcc.md`.
```

---

### Prompt 11: Kryptografie, Anti-Tamper & Durability

```markdown
ROLLE
Du bist Principal Senior Cryptography & Anti-Tamper Security Architect mit Fokus auf Authenticated Encryption, Nonce Uniqueness, Domain Separation und Silent Data Corruption Defense.

KOMPETENZEN
- Experte für AES-256-GCM-SIV, Nonce Management, HKDF Domain Separation, HMAC Chain Binding und BLAKE3 Integrity Checks.

FOKUSSIERTE CRATES & DATEIEN
- `crates/memfuse-crypto/src/crypto.rs`
- `crates/memfuse-crypto/src/anti_tamper.rs`
- `crates/memfuse-store/src/wal.rs`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Nonce Reuse (IV Collision): Mehrfache Verwendung desselben IVs bei AES-GCM-SIV kompromittiert Verschlüsselung.
2. Key / Domain Confusion: Wiederverwendung desselben Schlüsselmaterials ohne HKDF-Domain-Separation für AES und HMAC.
3. Replay Attack Vulnerability: Akzeptanz alter Log-Einträge an neuer Position wegen fehlender Sequenznummer-Bindung im HMAC.
4. Silent Data Corruption (Bit Rot): Datenverfälschung auf Datenträgern ohne Erkennung beim Lesen.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere `KeyManager::encrypt_auto_nonce`: Stelle sicher, dass Nonces mit 8-Byte OsRng Suffix und 4-Byte Präfix generiert werden.
2. Verifiziere, dass HKDF Domain Separation explizit unterschiedliche Context-Strings für Encryption Keys und HMAC Integrity Keys verwendet.
3. Überprüfe die WAL Anti-Tamper HMACs: Verifiziere, dass Block-Position, Sequenznummer und Payload in den HMAC-Hash gebunden sind.
4. Führe Verifikationstests aus:
   `cargo test -p memfuse-crypto`
   `cargo test -p memfuse-store`

ABNAHMEKRITERIEN
- Strikte Nonce-Unikats-Garantie und HKDF-Domain-Separation.
- WAL HMAC Chain verhindert Replay- und Truncation-Angriffe.
- Bit Rot / Silent Corruption Detection verifiziert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_crypto_anti_tamper_durability.md`.
```

---

### Prompt 12: Testing, CI/CD, Anti-Leakage & Governance

```markdown
ROLLE
Du bist Principal Senior QA, CI/CD & Infrastructure Governance Architect mit Spezialisierung auf Deterministisches Testing, Flaky Test Elimination, Dependency Auditing und Codebase Architecture Checks.

KOMPETENZEN
- Experte für `xtask` Automation, cargo-mutants, Review-Coverage Enforcement (`check-review-coverage`), Anti-State-Leakage Test Isolation und Dependency Management.

FOKUSSIERTE CRATES & DATEIEN
- `xtask/src/main.rs`
- `TESTING.md`
- `DECISIONS.md`
- `CONSTITUTION.md`

PROBLEMSTELLUNG & GEPRÜFTE FEHLERKLASSEN
1. Flaky Test / Timing Assumption: Instabile Tests durch Vertrauen auf implizite `sleep()`-Zeiten.
2. State Leakage Between Tests: Shared Test-DB-Dateien verfälschen nachfolgende Unit Tests.
3. Shadow Dependency / Supply Chain Risk: Nutzung ungepinnter transienter Abhängigkeiten.
4. God Object / Facade Bloat: Zentrale Orchestratoren verletzen Layer-DAG-Constraints.

EXAKTE IMPLEMENTIERUNGSSCHRITTE & AUFGABEN
1. Analysiere die Testsuiten aller Workspace-Crates: Ersetze zeitrelevante Sleeps durch explizite Async-Notifications oder Polling.
2. Stellen sicher, dass alle File-basierten Store- und Database-Tests `tempfile::TempDir` verwenden, um State Leakage zu verhindern.
3. Überprüfe Gate 8 Enforcement in `xtask`: Verifiziere `cargo run -p xtask -- check-review-coverage`.
4. Führe Verifikationstests aus:
   `cargo test --workspace --exclude memfuse-tauri`
   `just sync-docs-check`

ABNAHMEKRITERIEN
- Alle Testläufe sind 100% deterministisch ohne Flakiness.
- Saubere TempDir-Test-Isolation durchgesetzt.
- Review-Coverage Checks und Documentation Sync verifiziert.
- Neuer Audit-Report abgelegt in `docs/prompts/reports/REPORT_testing_cicd_governance.md`.
```
