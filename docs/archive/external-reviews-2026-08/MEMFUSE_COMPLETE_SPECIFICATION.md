# MemFuse Brain — Umfassende Spezifikation & Architektur (v2.0)
## Das Cognitive Operating System für lokale LLM-Agenten

> **Synthese**: Dieses Dokument konsolidiert:
> 1. Die vier ursprünglichen Spezifikationsdokumente (`memfuse_interface_spec.md`, `memfuse_interface_spec_updated.md`, `memfuse_v2_optimierungsspezifikation.md`, `MEMFUSE_INTERFACE_SPECIFICATION.md`)
> 2. Repository-Recherche (git commits, CONSTITUTION.md, AGENTS.md, SOURCE_OF_TRUTH.md, strategic_roadmap.md, DECISIONS.md, Feature-Flags)
> 3. Aktuelle Realität: 15 Workspace-Crates, ~60.786 LOC, 4-Phasen-Roadmap (Phase 1 ✅, Phase 2-4 📋)
>
> **Stand**: 2026-08-29 · HEAD: 73dd4d1 (Harden memfuse-store)

---

## Inhaltsverzeichnis

0. Mission & Produktvision
1. Gesamtarchitektur (5-schichtig, DAG)
2. Alle 15 Workspace-Crates im Detail
3. Die 4 Signale der Hybrid-Suche
4. Cognitive OS-Kernfeatures
5. Kryptographie & Sicherheit
6. 4-Phasen-Roadmap mit aktuellem Status
7. Quality Gates & Governance
8. Feature-Flags & Build-Varianten
9. APIs & Integrationspunkte
10. Bekannte Grenzen & Zukunftsarbeit
11. Quellenverzeichnis

---

## 0. Mission & Produktvision

### 0.1 Mission (aus ADR-020 & AGENTS.md)

**MemFuse Brain** ist ein **eingebettetes Cognitive Operating System für lokale LLM-Agenten**. Es kombiniert:

- **Vektorsuche** (semantisch, HNSW mit SIMD)
- **Volltextsuche** (lexikalisch, BM25 mit deutscher Morphologie)
- **Entity-Relation-Graph-Traversal** (assoziativ, CSR mit Bi-temporal)
- **Metadaten-Filterung** (strukturell, FilterExpr-Prädikate)

…zu einer **einzigen, integrierten Abfrage** via RRF-Fusion (Reciprocal Rank Fusion), ideal als Prompt-Kontext für lokale LLM-Inferenz.

### 0.2 Kernmerkmal: Cognitive OS (nicht nur "Memory Engine")

MemFuse positioniert sich bewusst NICHT als reiner Vector-Store, sondern als **aktives, mehrschichtiges Kognitions-System**:

| Schicht | Feature | Pattern | Status |
|---------|---------|---------|--------|
| Ingestion | Contextual Ingestion | LLM-generiertes Kontext-Präfix vor BM25 (Anthropic) | ✅ ADR-019 |
| Indexing | 4-Signal Hybrid-Index | HNSW + BM25 + Graph + Metadaten in einer Query | ✅ ADR-021 |
| Retrieval | Reciprocal Rank Fusion | Fusion unkompattibler Score-Skalen | ✅ ADR-003 |
| Expansion | Multi-Step Query Engine | Iteratives Query-Rewriting (OpenAI o-series) | ✅ ADR-021 |
| Reranking | Cross-Encoder Reranking | Post-RRF Neuordnung via lokalem ONNX | ✅ ADR-021 |
| Kontext | Context Compaction | Token-Reduktion via LLM-Summarization (Grok) | ✅ ADR-019 |
| Konversation | Session DAG Branching | Persistent Gesprächsverzweigung (Grok) | ✅ ADR-020 |
| Sicherheit | MCP Sandbox Isolation | AES-256-GCM-SIV Tool-Output-Verschlüsselung | ✅ ADR-010 |

### 0.3 Nicht-Verhandelbare Constraints

**Non-negotiable constraints** aus AGENTS.md § 1:
- **Pure-Rust**: Kein C, kein extern-C in den Kern-7-Crates (Layer 0–2)
- **Luftgegrenzt**: Keine Cloud, keine obligatorischen HTTP-Services
- **Lokal-first**: Einzelner Installer, Zero-Docker, läuft auf jedem Laptop
- **Fehlerfortpflanzung**: ALLE Fehler via `MemFuseError` + `?`, KEINE `let _ = err` in Production

### 0.4 Unterscheidung zu Alternativen

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|-----------|---------|------|--------------|---|
| 4-Signal Hybrid-Fusion | ✅ | ❌ | Teilweise | Extern |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| Air-gapped | ✅ | ❌ | ❌ | ✅ (aber komplex) |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |

---

## 1. Gesamtarchitektur (5-Schichten-DAG)

### 1.1 Schichtenmodell

```
┌─────────────────────────────────────────────────────────┐
│ Layer 4: User-Facing Interfaces                         │
│  memfuse-mcp (stdio JSON-RPC 2.0)                       │
│  memfuse-tauri (Desktop GUI, Electron-ähnlich)          │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│ Layer 3: Application Services                           │
│  memfuse-py (PyO3 Python FFI)                           │
│  memfuse-ollama (LLM + Embedding Backend)               │
│  memfuse-agent (Persistent Workflow Engine)             │
│  memfuse-router (Routing/Cascading Decision Engine)     │
│  memfuse-embed (ONNX Embeddings, optional)              │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│ Layer 2: Orchestrator Facade                            │
│  memfuse-db (Collections, 4-Signal Fusion, MultiStep)   │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│ Layer 1: Triebwerk (Storage Engines)                    │
│  memfuse-store (LSM-Tree, MVCC, WAL, Crypt-at-Rest)     │
│  memfuse-index (HNSW, SIMD, SQ8, DiskANN-Exp)           │
│  memfuse-text (BM25, Inverted Index, Morphologie)       │
│  memfuse-graph (CSR-Graph, Entity-Relation, PPR, etc.)  │
│  memfuse-checkpoint (Async Snapshotting)                │
│  memfuse-crypto (AES-256-GCM, HMAC-Chaining)            │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│ Layer 0: Fundament (Pure Types & Traits, kein I/O)      │
│  memfuse-core (Errors, Traits, DTOs, Constants)         │
│  • Zero unsafe (sovereign-core-doktrin)                 │
│  • Keine Abhängigkeiten                                 │
└─────────────────────────────────────────────────────────┘
```

### 1.2 Abhängigkeitsgraph (Ist-Zustand)

```
memfuse-core (Wurzel)
  ├── memfuse-crypto          → core
  ├── memfuse-checkpoint      → core
  ├── memfuse-store           → core, crypto
  ├── memfuse-index           → core, store  [optional: graph]
  ├── memfuse-text            → core
  ├── memfuse-graph           → core, store
  ├── memfuse-db              → all Layer-1 crates
  ├── memfuse-agent           → core, db, graph, checkpoint, store
  ├── memfuse-ollama          → core
  ├── memfuse-embed           → core [optional, feature-gated]
  ├── memfuse-py              → core, db
  ├── memfuse-router          → core [optional: memfuse-ollama]
  ├── memfuse-mcp             → agent, core, crypto, db, ollama
  └── memfuse-tauri           → core, db, graph, ollama

xtask (Buildtool, kein Produktiv-Code)
```

### 1.3 Topology-Invarianten (aus CONSTITUTION.md)

- **DAG ist heilig**: Dependencies fließen STRENG downward
- **Layer-0 ist agnostisch**: memfuse-core kennt KEIN HTTP, kein async I/O, keine High-Level-Semantik
- **Sovereign Core**: Layer 0–2 sind 100% Safe Rust (kein unsafe außer Test-Spezialfällen, ADR-036)
- **Violations sind Architektur-Defekte**, nicht Stilfragen → ADR required

### 1.4 Linesof-Code-Inventar (WORKING_STATE.md, 2026-08-29)

| Crate | Layer | LOC | Status |
|-------|-------|-----|--------|
| memfuse-core | L0 | 7.157 | 🟢 Clean |
| memfuse-checkpoint | L1 | 1.166 | 🟢 Clean |
| memfuse-crypto | L1 | 1.144 | 🟢 Clean |
| memfuse-graph | L1 | 4.506 | 🟢 Clean |
| memfuse-index | L1 | 7.305 | 🟢 Clean |
| memfuse-store | L1 | 10.095 | 🟢 Clean |
| memfuse-text | L1 | 3.562 | 🟢 Clean |
| memfuse-db | L2 | 11.963 | 🟢 Clean |
| memfuse-agent | L3 | 2.280 | 🟢 Clean |
| memfuse-embed | L3 | 1.022 | 🧊 Optional |
| memfuse-ollama | L3 | 2.369 | 🟢 Clean |
| memfuse-py | L3 | 963 | 🟢 Clean |
| memfuse-router | L3 | 510 | 🟢 Clean |
| memfuse-mcp | L4 | 2.095 | 🟢 Clean |
| memfuse-tauri | L4 | 2.609 | 🟢 Clean |
| **TOTAL** | — | ~60.786 | — |

---

## 2. Alle 15 Workspace-Crates im Detail

### 2.1 Layer 0: memfuse-core (Fundament)

**Zweck**: Zentrale Typ-Definitionen, Trait-Kontrakte, Fehlerbehandlung. Zero I/O, Zero async, Zero Abhängigkeiten.

**Schlüsseltypes**:
- `MemFuseError` (enum #[non_exhaustive], ausführliches Error Handling)
- `TxId` (u64, transaktionale Versionierung)
- `DocId`, `EntityId` (u64 BLAKE3-Trunkierung, ADR-016)
- `Embedding` (Vec<f32>, ggf. Matryoshka-Trunkierbar)
- `Entity`, `Edge` (CSR-Graph-Knoten/Kanten)
- `ContextChunk` (mit `contextual_prefix: Option<String>`, ADR-019)
- `MemoryType` (enum: `Episodic | Semantic | Procedural | Working`, ADR-025)
- `FilterExpr` (AST für Metadaten-Prädikate)

**Kern-Traits** (alle mit Default-Methoden, `CapabilityUnsupported`-Fallback):
- `StorageEngine` (Persistenz-Abstraktion, impl. von `LsmStorage`)
- `VectorIndex` (HNSW-Abstraktion, impl. von `HnswIndex`)
- `TextEmbeddingEngine` (Embedding-Produktion, impl. von Ollama)
- `TextIndex` (BM25-Abstraktion)
- `GraphIndex` (CSR-Graph-Traversal)
- `Checkpoint`, `Snapshot`, `DistanceCalculator` (spezialisiert)

**Governance** (aus ADR-035):
- Zero-Panic Invariante: offene `.expect()` nur in Tests (2 in `snapshot.rs`, 3 in `SessionPool`, Status: 🟡)
- Trait-Default-Pflichttest: MUSS für jeden neuen Implementor eines Traits mit Defaults existieren
- Typ-Dopplungs-Prävention: `TYPE_REGISTRY.md` vor Anlage neuer Typen prüfen

---

### 2.2 Layer 1: Triebwerk (6 Engine-Crates)

#### 2.2.1 memfuse-store — LSM-Tree-Persistenzschicht

**Architektur**: Log-Structured Merge Tree (LSM), MVCC-Snapshot-Isolation, Write-Ahead-Log (WAL), SSTables, Compaction.

**Schlüsselkomponenten**:
- `LsmStorage` (Hauptimplementierung von `StorageEngine`)
- `MemTable` (In-Memory-Schreibpuffer, roter/schwarzer Baum)
- `WalWriter` + `WalReader` (Write-Ahead-Log mit HMAC-Integritätskette, ADR-029)
- `SstableBuilder`, `SstableReader` (SSTable-Format, Level-basiertes Compaction)
- `CompactionEngine` (Multi-Level-Merge, konfigurierbares Verhältnis)
- `LsmConfig` (Tuning: memtable_size, level_ratio, max_levels, etc.)

**Snapshot-Isolation** (ADR-024):
- `scan_prefix_at(prefix, tx_id)` — Punkt-in-Zeit-Sicht auf KV-Paare
- MVCC über Read-Timestamp-Abgleich gegen WriteIntent-Log
- **Aktuell**: Storage-Engine ist snapshot-isoliert ✅, Graph & Vektor sind NICHT snapshot-isoliert (bewusst, für Performance)

**Kryptographie** (ADR-029):
- AES-256-GCM Encryption-at-Rest (Disk-Ebene)
- HMAC-SHA256-Kette über WAL-Einträge (Integritätsprüfung, nicht Konfidentialität)
- `load_or_create_integrity_key()` (Schlüsselerzeugung)

**Hardening** (Letzter Commit #1047):
- Silent errors gegen Malformed Inputs gehärtet
- Resource-Exhaustion-Schutz (max. file descriptors, etc.)

**Bekannte Grenzen**:
- Storage ist NICHT disk-resident (gesamte LSM-Topologie im RAM)
- Performance-Tuning erfordert manuelle `LsmConfig`-Anpassung
- Compaction-Overhead bei extrem häufigem random-Write

---

#### 2.2.2 memfuse-index — Vektor-Indexierung & SIMD

**Architektur**: HNSW (Hierarchical Navigable Small World) mit SIMD-Beschleunigung.

**Implementoren**:
- `HnswIndex` (Haupt-Implementierung, Production-ready)
- `DiskAnnIndex` (Experimentell, Feature-Flag `experimental-diskann`, siehe Roadmap-Phase 3)

**HNSW-Komponenten**:
- `Graph` (Linked-List-Struktur mit Ebenen-Organisation)
- `Node` (Knoten mit Nachbarslisten pro Level)
- `Heuristic`-Funktion (Kandidaten-Auswahl, M und M_max konfigurierbar)

**SIMD-Distanzberechnung** (memfuse-index/src/distance.rs, ADR-017, ADR-034):
- `DistanceCalculator` Trait
- `FlatDistanceCalculator` (Baseline, Rust-Standard-Schleifen)
- AVX2/AVX-512-VNNI Intrinsics (Runtime-Feature-Detection via `std::is_x86_feature_detected!`)
- **SAFETY**: Jede unsafe-Region trägt `// SAFETY:` Proof-Comment (ADR-017)
- Fallback auf Scalar bei CPU-Feature-Mängel (garantiert)

**Quantisierung** (SQ8, Skalar-Quantisierung):
- `ScalarQuantizer` (Min/Max-Kalibrierung pro Dimension)
- Int8-Komprimierung für Speicher-Effizienz
- Reranking auf Full-Precision nach Candidate-Selection

**Persistierung**:
- `HnswMetadata` (Graph-Struktur, Ebenen-Info, Knoten-Listen)
- `VectorStore` (Rohvektor-Speicher, Mmap-basiert)
- Atomic-Rename beim Persist (Data-Consistency)

**Experimentelle Features** (Feature-Flag `experimental-diskann`):
- `DiskAnnIndex` — Disk-resident Vamana-ähnlicher Index (Roadmap Phase 3, nicht produktionsreif)
- PQ-Codierung mit Shortlist-Phase

**Bekannte Grenzen**:
- In-Memory-Only: Collection-Größe limitiert auf verfügbares RAM
- Deletion: Marked-as-Deleted (Tombstones), Full-Rebuild bei >20% Deletions
- HNSW: M/M_max-Tuning ist nicht-trivial (Default: M=5, M_max=10)

---

#### 2.2.3 memfuse-text — Volltextsuche

**Architektur**: BM25-Scoring mit Inverted Index.

**Komponenten**:
- `Tokenizer` (Whitespace + Regex-basiert)
- `Morphology` (Deutsche Compound-Wort-Zerlegung, z.B. "Urlaubsantrag" → ["Urlaub", "Antrag", "Prozess"])
- `InvertedIndex` (Token → DocIds + Positions)
- `BM25Scorer` (IDF + BM25-Formel, k1=1.5, b=0.75 Standard)
- `TermVectorStore` (Terme pro Dokument, für Reranking)

**Snapshot-Isolation** (ADR-024):
- `search_at(query, tx_id)` — Punkt-in-Zeit-Suche
- Berücksichtigt nur Dokumente geschrieben vor `tx_id`

**Sicherheit** (ADR-014):
- ReDoS-Härtung für Regex-Transformationen
- Pattern-Caching, Timeout auf lange-laufende Matches

**Deutsche Morphologie** (Highlight MemFuse vs. Alternativen):
- Prefix-Matching für Compound-Zerlegung
- Suffix-Rules für Flexion
- Ablauf: "Urlaubsantragsprozess" → Suche nach "Antrag" findet Treffer

**Bekannte Grenzen**:
- BM25 ist Baseline; Semantic Search via Vektor-Index bessere Qualität
- Tokenizer ist nicht Unicode-aware (ASCII-zentriert, Arbeit benötigt)
- ReDoS-Härtung: strikte Timeout-Obergrenzen, kein backtracking

---

#### 2.2.4 memfuse-graph — Knowledge Graph (CSR + Session DAG + PPR + Community)

**Architektur**: Compressed Sparse Row (CSR) Format für Entity-Relation Graph.

**Komponenten**:

1. **CSR-Graph** (Hauptstruktur):
   - `Entity` (Knoten, mit Text/Embedding)
   - `Edge` (gerichtete Kanten, weighted, mit Validitätsfenster `valid_from`/`valid_to`, ADR-033)
   - `CsrGraph` (CSR-Speicher: offsets, targets, weights)
   - Persistierung: unter `__graph:entity:` und `__graph:edge:` Präfixen im LSM-Store

2. **Bi-temporale Kanten** (ADR-033, Phase 2 Foundation):
   - `valid_from`: Transaktionszeit, wann die Kante geschrieben wurde
   - `valid_to`: Transaktionszeit, wann die Kante ungültig wurde
   - `traverse_at_time(start, tx_id, max_hops)` — Punkt-in-Zeit-Traversierung
   - Ermöglicht Zeitreisen-Abfragen: "Was waren die Relationen zum Zeitpunkt X?"

3. **Personalized PageRank** (PPR, ADR-026, neu in Phase 1):
   - `ppr.rs` (pub(crate), Power-Iteration-Algorithmus)
   - `compute_ppr(inner, seed_nodes, config)` — Dangling-Node-Handling, Restart-Wahrscheinlichkeit
   - Seed-Knoten können Einzeln oder Multi-Hop-Nachbarn sein
   - Konfluiert zu `GraphIndex::personalized_page_rank()` Trait-Methode

4. **Community Detection** (ADR-027, neu in Phase 1):
   - Label Propagation Algorithmus (deterministisch via Seeded RNG)
   - `detect_communities(graph, config)` → Vec<CommunityAssignment>
   - Persistierung: unter `__graph:community:{entity_id}` Präfix
   - Nutzung: `same_community_as` Filter in `hybrid_search_with_strategy`

5. **Session Branching DAG** (ADR-NNN, Grok-Pattern):
   - `SessionBranchTree` (Persistent Conversation DAG)
   - `AgentStateNode` (Conversation-Turn als Knoten)
   - `DagEdge` (Branch/Merge-Relationen)
   - Ermöglicht User-Exploration von Multi-Path-Konversationen

**Snapshot-Isolation**: NICHT snapshot-isoliert aktuell (Performance-Trade-off). Graph-Abfragen sehen immer aktuelle Topologie.

**Bekannte Grenzen**:
- CSR ist Read-Optimiert, Write kann Reorg erfordern
- PPR-Konvergenz benötigt Tuning (max_iterations, epsilon)
- Community Detection ist Single-Pass, nicht inkrementell

---

#### 2.2.5 memfuse-checkpoint — Snapshot-Management

**Zweck**: Asynchrone, transaktionale Snapshots der gesamten DB-State (Storage + Index + Graph + Text).

**Komponenten**:
- `Checkpoint` Trait
- `CheckpointCoordinator` Trait (Orchestrierung mehrerer Checkpoint-Quellen)
- `CheckpointGuard` (RAII-Pattern, Rollback auf Drop)
- State-Serialisierung (Bincode oder custom Binary-Format)

**Bekannte Grenzen** (ADR-011):
- Zwei `CheckpointGuard`/`StateCheckpoint`-Paare erzeugen Verwirrung (Doku-Risiko #7)
- Async-Checkpoint kann zu Speicher-Spikes führen (gesamte DB im RAM snapshotten)

---

#### 2.2.6 memfuse-crypto — Kryptographie & Integrität

**Komponenten**:
- `IntegrityVerifier` (HMAC-SHA256-Kette über WAL, ADR-029)
- `VolatileEncryptionKey` (AES-256-GCM für MCP Tool-Outputs, Zeroize on Drop)
- `WalHmac` (Chaining-HMAC, prev_hmac → neue Integrität)
- Anti-Tamper-Erkennung (Bit-Flip-Schutz via Redundanzprüfung)

**Safety** (ADR-036):
- Test-only unsafe in `anti_tamper.rs` für Zeroize-Verifikation (via raw pointer inspection)
- Production: 100% Safe (via `#![cfg_attr(not(test), forbid(unsafe_code))]`)

**Bekannte Grenzen**:
- AES-256-GCM benötigt Key-Management (aktuell: lokale Datei, kein HSM)
- HMAC ist für Integrität, nicht für Encryption (Storage nutzt zusätzlich AES-256-GCM)

---

### 2.3 Layer 2: memfuse-db — Orchestrator-Facade (zentrale Geschäftslogik)

**Zweck**: Oberflächenebene, koordiniert alle Layer-1-Engines in einer kohärenten RAG-Architektur.

**Schlüsselkomponenten**:

1. **Collection<S: StorageEngine>** (pro-Namespace-Isolation):
   - Generic über `StorageEngine` (aktuell nur `LsmStorage`, aber erweiterbar)
   - Bietet Unified API über alle 4 Signale
   - Methoden:
     - `insert(doc_id, embedding, metadata)` → Schreibt in Store + Index + Graph + Text
     - `hybrid_search(text_query, vector_query, k, filters)` → RRF-fusioniert alle 4 Signale
     - `hybrid_search_with_strategy(strategy)` → Mit Custom-Retrieval-Strategie (z.B. Community-basiert)
     - `relate(from_id, to_id, weight)` → Entity-Relation-Graph-Schreiber
     - `insert_typed(doc_id, memory_type, embedding)` → Typed Insert (Episodic/Semantic/etc., ADR-020)
     - `sweep_decay(tx_id)` → Decay-Sweep für Memory-Importance (ADR-025)

2. **HybridQuery** (DTO für komplexe Abfragen):
   - `text_query`, `vector_query`, `graph_start_node`
   - `fusion: FusionStrategy` (WeightedSum | ReciprocalRankFusion{k} | RrfWeighted)
   - `filter: FilterExpr`
   - `memory_type_filter: Option<MemoryType>` (Neu, ADR-025)

3. **FusionStrategy** (ADR-003, ADR-021):
   - `WeightedSum` (Score-basiert, erfordert Skalenkalibrierung)
   - `ReciprocalRankFusion { k: 60 }` (Rank-basiert, Default, robust)
   - `RrfWeighted` (RRF mit zusätzlichen Gewichten pro Modalität)

4. **MultiStepEngine** (OpenAI o-series Pattern, ADR-021):
   - Iteratives Query-Rewriting (max. 3 Runden)
   - Nutzt Ollama für Query-Expansion
   - Pseudocode:
     ```
     q_0 = original_query
     results = ∅
     for i in 1..=3:
       r_i = hybrid_search(q_i)
       results ∪= r_i
       if confidence(r_i) > threshold: break
       q_{i+1} = expand_query(q_i, r_i)
     ```

5. **ContextCompactor** (Grok Pattern, ADR-019):
   - `consolidate_via_llm(chunks, ollama)` → LLM-Summarization
   - Nutzt Ollama für Kontext-Zusammenfassung
   - Outputs: `CompactedContext` (reduzierte Chunks + StatusToken)
   - Geplant: `consolidate_via_llm_with_provenance()` mit Herkunfts-Tracking (Achse B, ADR-032)

6. **Context Compaction Strategy** (Umbenannt von `CompactionStrategy`, ADR-040, einzige Breaking Change):
   - Trigger: TokenBudget überschritten, Age-Threshold erreicht, Manual
   - Fold-Batch-Size (wie viele Chunks zusammenfassen)
   - Output: `FoldedSegment` (Summary + Pointers zu Originalen)

7. **ContextPrefixEngine** (Anthropic Pattern, ADR-019, in memfuse-ollama):
   - Vor BM25-Scoring: Generiert LLM-Kontext-Präfix für jeden Chunk
   - `combined_text_owned()` — Konstruiert "[CONTEXT] {prefix} [TEXT] {actual_text}"
   - Effekt: 49% weniger Retrieval-Fehler (laut README)

8. **MetadataFilter** (FilterExpr-Konvertierung, ADR-040):
   - `FilterExpr` (user-facing AST)
   - `MetadataFilter` (interne, evaluierende Repräsentation)
   - TryFrom-Konvertierung mit Fehlerbehandlung

9. **Transaktions-API** (ADR-033, ADR-024):
   - `allocate_tx()` (Transaktions-ID-Generator)
   - `begin_tx()` / `commit_tx()` / `rollback_tx()`
   - MVCC-konsistente Leseversion über `SnapshotId` (TxId)

**Hybrid-Search-Pipeline** (ADR-021):
```
Contextual Ingestion (ADR-019)
    ↓ (LLM-generierter Präfix pro Chunk)
4-Signal Indexing (HNSW + BM25 + Graph + Metadata)
    ↓
MultiStep Query Expansion (OpenAI Pattern, optional, max 3 Runden)
    ↓
RRF Fusion (Reciprocal Rank Fusion, robust gegen Scale-Mismatch)
    ↓
Cross-Encoder Reranking (ONNX, ADR-021, optional)
    ↓
Context Compaction (Token-Reduktion, Grok Pattern)
    ↓
Final Kontext → LLM-Prompt
```

**Bekannte Limitierungen**:
- `hybrid_search` ist synchron (async wrapper möglich, aber komplex)
- RRF-Parameter (k=60) sind nicht tunable pro Query (Roadmap)
- Cross-Encoder-Reranking optional (Feature-Flag `onnx` in memfuse-embed)

---

### 2.4 Layer 3: Application Services (5 Crates + 1 optional)

#### 2.4.1 memfuse-py — Python-Bindings (PyO3)

**API-Oberflächentypen**:
- `PyMemFuse` (wrapper um `MemFuse`)
- `PyCollection` (wrapper um `Collection`)
- `PySearchResult` (Treffer + Scores)
- `PyDocument` (DocId + Embedding + Metadata)
- Stats-DTOs: `PyVectorIndexStats`, `PyStorageStats`, `PyDbStats`

**Async-Handling**: `#[pyo3(signature = (..., py))]` mit `pyo3_asyncio` für Async-Methoden.

**Error-Mapping** (ADR-037, Teil der FFI-Härtung #1044):
- `MemFuseError` → `PyErr` via strukturiertem `FfiError`
- Code + Message + Retry-Flag als Python-Exception-Args

**Bekannte Limitierungen**:
- PyO3 GIL-Freigabe für Blocking-Calls nötig
- Complex Types (Nested Structs) benötigen Custom Conversion

---

#### 2.4.2 memfuse-ollama — LLM & Embedding Backend

**Zweck**: HTTP-Client für Ollama (lokal laufende LLM/Embedding-Service).

**Komponenten**:
- `OllamaClient` (HTTP-Wrapper via reqwest, ADR-039 recent approval)
- `OllamaEmbedder` (implementiert `TextEmbeddingEngine` Trait)
- `OllamaTextGenerator` (für Contextual Retrieval + Query Expansion)
- `ContextPrefixEngine` (Anthropic Pattern, LLM-generierter Präfix-String)
- `ModelInfo` (Metadata: Dimension, Architektur, Tokenizer-Info)

**Nicht im Scope**: GPU-Optimierung, Quantization (Sache von Ollama).

**Bekannte Limitierungen**:
- Abhängig von externem Ollama-Service (keine Fehlertoleranz, wenn Ollama down)
- HTTP-Latenz (Netzwerk-Round-Trip pro Embedding)
- Modellwahl und Parameter sind Nutzer-Verantwortung

---

#### 2.4.3 memfuse-agent — Persistent Agent Workflow Engine

**Architektur**: Checkpoint→Execute→Commit→Audit-Loop (ADR-020 partial restoration).

**Komponenten**:
- `OrchestratorEngine` (State-Machine-Orchestrator)
- `AgentTool` Trait (Werkzeug-Schnittstelle, z.B. Search, Relate, Sleep-Cycle)
- `AgentContext` (Ausführungskontext: Session-ID, Checkpoint, Permissions)
- `WorkflowGraph` (DAG von Tool-Aufrufen)
- `AuditLog<S: StorageEngine>` (Generisch, ermöglicht Mock in Tests, ADR-040)
- `SandboxBridge` Trait → `#[async_trait]` (ADR-040)

**Workflow-Muster** (geplant, Phase 2–3):
- Sleep-Cycle-Konsolidierung (Memory Consolidation, ADR-032)
- Proactive Foresight (EverMemOS/CogniFold Pattern, geplant Phase 3)
- Decay-Sweep (MemoryGovernance Lifecycle, ADR-025)

**Bekannte Limitierungen**:
- Workflow-Persistierung ist All-or-Nothing (kein inkrementeller Commit)
- Agent-Parallelisierung nicht unterstützt (Single-Threaded Orchestrator)
- Tool-Fehler können Workflow unterbrechen (kein Auto-Retry-Mechanismus)

---

#### 2.4.4 memfuse-embed — ONNX-Embeddings & Reranking (Optional)

**Status**: 🧊 Optional (Feature-Flag `onnx`)

**Komponenten**:
- `OnnxEmbedder` (Locale Embedding-Modelle via ONNX Runtime)
- `CrossEncoderReranker` (Post-RRF Neuordnung, bis zu 3 Top-k-Results pro Query)
- `SessionPool` (ONNX-Session-Caching, Performance-Optimierung)

**Embedding-Modelle** (vom Nutzer bereitgestellt):
- `bge-base-en-v1.5.onnx` (384-dim, oder ähnlich)
- Tokenizer: `tokenizer.json` (HuggingFace format)

**Reranking-Qualität** (laut README):
- 67% weniger Fehler kombiniert mit Contextual Retrieval (Benchmark)

**Bekannte Limitierungen**:
- ONNX Runtime binary ist groß (~100 MB+) → Feature-Flag sinnvoll
- Modell-Download ist Nutzer-Verantwortung (kein auto-download)
- Session-Pool ist nicht thread-safe (Arbeit für Multi-Agent)

---

#### 2.4.5 memfuse-router — Routing & Cascading Decision Engine

**Zweck**: Model-Cascading für kostenoptimale Routing (geplant: UCCI-Kalibrierung, ADR-038 future).

**Komponenten** (geplant):
- `RouterEngine` (Routing-Entscheidung: small profile vs. large profile)
- `IsotonicCalibrator` (Kalibrierung roher Scores auf Fehlerwahrscheinlichkeit, UCCI-Pattern, geplant)
- `CascadeCostConfig` (Kostenmodell)

**Bekannte Limitierungen** (Roadmap Phase 3–4):
- Aktuell: einfache Cascading (Score-Aggregation + 1.2× Community-Boost)
- UCCI-Kalibrierung benötigt Feedback-Signal (Korrektheits-Labels), noch nicht vorhanden
- Kalibrierungskurve ist nicht multi-tentative (Single Curve für alle Queries)

---

### 2.5 Layer 4: User-Facing Interfaces

#### 2.5.1 memfuse-mcp — Model Context Protocol Server

**Transport**: stdio JSON-RPC 2.0 ONLY (ADR-010, HTTP-Stub wurde entfernt).

**Tools**:
- `memfuse_search` (Hybrid-Suche mit Query-String und Embedding)
- `memfuse_insert` (Dokument-Ingestion mit optionalem Metadaten)
- `memfuse_get` (DocId-Lookup)
- `memfuse_relate` (Entity-Relation-Graph-Schreiben)
- `memfuse_collections` (Auflistung aller Collections)

**Sandbox** (`McpSandbox`, ADR-010):
- DB-Reads: erlaubt (Standard)
- DB-Writes: opt-in via `SandboxPolicy`
- Code-Execution: opt-in
- Tool-Outputs: AES-256-GCM-SIV verschlüsselt, Zeroize on Drop

**Error-Mapping** (neu, #1044):
- `MemFuseError` → `MemFuseErrorDto` (strukturiert) → JSON-RPC `data` Feld
- Client-seitig kann Error-Code (.args[0]) auslesen statt Message-Parsing

**Integration**: Für Claude Desktop, aber herstellerunabhängig.

---

#### 2.5.2 memfuse-tauri — Desktop Application

**Architektur**: Tauri (Electron-ähnlich, Rust-Backend + Web Frontend).

**Sicherheit** (ADR-NNN):
- Path Traversal-Schutz für Ingestion (Commit #1039, #1043)
- File-Size Limits (Commit #1039)
- XSS-Prevention via escapeHtml (Commit #1025)
- HNSW VectorIndex Recursion Guard (Commit #1043)

**Commands** (Tauri-IPC zu Rust):
- `search`, `insert`, `get_collection_stats`
- `import_documents` (Batch-Ingestion aus Dateisystem)
- `start_ollama_service` (Wrapper für Ollama start)

**State Management**:
- `AppState` (DB-Reference, Ollama-Client, Router)
- Window-Persistierung (Größe, Position)

**Ingestion Pipeline** (Commit #1038):
- Markdown/PDF/Word → `MarkdownChunker` → Contextual Prefix → Embedding → Insert

**Bekannte Limitierungen**:
- Desktop-App ist nur auf Windows/macOS/Linux mit Tauri-Support
- Ollama muss separat gestartet sein (kein integrated Launcher)
- Ingestion ist Batch-only (kein Real-Time Streaming)

---

## 3. Die 4 Signale der Hybrid-Suche

### 3.1 Signal 1: Vektorsuche (Semantik)

**Engine**: memfuse-index::HnswIndex
**Scoring**: Cosine-Similarity via SIMD (0.0–1.0, höher = ähnlicher)
**Charakteristik**: Erfasst semantische Ähnlichkeit ("Was bedeutet ähnliches?")
**Beispiel**: Query: "Machine Learning Techniken"
- Treffer: "Deep Learning", "Neural Networks" (hohe Score)
- Miss: "Elektro-Architektur" (niedrig)

---

### 3.2 Signal 2: Volltextsuche (Lexikalisch)

**Engine**: memfuse-text::BM25
**Scoring**: BM25-Formel (0.0–∞, höher = mehr Terme)
**Charakteristik**: Erfasst Wort-Vorkommen ("Was enthält die genauen Begriffe?")
**Deutsche Morphologie**: "Urlaubsantragsprozess" matched auch "Antrag"
**Beispiel**: Query: "Urlaubsantrag"
- Treffer: "Der Urlaubsantrag wurde genehmigt" (exakte Treffer)
- Miss: "Vacationrequest" (anderer Term)

---

### 3.3 Signal 3: Wissensgraph-Traversal (Assoziativ)

**Engine**: memfuse-graph::CsrGraph (PPR oder traverse_at_time)
**Scoring**: Edge-Weights + Hop-Decay oder PPR-Score
**Charakteristik**: Erfasst relationaler Assoziationen ("Was ist mit Entity X verbunden?")
**Multi-Hop**: Kann mehrstufige Relationen aufdecken
**Beispiel**: Entity "CEO" Query
- Direkter Treffer: Company → CEO (1-Hop)
- Indirekter: Industry → Company → CEO (2-Hop via PPR)

**Bi-Temporal Filtering** (ADR-033):
- `traverse_at_time(start, tx_id)` — Findet Relationen, die zu bestimmtem Zeitpunkt gültig waren

---

### 3.4 Signal 4: Metadaten-Filterung (Strukturell)

**Engine**: memfuse-db::MetadataFilter
**FilterExpr**: AST-basiert (AND/OR/NOT, Prädikate über Metadata-Felder)
**Scoring**: Boolean (include/exclude, nicht graduell)
**Charakteristik**: Erfasst strukturelle Constraints ("Nur Dokumente mit Tags=[X,Y] und Datum > 2026-01-01")
**Beispiel**: `(tag='employee' AND department='engineering' AND created_at > '2026-01-01')`
- Treffer: Engineering-Docs 2026+
- Miss: Sales-Docs, Historic-Docs

---

### 3.5 RRF-Fusion (Kombination aller 4)

**Algorithmus**: Reciprocal Rank Fusion (ADR-003, ADR-021)

**Score pro Document**:
```
rrf_score(d) = Σ_m 1/(k + rank_m(d))
```
wobei:
- `m` = Modalität (Vektor, Text, Graph, Metadaten)
- `rank_m(d)` = Rang von `d` in Modalität `m` (1 = Top, ∞ = nicht vorhanden)
- `k` = 60 (Standard, balanciert Head vs. Long-Tail)

**Robustheit**: RRF arbeitet auf Rängen, nicht auf Scores → kein Skalierungsproblem zwischen 0.0–1.0 (Cosine) vs. 0.0–∞ (BM25).

**Beispiel**:
```
Document: "Machine Learning Paper"

Signal 1 (Vector):    rank=3, score 0.85
Signal 2 (Text):      rank=1, score 8.5
Signal 3 (Graph):     rank=5, (low relevance edge)
Signal 4 (Metadata):  rank=∞, (filtered out)

RRF = 1/(60+3) + 1/(60+1) + 1/(60+5) + 0
    = 0.0149 + 0.0161 + 0.0147
    = 0.0457
```

---

## 4. Cognitive OS-Kernfeatures

### 4.1 Contextual Retrieval (ADR-019, Anthropic Pattern)

**Feature**: LLM-generierter Kontext-Präfix vor BM25-Scoring.

**Mechanik**:
1. Vor der Ingestion: Ollama generiert Kontext-String für jeden Chunk
   ```
   Chunk: "The algorithm is O(n log n)"
   Context: "This describes time complexity in computer science, related to sorting algorithms"
   ```
2. Bei Retrieval: BM25 scored über "[CONTEXT] {prefix} [TEXT] {chunk_text}"

**Effekt**: 49% weniger Retrieval-Fehler (README-Benchmark).

**Implementation**: `ContextPrefixEngine` in memfuse-ollama.

---

### 4.2 Cross-Encoder Reranking (ADR-021, ONNX)

**Feature**: Nach RRF-Fusion werden Top-k Results neu geordnet via Cross-Encoder.

**Mechanik**:
1. RRF produziert Top-100 Results (fusioniert alle 4 Signale)
2. Cross-Encoder (ONNX) scoret jedes Paar (Query, Result) direkt
3. Top-20 werden nach Cross-Encoder-Score neu geordnet

**Effekt**: 67% weniger Fehler kombiniert mit Contextual Retrieval.

**Implementation**: `CrossEncoderReranker` in memfuse-embed (`--features onnx`).

---

### 4.3 Multi-Step Query Expansion (OpenAI o-series Pattern)

**Feature**: Iteratives Query-Rewriting für komplexe Abfragen.

**Mechanik**:
1. User Query: "How do I deploy on AWS?"
2. Iteration 1: Suche nach ["AWS", "deploy"]
   - Wenn Confidence < Threshold, gehe zu Iter 2
   - Query Expansion: "How can I deploy applications on AWS, including EC2 and Lambda?"
3. Iteration 2: Suche nach erweiterte Query
4. Max 3 Iterationen

**Effekt**: Bessere Recall für Mehrteil-Fragen.

**Implementation**: `MultiStepEngine` in memfuse-db.

---

### 4.4 Session DAG Branching (Grok Pattern, ADR-NNN)

**Feature**: Persistent Conversation Branching (nicht linear Timeline).

**Mechanik**:
1. User-Session wird als DAG persistiert (nicht Linked-List)
2. User kann jede Konversations-Branch zurück und neu-exploren
3. Jede Branch hat eigenen komprimierten Kontext (`SessionBranchTree`)

**Effekt**: Multi-Path-Exploration ohne Neustart.

**Implementation**: `SessionBranchTree` + `AgentStateNode` in memfuse-graph.

---

### 4.5 Context Compaction (Grok Pattern, ADR-019)

**Feature**: Intelligente Token-Reduktion durch LLM-Summarization.

**Mechanik**:
1. Wenn Token-Budget des Kontexts voll: Compactor lädt alte Chunks
2. LLM fasst zusammen: z.B. 10 Chunks → 1 Summary-Chunk
3. Original-Chunks bleiben als Pointers (drill-down möglich)

**Effekt**: Unbegrenzte Conversation-Länge, konstantes Token-Budget.

**Implementation**: `ContextCompactor` in memfuse-db.

---

### 4.6 MCP Sandbox Isolation (Anthropic Containment Pattern, ADR-010)

**Feature**: Sichere Tool-Ausführung mit AES-256-GCM-SIV Encryption.

**Mechanik**:
1. Agent-Tool wird in Sandbox aufgerufen
2. DB-Reads: erlaubt (default)
3. DB-Writes: opt-in (`SandboxPolicy`)
4. Code-Execution: opt-in
5. Tool-Output wird AES-256-GCM-SIV verschlüsselt
6. auf `VolatileToolResult` Drop: Zeroize (sichere Speicher-Löschung)

**Effekt**: Vertrauenswürdige Tool-Ausführung ohne Seitenkanalgefahren.

**Implementation**: `McpSandbox` in memfuse-mcp.

---

## 5. Kryptographie & Sicherheit

### 5.1 Übersicht (aus CONSTITUTION.md § 1)

**Sovereign Core Doctrine**:
- Memory Safety: Safe Rust bevorzugt
- No Panics: Fehlerfortpflanzung statt Crashes
- WAL-First: Keine In-Memory-Änderung vor WAL-Sync
- Deterministic Recovery: State rekonstruierbar aus Logs

### 5.2 Encryption-at-Rest (AES-256-GCM)

**Wo**: LSM-Tree Disk-Writes (memfuse-store)
**Key Management**: `load_or_create_integrity_key()` (lokale Datei, aktuell kein HSM)
**IV**: Random per Block
**Auth**: GCM-Tag

### 5.3 Integrity Chaining (HMAC-SHA256, ADR-029)

**Wo**: Write-Ahead-Log (memfuse-store::wal.rs)
**Mechanik**: Jeder WAL-Entry trägt HMAC(data ⊕ prev_hmac)
**Zweck**: Bit-Flip-Detektion, Silent Corruption Prävention
**Recovery**: Aus HMAC-Abweichung wird `WalCorruption` Error → Safe Shutdown

### 5.4 Tool-Output Encryption (AES-256-GCM-SIV, ADR-010)

**Wo**: MCP Tool-Outputs (memfuse-mcp)
**Typ**: Committing AEAD (GCM-SIV für Determinismus)
**Zeroize**: `VolatileToolResult` zeroized auf Drop

---

## 6. 4-Phasen-Roadmap mit aktuellem Status

### Phase 1: RAG-Fundament ✅ (Abgeschlossen, HEAD 4162ebb)

**Ziel**: Vollständige 4-Signal-RAG mit Cognitive-OS-Kernfeatures.

**Deliverables**:

| Sprint | Feature | Status | Commits |
|--------|---------|--------|---------|
| RAG-01 | Contextual Retrieval | ✅ | ADR-019 |
| RAG-02 | Cross-Encoder Reranking | ✅ | ADR-021 |
| RAG-03 | Multi-Step Query Engine | ✅ | ADR-021 |
| RAG-04 | Context Compaction | ✅ | ADR-019 |
| RAG-05 | Session DAG + MCP Sandbox | ✅ | ADR-010, ADR-020 |

**Phase 1 Arkitektur**: LSM-Store + HNSW + BM25 + CSR-Graph + RRF-Fusion vollständig umgesetzt.

**CI-Status**: ✅ All tests green, `just check` passes.

---

### Phase 2: Cognitive Memory 📋 (Q4 2026, geplant)

**Ziel**: Explizite kognitive Gedächtnistypen und temporale Graphen.

**Geplante Features**:

| Feature | Type | Abhängigkeit | ADR |
|---------|------|--------------|-----|
| Memory Type Classification | Episodic / Semantic / Procedural / Working | core | ADR-025 (partial: decay) |
| Bi-Temporal Graph | Validitätszeit + Transaktionszeit | Phase 1 | ADR-033 |
| Memory Importance Score | LLM-bewertet, Decay-Funktion | Phase 1 | ADR-025 |
| Recency Decay | Math. Verfallsfunktion episodischer Mem. | core | ADR-025 |

**Implementierungsplan**:
1. Extend `MemoryType` enum (core)
2. `insert_typed(doc_id, memory_type, embedding)` Methode in Collection
3. `sweep_decay(tx_id)` für Lifecycle-Management
4. Vergleich gegen Mem0/MemOS Importance-Scoring-Benchmarks

---

### Phase 3: Selbstorganisierung 📋 (Q1 2027, geplant)

**Ziel**: Automatische Reflexion, Konsolidierung, Multi-Hop-Retrieval.

**Geplante Features**:

| Feature | Scope | Abhängigkeit | Pattern |
|---------|-------|--------------|---------|
| Memory Consolidation | Auto-Summarization veralteter Chunks | Phase 2 | Sleep-Cycle (ADR-032) |
| Personalized PageRank (PPR) | Graph-basiertes Multi-Hop-Retrieval | Phase 1 | ADR-026 (partial: implementiert) |
| Community Detection | Wissensgraph-Clustering | Phase 1 | ADR-027 (partial: implementiert) |
| A-MEM Zettelkasten | Explizite Querverweise zwischen Mem. | Phase 2 | A-MEM Pattern |

**Implementierungsplan**:
1. Sleep-Cycle-Tool als AgentTool (Agent-Framework)
2. Decay-basierte Triggering (Memory-Lifecycle-Manager)
3. Consolidation-Strategie (LLM-gesteuert vs. regelbasiert)

---

### Phase 4: Enterprise & Skalierung 📋 (Q2 2027, geplant)

**Ziel**: Enterprise-Sicherheit, Multi-Tenancy, Benchmarks.

**Geplante Features**:

| Feature | Scope | Abhängigkeit | Notes |
|---------|-------|--------------|-------|
| OAuth 2.0 | MCP-Server Auth | Phase 3 | Nur für Server, nicht Desktop |
| RBAC | Role-Based Access Control | Phase 3 | Collection-Ebene Permissions |
| Multi-Tenant Isolation | Logisch + kryptographisch | Phase 3 | Separate LSM-Trees pro Tenant |
| Immutable Audit-Trail | Append-only Ops-Log | Phase 3 | Für Compliance |
| Benchmark Suite | SOTA vs. Mem0/Zep/MemOS/MIRIX | Phase 4 | Standardisierte Metriken |

---

## 7. Quality Gates & Governance

### 7.1 Triple-Test-Gate (vor jedem Commit)

```bash
# 1. Typensystem & Kompilierbarkeit
cargo check --workspace --exclude memfuse-tauri

# 2. Gesamte Testsuite
cargo test --workspace --exclude memfuse-tauri

# 3. Flaky-Test-Detektor (3× Läufe)
just triple-test

# 4. Format + Lint
just check

# 5. DAG-Validierung
just dag-check

# 6. Debt-Scan (unwrap/expect/std::fs)
just debt-audit

# 7. Docs-Sync
just sync-docs
```

### 7.2 Exit Criteria (Definition of Done)

1. **Alle AI-TAG/TODO-Einträge gelöst oder getrackt**
2. **Gate-Stack: grün** (`just check` + `cargo test` + `just triple-test`)
3. **Architektur-Entscheidungen dokumentiert** (ADR in DECISIONS.md)
4. **Keine BLOCKER/CRITICAL Security-Risiken offen**
5. **WORKING_STATE.md aktualisiert** (via `just sync-docs`)

### 7.3 Invarianten-Status (aus SOURCE_OF_TRUTH.md)

| Invariante | Status | Details |
|-----------|--------|---------|
| Snapshot Isolation | 🟢 Complete | Storage via `scan_prefix_at`, Text via BM25 `search_at`, Graph/Vektor NICHT isoliert |
| Zero Panic | 🟡 In Progress | 5 offene `.expect()` stellen: SessionPool + snapshot.rs |
| Pure Rust (Core) | 🟢 Complete | Layer 0–2 sind 100% Safe (exkl. SIMD + Mmap unsafe) |
| DAG Integrity | 🟢 Complete | Alle 15 Crates respektieren Layer-Grenzen |
| CI-Verified | ✅ All Green | Letzter Commit #73dd4d1 |

---

## 8. Feature-Flags & Build-Varianten

### 8.1 Workspace-Level Features

Keine; alle Features sind per-Crate.

### 8.2 Per-Crate Features

| Crate | Feature | Default | Zweck |
|-------|---------|---------|--------|
| memfuse-db | `experimental-diskann` | NO | Disk-resident Vamana Index (Phase 3 Preview) |
| memfuse-db | `sandbox` | NO | MCP Sandbox-Modus |
| memfuse-db | `cluster` | NO | Distributed/Clustering (geplant, derzeit noop) |
| memfuse-db | `bench` | NO | Benchmark-Utilities |
| memfuse-embed | `onnx` | NO | ONNX Runtime + CrossEncoderReranker |
| memfuse-index | `graph` | NO | CSR-Graph Modul (eigentlich immer YES, TODO: refactor) |
| memfuse-index | `experimental-diskann` | NO | DiskANN Index (Vamana) |
| memfuse-mcp | `agent-workflows` | NO | Agenten-Integration im MCP-Server |
| memfuse-router | `ollama` | NO | Ollama-Integration im Router |
| memfuse-agent | `test-utils` | NO | Test-Utilities (Mock-Agenten) |
| memfuse-crypto | `test-utils` | NO | Test-Utilities (Zeroize-Verifikation) |

### 8.3 Empfohlene Build-Varianten

**Development**:
```bash
cargo build --workspace --exclude memfuse-tauri
```

**Produktion (alle Features)**:
```bash
cargo build --release --workspace --all-features
```

**Minimal (nur Core-Engine, kein Embedding/Router)**:
```bash
cargo build --release -p memfuse-db
```

---

## 9. APIs & Integrationspunkte

### 9.1 Rust Library API (Primary Use-Case)

```rust
use memfuse_db::MemFuse;

#[tokio::main]
async fn main() -> Result<()> {
    let db = MemFuse::open("./my_data").await?;
    let col = db.collection("documents").await?;

    // Insert
    col.insert("doc-1", &embedding_vec, Some(json!({"title": "..."}))).await?;

    // Hybrid Search
    let results = col.hybrid_search(
        "my question",
        &query_embedding,
        5,
        None,
    ).await?;

    for result in results {
        println!("{}: {}", result.doc_id, result.score);
    }

    Ok(())
}
```

### 9.2 MCP Server API (Claude Desktop, anderen Clients)

**Protocol**: stdio JSON-RPC 2.0

**Tools**:
- `memfuse_search`
- `memfuse_insert`
- `memfuse_get`
- `memfuse_relate`
- `memfuse_collections`

**Invocation**:
```bash
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./data
```

### 9.3 Python API (PyO3)

```python
import memfuse

db = memfuse.MemFuse("./my_data")
col = db.collection("documents")

col.insert("doc-1", embedding, {"title": "..."})

results = col.hybrid_search("question", query_embedding, k=5)
for doc in results:
    print(f"{doc.doc_id}: {doc.score}")
```

### 9.4 Desktop App API (Tauri Commands)

**IPC via Tauri**:
- Frontend → `invoke('search', {query, embedding, k})`
- Backend executes, returns JSON

---

## 10. Bekannte Grenzen & Zukunftsarbeit

### 10.1 Performance & Skalierbarkeit

| Limitation | Root Cause | Mitigation (Phase 3+) |
|-----------|-----------|----------------------|
| HNSW rein In-Memory | Storage-Design | DiskANN/Vamana Index (Feature-Flag) |
| No KV-Cache Reuse | Inference-Layer Disconnect | memfuse-kv (Retrieval↔Inference Bridge, geplant) |
| Sync hybrid_search | Async-Overhead | Async-Wrapper oder Tokio Integration |
| No Multi-Agent Parallelism | Single OrchestratorEngine | Agent Pool (Phase 3+) |

### 10.2 Sicherheit & Privacy

| Issue | Status | Roadmap |
|-------|--------|---------|
| Kein Key-Rotation | 🔴 Open | Phase 4 (Enterprise) |
| No per-User Encryption | 🔴 Open | Multi-Tenant Isolation (Phase 4) |
| No Audit-Trail Immutability | 🔴 Open | Immutable Logs (Phase 4) |
| Zero-Panic: 5 offene `.expect()` | 🟡 In-Progress | Phase 1 finale Polish |

### 10.3 Kognitiv & ML

| Limitation | Reason | Next Step |
|-----------|--------|-----------|
| No Memory Consolidation | Requires LLM-Agent Loop | Phase 3 Sleep-Cycle |
| No Importance Weighting | Baseline all memories equally | Phase 2 LLM-Scoring + Decay |
| No Proactive Foresight | Advanced cognitive pattern | Phase 3 (EverMemOS/CogniFold-Pattern) |
| No Zettelkasten Querverweise | Requires explicit cross-refs | Phase 3 A-MEM Pattern |

---

## 11. Quellenverzeichnis

### Primärquellen (Repository)

- **README.md** — Product Vision, Features, Comparison
- **CONSTITUTION.md** — Governance Principles & Quality Philosophy
- **AGENTS.md** — Operative Rules & Judgment Boundaries
- **DECISIONS.md** — 40 ADRs (append-only log)
- **SOURCE_OF_TRUTH.md** — Living State: Crate Inventory, Roadmap
- **docs/memfuse_strategic_roadmap.md** — 4-Phase Strategic Plan
- **docs/ARCHITECTURE.md** — Auto-generated DAG & Layer Topology
- **docs/TYPE_REGISTRY.md** — Central Type & Trait Index

### Forschungsquellen (aus Spezifikationen)

| Paper | Relevanz | Verifikationsstatus |
|-------|----------|-------------------|
| MAGMA (2601.03236) | Multi-Graph Agentic Memory (Roadmap Phase 2) | ✅ ACL 2026 Main |
| EverMemOS (2601.02163) | Self-Organizing Memory OS | ✅ Verifiziert |
| CogniFold (2605.13438) | Proactive Memory via Cognitive Folding | ✅ v4 2026-08-05 |
| VMG-Survey (2604.16548) | Long-Term Memory Security | ✅ MemTensor Shanghai |
| Auto-Dreamer (2605.20616) | Offline Memory Consolidation | ✅ 2026-05-20 |
| UCCI (2605.18796) | Cost-Optimal Cascade Routing | ✅ SOTA 31% cost reduction |
| Mem0 (ECAI-2025) | Competitive Memory Product | ✅ Benchmark Reference |

---

## Schlusswort

**MemFuse Brain** ist ein ambitioniertes Projekt, das LLM-Memory nicht nur als Vektoren speichert, sondern als ein **Cognitive Operating System** ausgestaltet: Das nutzt proprietäre 4-Signal-Fusion, lokale LLM-Integration, MCP-native Tooling und eine transparent dokumentierte, 4-Phasen-Roadmap zum Aufbau wirklich kognitiver Gedächtnis-Architekturen.

Die Codebasis ist **reif für Phase 1** (RAG-Fundament), mit starken Governance-Strukturen und laufender Community-Entwicklung. Phase 2–4 setzen auf gut dokumentierte Forschungsmuster (MAGMA, EverMemOS, A-MEM) statt ad-hoc-Design.

---

**Version**: 2.0  
**Zuletzt aktualisiert**: 2026-08-29  
**HEAD Commit**: 73dd4d1 (Harden memfuse-store)  
**LOC**: ~60.786 (alle Crates)  
**Status**: Phase 1 ✅ Complete; Phase 2–4 📋 In Design
