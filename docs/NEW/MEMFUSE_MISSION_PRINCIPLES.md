# MemFuse — Mission, Prinzipien & Architekturentscheidungen
> Vollständige Synthese für LLM-Kontext ohne Code-Zugriff  
> Basis: Repository `github.com/tfufuz1/memfuse` (HEAD ~PR #1096) + 8 Strategie-/Audit-Dokumente  
> Stand: 2026-08-30

---

## 1. MISSION & VISION

### 1.1 Kernsatz

**MemFuse ist das Cognitive Operating System für lokale KI-Agenten** — eine eingebettete, air-gapped, Pure-Rust-Bibliothek und Desktop-Applikation, die Gedächtnis, Retrieval, Kontext und (perspektivisch) Inferenz-Brücke so organisiert, dass die Arbeit mit LLMs maximal effizient wird.

Drei Eigenschaften sind dabei **nicht verhandelbar**:

- **Hardware-nah**: Der Datenpfad nutzt tatsächliche Hardwarefähigkeiten (SIMD-Dispatch, Zero-Copy-Mmap) statt generischer Abstraktionen, die Latenzbudget verschenken.
- **Performant**: Jede neue Fähigkeit wird an einem quantifizierbaren Effizienzgewinn gemessen — keine Feature-Arbeit ohne messbaren Nutzen.
- **Architektonisch exzellent**: Additive Kompatibilität als Grundgesetz. Kein Feature rechtfertigt einen Schichtgrenzen-Bruch oder Trait-Vertrags-Verletzung.

### 1.2 Was MemFuse NICHT ist (Scope-Abgrenzung, verbindlich)

- **Kein verteiltes System** — kein Cluster, kein Raft, kein horizontales Scaling (ausgelagert, nicht auf der Roadmap bis Phase 4)
- **Kein HTTP-Server** — MCP ausschließlich via stdio JSON-RPC, kein REST-Endpunkt
- **Keine Cloud-Integration** — deliberate Design-Choice, Air-Gap-Prinzip (Sovereign Core Doctrine)
- **Kein LangChain-/LangGraph-Ersatz** — Komplementärstrategie, kein Allzweck-Framework
- **Kein General-Purpose-DB-Ersatz** — kein Ersatz für PostgreSQL/SQLite
- **Kein Betrieb ohne Ollama** — lokaler Ollama-Prozess ist Laufzeit-Abhängigkeit für Embedding/LLM

### 1.3 Produktpositionierung

MemFuse ist eine **neue Kategorie**: kein Cloud-Vektordatenbank-Ersatz (Qdrant, Pinecone), keine einfache Memory-Library (Mem0), kein Graph-Only-System (Zep/Graphiti), sondern das **lokale, in-process, air-gapped Cognitive OS** für LLM-Agenten.

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|---|---|---|---|---|
| Air-gapped | ✅ | ❌ | ❌ | ✅ |
| 4-Signal-Fusion | ✅ | ❌ | Teilweise | Extern |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Contextual Retrieval | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |
| Bi-temporaler Graph | ✅ | ❌ | ❌ | ❌ |

---

## 2. KERN-USPs (unveränderliche Differenzierungsmerkmale)

Diese Eigenschaften dürfen durch **keine** Implementierung kompromittiert werden:

**1. ACID + MVCC**  
WAL-V3 (HMAC-chained, `tx_id`-gebunden), SnapshotRegistry, `commit_mutex`-Serialisierung. Volle transaktionale Garantien ohne externe Abhängigkeiten.

**2. 4-Signal-Fusion via RRF**  
HNSW (Vektor/semantisch) + BM25 (Volltext/lexikalisch) + CSR-Graph (assoziativ/relational) + Metadaten-Filter — in einer einzigen `hybrid_search()`-Abfrage fusioniert via Reciprocal Rank Fusion.

**3. Zero-External-C-Deps im Core (Layer 0–2)**  
100% Safe Rust in L0–L2. SIMD-Ausnahmen ausschließlich in `memfuse-index/src/distance.rs`, vollständig dokumentiert mit `// SAFETY:`-Kommentaren. Sovereign Core Policy.

**4. Sovereign Privacy**  
Air-gapped, lokales Ollama als Embedding-Backend, AES-256-GCM-SIV at-rest, kein Cloud-Account nötig, kein einziges Byte verlässt das Gerät.

**5. Mehrstufige RAG-Pipeline**  
Contextual Ingestion → 4-Signal-Index → RRF-Fusion → Multi-Step-Expansion → Cross-Encoder Reranking → Context Compaction. Jede Stufe optional und rückwärtskompatibel. Empirisch: +49% weniger Retrieval-Fehler (Anthropic Pattern), +67% mit Cross-Encoder.

**6. Deutsche Sprachmorphologie**  
GermanMorphTokenizer (Komposita-Splitter) — versteht „Urlaubsantragsprozess" als „Urlaub" + „Antrag" + „Prozess". Alleinstellungsmerkmal für DACH-Markt.

**7. MCP-Kompatibilität**  
Lokaler stdio-Server für Claude Desktop, Cursor und andere MCP-Clients. Zero-Config (kein Port-Binding, keine Firewall).

**8. Bi-temporaler Wissensgraph**  
`valid_from`/`valid_to` auf Kanten-Ebene — einzigartig in Open-Source-Memory-Lösungen. Ermöglicht historische Wissensgraph-Abfragen.

**9. Drei Deployment-Pfade**  
`pip install memfuse` (PyPI/PyO3), `cargo add memfuse-db` (crates.io), Tauri-Desktop-App „MemFuse Brain".

---

## 3. ARCHITEKTUR-FUNDAMENTPRINZIPIEN (Constitution)

### 3.1 Safety First — Sovereign Core Doctrine

- **Memory Safety**: Safe Rust bevorzugt. `unsafe` nur für SIMD-Intrinsics in `memfuse-index/src/distance.rs` und Mmap in `diskann.rs` (ADR-017). Jede `unsafe`-Zeile erfordert zwingend einen lokalen `// SAFETY:`-Kommentar mit Beweis.
- **No Panics**: Libraries dürfen ihren Host niemals zum Absturz bringen. Fehlerbehandlung ausschließlich via `Result<T, MemFuseError>`. Ausnahme: Vorbedingungsverletzungen bei low-level SIMD-Funktionen (dokumentierte Ausnahme via ADR-034).
- **Kein `#![allow(unsafe_code)]`** auf Modul-Ebene — immer nur lokal per Funktion.

### 3.2 Reliability & Durability

- **WAL First**: Keine Datenmodifikation im Speicher, bevor die Änderung physisch im Write-Ahead-Log committet und auf Disk gesynct ist.
- **Deterministic Recovery**: Das System muss seinen Zustand ausschließlich aus Logs rekonstruieren können.
- **No Silent Failures**: Jeder I/O-Fehler muss propagiert werden — niemals mit `let _ =` verworfen.
- **Zero-Silent-Corruption**: Kollisionen (DocId-Hash) schlagen laut und kontrolliert fehl (Fail-Safe), nie still.

### 3.3 Modularity & The DAG

Strikt gerichteter azyklischer Graph über 5 Layer:

```
Layer 0: memfuse-core       — Typen, Traits, Error (kein I/O, kein async, kein Netzwerk)
Layer 1: memfuse-store      — LSM-Tree, WAL, SSTables
         memfuse-index      — HNSW, SIMD-Distanzen, SQ8-Quantisierung
         memfuse-text       — BM25, InvertedIndex, GermanMorphTokenizer
         memfuse-crypto     — AES-256-GCM-SIV, HMAC-Chaining
         memfuse-graph      — CSR-Graph, PPR, Community Detection, bi-temporal
         memfuse-checkpoint — RAII CheckpointGuard, CheckpointCoordinator
Layer 2: memfuse-db         — 4-Signal-Fusion, RRF, MultiStep, ContextCompactor
Layer 3: memfuse-ollama     — Ollama HTTP-Client, ContextPrefixEngine
         memfuse-embed      — ONNX optional, CrossEncoderReranker (Feature-gated)
         memfuse-agent      — OrchestratorEngine, StateGraph, AuditLog
         memfuse-py         — PyO3-Bindings
         memfuse-router     — SLM-Profil-Routing
Layer 4: memfuse-mcp        — MCP stdio JSON-RPC, McpSandbox (Schreib-Gate)
         memfuse-tauri      — „MemFuse Brain" Desktop-App
```

**Invariante**: Abhängigkeiten fließen **ausschließlich abwärts**. Rückwärts-Links sind Architekturdefekte, keine Stilfragen. `memfuse-core` importiert keine internen Crates.

### 3.4 Additive Kompatibilität als Grundgesetz

Jede neue Fähigkeit ist ein **neues Trait**, eine neue `#[non_exhaustive]`-Enum-Variante oder ein neues Crate — keine bestehende Signatur wird brechend verändert. Neue Trait-Methoden erhalten Default-Implementierungen, die `MemFuseError::CapabilityUnsupported` zurückgeben. Echte Breaking Changes erfordern einen expliziten ADR + Migrations-Guide.

### 3.5 Fehlerbehandlung

- Alle Fehler kategorisiert in `memfuse_core::MemFuseError` (`#[non_exhaustive]`)
- FFI-Grenzen (Python, MCP, Tauri-IPC): Mapping auf native Exception-Typen
- Kein Fehler-Swallowing. Kein String-Verlust über FFI-Grenzen (geplant: `MemFuseErrorCode #[repr(i32)]`)

### 3.6 TxId-Origin-Invariante

`TxId` stammt **ausnahmslos** aus `Collection::allocate_tx()` (Range `[1, MAX_COLLECTION_SEQUENCE]`) oder `TxId::INTERNAL_BASE + atomic`. **Niemals** aus `SystemTime::now()` — `SystemTime` ist projektweite verboten für Sequenzierung.

### 3.7 TOMBSTONE_BIT-Disziplin

Bit 63 der Sequenznummer (`1 << 63`) signalisiert ausschließlich das Lösch-Tombstone-Flag in Datenzeilen. Es stellt **keinen** numerischen Wertanteil der Sequenznummer dar. Alle seq-Vergleiche außerhalb der MemTable/SSTable-Serialisierung müssen `& !TOMBSTONE_BIT` anwenden. Verletzung führt zu irreversiblem Datenverlust (alle nachfolgenden Inserts werden als Deletes behandelt).

---

## 4. DIE RAG-PIPELINE IM DETAIL

MemFuse implementiert eine mehrstufige, additiv erweiterbare RAG-Pipeline:

```
[1] Contextual Ingestion
    LLM generiert 50–100 Token Kontext-Präfix vor BM25/HNSW-Indexierung
    → combined_text_owned() = "prefix\n\ncontent"
    → Präfix wird NICHT im Originalinhalt persistent, nur bei Bedarf synthetisiert
    Effekt: -49% Retrieval-Fehler (Anthropic Pattern)

[2] 4-Signal-Indexierung (parallel)
    Signal 1: HNSW-Vektor (semantisch, SIMD AVX2/NEON)
    Signal 2: BM25-Volltext (lexikalisch, Deutsche Morphologie)
    Signal 3: CSR-Wissensgraph (assoziativ, Entitäten/Relationen)
    Signal 4: Metadaten-Filter (strukturiert, typisiert)

[3] Hybrid Retrieval via RRF
    Reciprocal Rank Fusion fusioniert Ränge statt roher Scores
    → keine Normalisierungsprobleme (Kosinus vs. BM25-Score)
    → kein manuelles Parameter-Tuning

[4] Multi-Step Query Expansion (max. 3 Runden)
    Iteratives Query-Rewriting (OpenAI o-series Pattern)
    → Erweiterung komplexer Agenten-Abfragen durch LLM-Reformulierung

[5] Cross-Encoder Reranking (optional)
    BGE-Reranker via ONNX Runtime (Feature-gated, default=off)
    → Post-RRF Neuordnung — kombiniert mit Schritt 1: -67% Fehler
    → Passthrough-Fallback ohne ONNX (keine Verschlechterung)

[6] Memory Importance Filtering (Phase 2)
    effective_score(now_tx) = importance * decay_function
    → NACH RRF und Reranking als Filter (nicht als Re-Ranking)
    → Filterung schützt empirisch validierte RRF-Skalierungsunabhängigkeiten

[7] Context Compaction
    StatusToken-Ersetzung veralteter Tool-Outputs
    LlmSummarize-Strategie mit source_doc_ids Provenienz-Tracking
    → Grok Pattern: Token-Reduktion bei Wissenserhalt
```

---

## 5. GETROFFENE ARCHITEKTURENTSCHEIDUNGEN (ADRs)

### 5.1 Storage & Persistenz

**ADR-001 — LSM-Tree (statt B-Tree/SQLite)**  
Hoher Schreibdurchsatz durch sequenzielle WAL-Schreiboperationen und immutable SSTables. Crash-Konsistenz und saubere Snapshot-Isolation.

**ADR-002 — HNSW für Vektorsuche (statt IVF-PQ/Flat)**  
Exzellente Suchpräzision (Recall) und geringe Latenz auf CPU, kombiniert mit SIMD-Befehlssatz-Erkennung.

**ADR-029 — WAL V3 mit tx_id-gebundener HMAC-Kette**  
Bindet `tx_id` in HMAC ein — verhindert strukturell `tx_id`-Tampering und Kausalordnungs-Manipulation. Vollständig abwärtskompatibel (Auto-Migration V1/V2 → V3).

**ADR-041 — TOMBSTONE_BIT-Maskierungsdisziplin**  
`& !TOMBSTONE_BIT` in allen seq-Vergleichen außerhalb Serialisierung ist Pflicht. Verhindert irreversiblen Datenverlust nach Rollbacks auf Delete-Operationen.

**ADR-043 — `last_committed_tx` vor SSTable-Sichtbarkeit aktualisieren**  
In `flush()`: MVCC-Snapshot-Isolation erfordert, dass `last_committed_tx` VOR `sstables.push()` aktualisiert wird. Verhindert Stale Reads im Race-Window.

**ADR-023 — Kompensierende Transaktion für Multi-Store `relate()`**  
Wenn `storage.commit(tx)` gelingt, aber `graph_index.commit(tx)` fehlschlägt: kompensierende Löschtransaktion (Tombstone-Write) mit neuer TxId — kein 2PC (zu komplex), keine Vereinheitlichung der Commit-Klammer (bricht Layer-Architektur).

### 5.2 Indexierung & Suche

**ADR-003 — RRF statt linearer Score-Gewichtung**  
Fusioniert Ränge statt roher, nicht normierter Scores (Kosinus vs. BM25). Kein manuelles Parameter-Tuning erforderlich.

**ADR-016 — DocId 64-Bit BLAKE3 + explizite Kollisionsprüfung**  
64-Bit-u64 (BLAKE3 8-Byte-Trunkierung) für HNSW-Kompatibilität. Kollisionsprüfung auf Orchestrationsebene mit explizitem Fehlschlag (kein stilles Überschreiben).

**ADR-024 — Snapshot-Isolation: nur Storage+Text (nicht Vektor+Graph)**  
Explizit dokumentiert: `HnswIndex::search_at` und `CsrGraph::traverse_at` geben `CapabilityUnsupported` zurück. Sofortiges Re-Engineering wäre zu risikoreich für Performance-kritische Pfade.

**ADR-026 — Personalized PageRank (PPR) auf CSR-Graph**  
Power-Iteration mit L1-Norm-Abbruchbedingung (`convergence_epsilon: 1e-6`) und harter Obergrenze (`max_iterations: 100`). Deterministische Konvergenz durch fixierten RNG-Seed und sekundäre Sortierung nach `EntityId`. Keine `petgraph`-Dependency (würde Graphen-Kopie erzwingen).

**ADR-027 — Label Propagation für Community Detection (statt Louvain)**  
Louvain ist bei paralleler Ausführung nicht-deterministisch und erfordert komplexe Hierarchie-Strukturen. Label Propagation: speichereffizient, deterministisch (fixierter Seed + Tie-Breaking via kleinstem `EntityId`), keine externe Dependency.

**ADR-033 — Bi-temporale Kanten (valid_from/valid_to via TxId)**  
`SystemTime`-Verbot: fachliche Zeitachsen ausschließlich via `TxId`. Abwärtskompatibel via `#[serde(default)]`. Ermöglicht historische Wissensgraph-Abfragen ohne Breaking Changes.

**ADR-038 — Zettelkasten Memory Links (A-MEM Pattern)**  
`ContextChunk` erhält `links: Vec<MemoryLink>` mit `LinkRelation` (Elaborates, Contradicts, Supersedes, References). Supersedes-Verdrängungslogik filtert überholte Chunks automatisch aus Retrieval-Ergebnissen (ohne Löschen der Historie).

### 5.3 Embedding & LLM-Integration

**ADR-008 — Ollama als primäres Embedding-Backend (statt ONNX in-process)**  
Ollama dient im KMU-Desktop-Szenario bereits als LLM-Runtime. Modell-Tausch ohne Code-Änderung, Apple-Silicon-Optimierung nativ vorhanden. Kosten: höhere Latenz pro Embedding (mitigiert durch parallele Batch-Requests), harte Laufzeit-Abhängigkeit von Ollama.

**ADR-019 — Contextual Retrieval via `combined_text_owned()` (statt Mutation)**  
Originaler Chunk-Content bleibt unverändert. Präfix als optionales Feld `contextual_prefix: Option<String>` mit `#[serde(default)]` — abwärtskompatibel. `combined_text_owned()` synthetisiert bei Bedarf.

### 5.4 Produkt & Distribution

**ADR-007 — Eingebettete Agent-Memory-Library (Richtung C)**  
Kein Server, kein Docker, kein Cloud-Account. Primäre Vertriebskanäle: PyPI + crates.io. Verworfen: Richtung A (Sovereign Edge-DB, Enterprise-Vertrieb als Solo-Entwickler aktuell nicht realisierbar), Richtung B (DACH Enterprise-Search, Morphologie-Merkmal zu schmal).

**ADR-009 — Tauri Desktop-App „MemFuse Brain"**  
Strategische Neuausrichtung zur benutzerfreundlichen Desktop-Applikation mit GUI für nicht-technische Nutzer.

**ADR-018 — Doppelstrategie: PyPI-Library UND Desktop-App**  
Auflösung des ADR-007/ADR-009-Konflikts: beide Kanäle adressieren komplementäre Zielgruppen ohne Kannibalisierung. Desktop-App: DACH-Unternehmensanwender. Library: Python/Rust-KI-Entwickler. Beide teilen denselben Kern (Layer 0–2).

**ADR-010 — MCP via stdio JSON-RPC 2.0 (statt HTTP/SSE)**  
Claude Desktop, Cursor und lokale MCP-Clients erwarten stdio per Definition. Zero-Config (kein Port, keine Firewall, kein TLS). axum/tower-Dependencies entfernt.

**ADR-020 — Cognitive Operating System als Produktvision**  
Der Wettbewerb (Mem0 ECAI-2025, Zep/Graphiti, MemOS) hat sich zu kognitiven Gedächtnisarchitekturen entwickelt. „4-Signal RAG-Engine" ist 2026/2027 nicht SOTA. Neue Positionierung: aktiv selbstorganisierende Gedächtnisarchitektur, nicht passive Speicherung.

### 5.5 Sicherheit & Kryptographie

**ADR-004 — Pure Rust Policy / Sovereign Core**  
`#![forbid(unsafe_code)]` in Layer 0–2 (ausgenommen SIMD in `memfuse-index`). Keine C-Bibliotheken im Default-Profil. Maximale Memory Safety, deterministisches Cross-Compiling, unproblematischer Betrieb in isolierten Systemen.

**ADR-017 — Explizite Authorisierung von `unsafe` Mmap in DiskANN**  
Generelle Regel erweitert für `diskann.rs` und `persistence.rs`. Mmap ist inhärent unsafe aber für High-Performance-Vektorindizes unabdingbar. Modulweite `#![allow(unsafe_code)]`-Attribute bleiben verboten.

**ADR-034 — Runtime Assertions in öffentlichen SIMD-Distanzfunktionen**  
`assert_eq!(a.len(), b.len())` ersetzt `debug_assert_eq!` — release-aktiv. Verhindert Out-of-Bounds-Buffer-Overread in `unsafe` AVX2/AVX512/NEON-Blöcken. Signatur `-> f32` bleibt erhalten (kein `Result<f32>` — zu hoher Hot-Path-Overhead).

**ADR-044 — MCP Default Read-Only / Write-Authorization-Gate**  
DB-Schreibzugriffe standardmäßig gesperrt. Schreibberechtigung nur via explizitem Flag (`--allow-write`) oder Env-Var (`MEMFUSE_MCP_ALLOW_WRITE=1`). Zentraler `validate_tool_call()` vor jedem Tool-Dispatch. Zero-Trust/Least-Privilege für LLM-gesteuerte MCP-Clients.

**ADR-014 — Regex Engine NFA/DFA (kein PCRE, kein Backtracking)**  
`regex` v1.13.1: lineare O(n) Laufzeitgarantie. Backreferences/Lookahead werden beim Kompilieren hart abgelehnt. ReDoS strukturell unmöglich. Semaphore (8 Permits) begrenzt parallele Blocking-Thread-Belegung.

### 5.6 Architektur-Integrität & Governance

**ADR-005 — Feature-Based Scaling**  
Optionale Features (ONNX, Raft-Clustering) als Opt-in in Layer 3 — verhindert C-Abhängigkeiten im souveränen Kern.

**ADR-011 — CheckpointCoordinator Trait-Konsolidierung**  
Klare Rollentrennung: `CheckpointCoordinator` = öffentliche API für persistenten State. `Checkpointer`/`CheckpointGuard` = RAII-Abstraktionen für WAL-Level-Rollbacks.

**ADR-013 — DiskANN als experimentelles Feature (versteckt)**  
DiskANN ist hinter `experimental-diskann`-Feature und `#[doc(hidden)]` verborgen. Noch nicht in `VectorIndexBackend`-Schnittstelle integriert — `Collection` und `HnswIndex` sind eng verzahnt, überhastete Integration würde Architektur-Integrität gefährden.

**ADR-022 — Single Responsibility für Dokumentation**  
Jede Information lebt an genau einem Ort. `xtask sync-docs` generiert `ARCHITECTURE.md` und `WORKING_STATE.md` deterministisch. Konsistenzprüfung via `cargo xtask check-consistency`.

**ADR-028 — Dezentrales Inline-Kontextsystem & Mehrfach-Session-Review**  
Sekundengenaue Zeitstempel (`TS:YYYY-MM-DDTHH:MM:SSZ`), Hash-basierte IDs (`AGT-<CRATE>-<8-hex-hash>`), verpflichtende `REVIEW-PASS[N/M]`-Grammatik für Qualitätssicherung ohne Bestätigungs-Bias.

**ADR-037 — VectorIndex-Generalisierung `Collection<S, V>`**  
`Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex>` — entkoppelt starre `Arc<HnswIndex>`-Bindung. Vollständige Abwärtskompatibilität durch Default-Typ.

**ADR-040 — collection.rs Modularisierung**  
God-Object (~2.900 LOC) aufgeteilt in Submodule unter `collection/`. Öffentliche API bleibt identisch über Re-Exports in `collection/mod.rs`.

**ADR-045 — JSON-RPC-Typen nach `memfuse-core::ipc` verschoben**  
Schichtgrenzenverletzung Layer 3 → Layer 4 behoben: `memfuse-router` importiert `JsonRpcRequest`/`JsonRpcResponse` aus `memfuse-core::ipc`, nicht aus `memfuse-mcp`.

**ADR-042 — Wiederherstellung `memfuse-agent`**  
MCP-Sandbox ist zustandslos — Multi-Step Agent-Workflows verlieren bei Crash ihren State. `memfuse-saos-agent` (gelöscht) wird als `memfuse-agent` wiederhergestellt: `AgentTool`-Trait, `OrchestratorEngine`, `StateGraph`, `AuditLog`. `checkpoint → execute → commit → audit`-Loop als fehlende Persistenzschicht.

---

## 6. VERWORFENE ALTERNATIVEN

### 6.1 Strategisch verworfen

| Alternative | Warum verworfen |
|---|---|
| Cloud-Service | Widerspricht Sovereign Core Doctrine (ADR-004) — fundamentaler |
| Nur Desktop-App (kein PyPI) | Adressiert andere Zielgruppe als Library — keine Kannibalisierung |
| Nur Library (kein Desktop) | Desktop erreicht nicht-technische DACH-Nutzer, die Library nicht |
| Raft-Clustering / Distributed | Solo-Entwickler: Enterprise-Vertrieb nicht realisierbar, Code ausgelagert |
| DACH Enterprise-Search-Fokus | Morphologie-Merkmal zu schmal für eigenständiges Produkt |
| KV-Cache-Bridging als Kern-Feature | In einer externen Rust-DB „nicht sauber implementierbar" — eigenes Source-of-Truth-Papier verwirft dies explizit; `memfuse-kv` bleibt Phase-4-Forschungscharakter |

### 6.2 Technisch verworfen

| Entscheidung | Alternative | Warum verworfen |
|---|---|---|
| HNSW | IVF-PQ (Quantisierung), Flat Index | HNSW: besserer Recall + geringere Latenz auf CPU |
| RRF | Lineare Score-Gewichtung | Nicht-normierte Scores (Kosinus vs. BM25) nicht direkt kombinierbar |
| LSM-Tree | B-Tree, SQLite | Hoher Schreibdurchsatz, saubere Snapshot-Isolation |
| Ollama (Embedding) | ONNX in-process (`memfuse-embed`) | Modell-Tausch ohne Code-Änderung, Apple-Silicon nativ, kein ONNX-Vendoring |
| stdio JSON-RPC (MCP) | HTTP/SSE-Transport | Zero-Config, lokale MCP-Clients erwarten stdio per Definition |
| NFA/DFA Regex | PCRE mit Backreferences | PCRE bricht Linearitätsgarantie — ReDoS möglich |
| Label Propagation | Louvain | Louvain: nicht-deterministisch bei paralleler Ausführung |
| PPR ohne petgraph | petgraph-Dependency | Petgraph erzwingt Graphen-Kopie (Speicher+Latenz-Overhead) |
| Kompensierende Transaktion | 2-Phase-Commit | 2PC: Breaking API-Änderungen an Trait-Schnittstellen |
| Kompensierende Transaktion | Vereinheitlichte Commit-Klammer | Bricht Layer-Architektur (`CsrGraph` und `LsmStorage` inkompatibel) |
| `ContextChunk.contextual_prefix` | Separater `ContextualDocumentChunk` | Typ-Explosion, Inkonsistenzen in bestehenden Pipeline-Ketten |
| `combined_text_owned()` | Festes Mutieren von `content` | Nutzer sollen beim Retrieval unveränderten Originaltext erhalten |
| `assert_eq!()` in SIMD-Distanzfunktionen | `-> Result<f32>` Signatur | Result-Propagation: signifikanter Hot-Path-Overhead, bricht alle Aufrufer |
| DocId 64-Bit + Kollisionsprüfung | 128-Bit/256-Bit UUID/Hash | Veränderung aller HNSW-Knoten-IDs und Speicherstrukturen |
| TOMBSTONE_BIT-Maskierung | Unmaskierte Übernahme in `max_seq` | Bit 63 wandert in `next_seq_no` → alle Inserts als Deletes behandelt |
| `last_committed_tx` VOR `sstables.push()` | Exklusiver Schreib-Lock über gesamten Lesepfad | I/O unter Lock hält Ressourcen zu lang blockiert |
| Atomare SSTable-Umbenennung (`.tmp` + rename) | Direktes Schreiben unter Zielnamen | Crash → halbgeschriebene `.sst`-Datei → Recovery-Korruption |
| DiskANN hinter Feature-Flag | Sofortige Integration in Collection | Enge Verzahnung Collection/HNSW, Snapshot-Isolation gefährdet |
| `memfuse-core::ipc` für JSON-RPC-Typen | Separates `memfuse-jsonrpc`-Crate | Crate-Explosion; `memfuse-core::ipc` existiert bereits |
| CheckpointGuard in `memfuse-checkpoint` | Entkoppelt lassen (Status quo) | Dauerhafte Code-Duplikation, zwei verschiedene Checkpoint-Konzepte |
| Abwärtskompatible `Edge`-Erweiterung | Separater `TemporalEdge`-Typ | Typ-Explosion, abwärtskompatible Deserialisierung über `#[serde(default)]` |
| Cognitive OS Positionierung | Beibehaltung „4-Signal Memory Engine" | Zu eng, kein Alleinstellungsmerkmal gegen Mem0/MemOS 2026/2027 |

---

## 7. ROADMAP (4 Phasen)

### Phase 1 — RAG-Fundament ✅ Abgeschlossen

- LSM-Tree Storage: MVCC, WAL-V3, Crash-Recovery, Crypt-at-Rest
- 4-Signal Hybrid-Index: HNSW (SIMD), BM25 (deutsche Morphologie), CSR-Graph, Metadaten
- Contextual Retrieval (`ContextPrefixEngine`, `combined_text_owned()`)
- Cross-Encoder Reranking (ONNX, optional, Feature-gated)
- Multi-Step Query Engine (max. 3 Schleifen)
- Context Compaction (`StatusToken`, `LlmSummarize` mit Provenienz)
- Session DAG Branching (`SessionBranchTree`, `AgentStateNode`)
- MCP Sandbox Isolation (`McpSandbox`, `VolatileToolResult`, AES-256-GCM-SIV)
- Distribution: Tauri-Desktop-App, MCP-Server, Python-Bindings

### Phase 2 — Cognitive Memory (Q4 2026)

- **Kognitive Gedächtnistypen** als explizite Collection-Typen: Episodic / Semantic / Procedural / Working Memory
- **Temporaler Wissensgraph**: bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit via TxId)
- **Memory Importance Score**: LLM-bewertet (wie Generative Agents)
- **Recency-Decay-Funktion**: mathematischer Verfall für episodische Relevanz
- Mehrere ADRs bereits verabschiedet: ADR-025 (Importance+Decay), ADR-033 (bi-temporal)

### Phase 3 — Selbstorganisierung (Q1 2027)

- **Memory Consolidation**: automatische Zusammenfassung veralteter Chunks via LLM
- **Personalized PageRank (PPR)**: Multi-Hop Graph-Retrieval (ADR-026 bereits implementiert)
- **Community Detection**: semantische Cluster via Label Propagation (ADR-027 bereits implementiert)
- **A-MEM Zettelkasten-Pattern**: Memories mit expliziten Querverweisen (ADR-038 bereits implementiert)
- **PathRAG**: Pfad-Extraktion als `GraphTraversalStrategy`
- **ProvenanceRecord**: kryptographisch authentifizierter Herkunftsnachweis
- **Verified Forgetting**: kryptographischer Löschbeweis in `memfuse-crypto`

### Phase 4 — Enterprise & Zukunft (Q2 2027)

- OAuth 2.0, RBAC, Multi-Tenant-Isolation
- Immutable Audit-Trail für Compliance
- Benchmark-Suite vs. Mem0, Zep/Graphiti, MemOS
- `memfuse-quant`: Matryoshka-Truncation, SQ8-Codec (neues Crate L1)
- `memfuse-kv`: KV-Cache-Brücke zur LLM-Inferenz (Forschungscharakter)
- `VamanaIndex`: disk-residenter ANN als Alternative zu HNSW
- `IoBackend`: io_uring/O_DIRECT Abstraktion für `memfuse-store`

---

## 8. QUALITÄTSPRINZIPIEN (Definition of Done)

Eine Codeänderung ist erst vollständig, wenn:

1. Alle offenen `TODO`/`AI-TAG`-Einträge im geänderten Bereich aufgelöst oder dokumentiert
2. Gate-Stack grün: `cargo check --workspace`, `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo xtask sync-docs`, `cargo xtask dag-check`
3. Nicht-triviale Architekturentscheidungen haben einen ADR in `DECISIONS.md` — **vor** Umsetzungsbeginn
4. Keine offenen `BLOCKER`- oder `CRITICAL`-Sicherheitsrisiken
5. `WORKING_STATE.md` mit aktuellem Status aktualisiert

**Triple-Test-Gate**: `just triple-test` führt `cargo test` 3× hintereinander aus — detektiert Flaky Tests.

**Statusindikator-Prinzip**: 🟢/🟡/🔴 werden **ausschließlich** durch CI-Ergebnisse gesetzt, niemals durch manuelle Agenten-Einschätzung.

**Trait-Default-Pflichttest**: Für jedes `pub trait` mit Default-Methode muss im selben PR ein Integrationstest beweisen, dass die Default-Implementierung nicht still greift.

**SAFETY-Kommentar-Unikats-Pflicht**: `// SAFETY:`-Kommentare müssen die konkrete Invariante der spezifischen Funktion benennen — word-identische Duplikate sind unzulässig.

---

## 9. BEKANNTE LIMITIERUNGEN & OFFENE RISIKEN (Stand 2026-08-30)

### 9.1 Architekturelle Begrenzungen (dokumentiert, akzeptiert)

- **Snapshot-Isolation**: Vektor- und Graph-Signale sind nicht snapshot-isoliert (nur Storage + Text). Explizit in ADR-024 dokumentiert.
- **DiskANN**: Out-of-Core-Vektorsuche experimentell, nicht in öffentliche API integriert (ADR-013).
- **Ollama-Abhängigkeit**: Harte Laufzeit-Abhängigkeit für alle Embedding- und LLM-Funktionen.
- **Bus-Faktor 1**: Ein-Personen-Projekt mit ~62.000 LOC. Governance-Prozesse (Multi-Session-Review) kompensieren dies teilweise, sind aber noch nicht vollständig implementiert.

### 9.2 Governace-Lücken (Senior Review 2026-08-30)

- Mehrere CI-Gates blockieren nicht wie dokumentiert (Gate 2, 8, 9 faktisch wirkungslos)
- Gate 7 hat terminiertes Ablaufdatum (Oktober 2026 nicht abgedeckt)
- `CONTEXT_ENGINEERING_SYSTEM.md` referenziert nicht-existente Tools (`context-cli`, `audit-export`)
- Gate-8-Review-Coverage-Check erfasst 24 von 25 echten ANCHOR-Tags nicht

### 9.3 Offene technische Schulden (priorisiert)

- Blake3 im MemTable-Hot-Path `shard_for()` (→ AHash ist 3–5× schneller)
- CSR `compact()`: O(N) Full-Rebuild (→ inkrementeller Delta-Aufbau)
- ~5 `unwrap()`/`expect()` in Produktionscode (L3/L4)
- 32 `.clone()`-Aufrufe in `lsm.rs` (Zero-Copy-Optimierung Phase 3)
- BM25-IDF-Floor `1e-6` statt `0.0` (Robertson-Standard leicht verletzt)

---

## 10. DOKUMENTENHIERARCHIE (Single Responsibility)

| Datei | Zuständigkeit | Quelle |
|---|---|---|
| `AGENTS.md` | Verbindliche Verhaltensregeln für Agenten | Manuell, stabil |
| `CONSTITUTION.md` | Governance-Prinzipien (Warum hinter den Regeln) | Manuell, selten |
| `DECISIONS.md` | ADR-Log, chronologisch, append-only | Manuell, vor Umsetzung |
| `docs/SOURCE_OF_TRUTH.md` | Produktstrategie, Roadmap, Entscheidungskontext | Manuell + auto-generiert |
| `docs/ARCHITECTURE.md` | Technische Ist-Architektur, DAG, Layer | Auto-generiert via `xtask sync-docs` |
| `WORKING_STATE.md` | Session-zu-Session-Handoff, offene Tags | Auto-generiert |
| `docs/TYPE_REGISTRY.md` | Zentrales Typ-/Trait-Register (Kollisionsschutz) | Manuell + xtask |
| `rules/*.md` | Domänenspezifische Detailregeln (SIMD, Crypto, Testing) | Manuell |

**Prinzip**: Jede Information lebt an **genau einem** Ort. Keine Duplikation.

---

*Erstellt auf Basis von: `github.com/tfufuz1/memfuse` (Clone 2026-08-31) + 8 Strategie-/Audit-Dokumente (Stand 2026-08-30)*  
*Enthält: README.md, CONSTITUTION.md, DECISIONS.md (45 ADRs), SOURCE_OF_TRUTH.md, MEMFUSE_FINALE_STRATEGIE_2026-08-30.md, MemFuse_Senior_Review_2026-08-30.md, MEMFUSE_MASTER_SPECIFICATION.md, MemFuse_Senior_Rust_Architektur_Analyse.md*
