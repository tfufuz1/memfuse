# MemFuse — Konsolidierte Gesamtspezifikation v6.0

> **Dokument-Typ:** Normative Gesamtspezifikation — einzige maßgebliche Wahrheitsquelle  
> **Version:** 6.0 — „Verified Continuity" (löst v5.0 vollständig ab)  
> **Stand:** 07. September 2026 (Abend-Audit)  
> **HEAD:** `05b382d8` · ~117.200 LOC Rust · 18 Workspace-Crates + xtask  
> **Konsolidiert aus:**  
> — `memfuse_gesamtspezifikation_v5_0.md` (HEAD `bb099dc2`, normative Basis)  
> — `memfuse_konsolidierung_und_fortschritt.md` (HEAD `05b382d8`, 8 Jules-Prompts)  
> — Live-Code-Audit HEAD `05b382d8` (Principal-Architect-Review, 07.09.2026 Abend)  
> **Syntheseprinzip:** Jede Aussage ist (a) live-code-verifiziert (HEAD `05b382d8`) **oder** (b) arXiv-belegt **oder** (c) aus expliziter Konfliktlösung mit dokumentiertem Entscheid. Keine unverifizierten Übernahmen aus v5.0.

---

## Inhaltsverzeichnis

- **§0** Methodische Grundlagen, Konfliktlösungsmatrix v6.0 & Quell-Hierarchie
- **§1** Produktvision, Säulen & Architekturprinzipien (P1–P12)
- **§2** Crate-Topologie — Vollständige Ziel-Architektur (18 Crates)
- **§3** Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie
- **§4** Layer 1 — Storage-Primitiven
- **§5** Layer 2 — Orchestrierung & Fusion
- **§6** Layer 3 — Inferenz, Routing & Physio-Selbstregulierung
- **§7** Layer 4 — Integrations-Grenzschicht
- **§8** Layer 5 — Evaluation & Benchmarking
- **§9** Physio-Feature-Katalog (F-01 bis F-11, mit Implementierungsstatus)
- **§10** PhysioScheduler & PhysioConfig
- **§11** Invarianten-Verzeichnis (normativ)
- **§12** Implementierungsstand & Priorisierte Roadmap
- **§13** Definition of Done
- **§14** Governance & Prozessmodell
- Anhang A: Wettbewerbspositionierung
- Anhang B: Verworfene Features (permanent)
- Anhang C: ArXiv-Paper-Verzeichnis (Tier 1–3)
- Anhang D: Technische Schulden (Stand v6.0)

---

## §0 Methodische Grundlagen, Konfliktlösungsmatrix v6.0 & Quell-Hierarchie

### §0.1 Hierarchie der Quellen (unveränderlich)

Wo Quellen sich widersprechen, gilt:

1. **Live-Code HEAD `05b382d8`** — schlägt alle Dokumente
2. **Konsolidierungsdokument HEAD `05b382d8`** (Jules-Prompts, Abend) — schlägt v5.0-Behauptungen
3. **v5.0 Spec HEAD `bb099dc2`** — Basis-Referenz, korrigiert wo Code abweicht
4. **ArXiv-Paper** (nach Datum) — für algorithmische Entscheidungen
5. **PRD-Features** — nur soweit durch (a)–(d) stützbar

### §0.2 Vollständige Konfliktlösungsmatrix v6.0

Die folgende Tabelle dokumentiert jeden Widerspruch zwischen v5.0-Spec und HEAD `05b382d8`:

| # | Konflikt | v5.0-Aussage | Code-Realität (`05b382d8`) | **Normative Auflösung v6.0** |
|---|---|---|---|---|
| **K1–K10** | Alle v5.0-Konflikte | Wie in v5.0 normativ aufgelöst | Bestätigt implementiert | **Bestätigung**: K1–K10 bleiben gültig. Cascade-Tombstone (P1), VETOES.md (P0-GOV-1), CI-VETOES-Check (P0-GOV-2) und memfuse-py-Isolation (ADR-064) sind vollständig umgesetzt. |
| **K11** | F-09 Resonanz-Kohärenz-Bonus Feature-Flag | v5.0: ✅ Produktiv (Commit #1698) | `#[cfg(feature = "physio-resonance-fusion")]` in `fusion.rs`, aber **kein** `physio-resonance-fusion`-Eintrag in `crates/memfuse-db/Cargo.toml [features]`. Feature ist in **keinem** Build aktivierbar — es ist totes, unaufrufbares Code. | **SOFORT FIXEN** — `physio-resonance-fusion = []` in `crates/memfuse-db/Cargo.toml` hinzufügen. Status bis Fix: ⛔ unaktivierbar trotz Implementierung. |
| **K12** | TenantId-Sicherheitsinvariante INV-TENANT-1 | v5.0: `try_new(0) → Err`, vollständige Prüfung | `TenantId::new(0)` (const fn, kein Guard), `TenantId::from(0u64)` (From-Impl, kein Guard) — **beide umgehen die Sicherheitsprüfung**. `SYSTEM`, `DEFAULT`, `INVALID` zeigen alle auf `TenantId(0)`, semantisch mehrdeutig. | **`TenantId::new()` deprecieren.** `From<u64>` auf `try_new`-basierte Variante migrieren. `TenantId::SYSTEM` ist der einzige legitime Weg, id=0 zu halten. INV-TENANT-1 gilt erst als durchgesetzt, wenn `const fn new()` keine 0 mehr akzeptiert. P1-Fix. |
| **K13** | F-07 Replikatordynamik — Duplikat | Konsolidierungsdokument: `AdaptiveFusionWeights` in `memfuse-db/src/replicator.rs` ist totes Duplikat | Bestätigt: Kein Aufrufer außerhalb der Datei selbst, P10-Verletzung, `pub mod replicator;` in `lib.rs` exportiert unbenutzte Struktur | **Löschen**: `crates/memfuse-db/src/replicator.rs` vollständig entfernen. `pub mod replicator;` aus `lib.rs` entfernen. Einzige F-07-Implementierung verbleibt in `memfuse-calibration`. P0-Fix. |
| **K14** | KvSegment Feldstruktur | v5.0 §7.1: `model_fingerprint`, `rope_offset`, `encrypted_layers: Vec<EncryptedKvLayer>` (AES-256-GCM-SIV) | `KvSegment` hat nur `tenant_id`, `segment_id`, `data: Vec<u8>` — **keine Verschlüsselung, kein ModelFingerprint, kein RoPE-Offset**. KV Bridge ist Zeroize-Skeleton, nicht Production-Ready. | **Increment-Planung**: Zeroize-Skeleton (✅ korrekt als Erstes) → Increment 2: AES-256-GCM-SIV via `KeyManager` + `ModelFingerprint` + `rope_offset`. v5.0-§7.1-Signatur wird bei Increment 2 normativ. Bis dahin: `memfuse-kv-bridge` ist Sicherheits-Skeleton ohne Krypto. |
| **K15** | F-09 β-Default | v5.0: β=0.15 default | `ResonanceConfig::default()`: `beta: 0.5` | **Code gewinnt**: β=0.5 ist der normative Default für F-09. Spec-Wert 0.15 war Designabsicht, nicht implementiert. Begründung: höheres β gibt mehr Gewicht der Kohärenz bei Multi-Signal-Treffern — empirisch besser für dichten Wissensgraphen. Wenn LongMemEval-CI einen Rückgang zeigt: auf 0.15 kalibrieren. |
| **K16** | KV-Cache-LRU-Korrektheit | v5.0: LRU-Eviction | `eviction_worker.rs`: `segs.remove(0)` — das ist **FIFO** (ältestes zuerst), kein echtes LRU (seltenst-genutztes zuerst). Keine Zugriffs-Zeitstempel werden gepflegt. | **FIFO als akzeptiertes Näherungsverfahren** für das erste Increment. Echter LRU (via Zeitstempel oder intrusive Liste) ist Pflicht vor Production-Release von Increment 2. ADR erforderlich. |
| **K17** | PhysioScheduler | v5.0: `PhysioConfig`-Struct in `memfuse-core` (§10.2) | **Kein `physio_scheduler.rs`** in `crates/memfuse-db/src/`. `start_thermostat_reaper` und `start_nrem_reaper` existieren noch in `reaper.rs` als separate, unkonsolidierte Tasks. | **Implementierungslücke P3**: PhysioScheduler ist nicht implementiert. Roadmap-Eintrag bleibt. `start_thermostat_reaper` und `start_nrem_reaper` bleiben operative Pfade bis zur Migration. |
| **K18** | F-03 Synaptische Verstärkung | v5.0: H2 (Horizont 2) | `synaptic.rs` existiert mit `SynapticConfig`, `apply_hebbian_update()`, `synaptic_score()` — **aber kein `SynapticUpdateBuffer`, kein 5. Fusionssignal** in `fusion.rs`. Feature-Flag `physio-synaptic-edges` existiert in `memfuse-graph/Cargo.toml`. | **Status**: Berechnungslogik vorhanden, Integration fehlt. F-03 verbleibt in H2. `SynapticUpdateBuffer` + Flush-Integration in PhysioScheduler (§10) ist nächster Schritt. |
| **K19** | GASP Post-Hoc-Validator | v5.0: H2 | Kein `gasp.rs` im Workspace gefunden. | **Status**: H3 — abhängig von `memfuse-candle`-Pipeline (P3). Keine Statusänderung. |
| **K20** | memfuse-tauri | v5.0: nicht erwähnt | `crates/memfuse-tauri/` existiert (6.156 LOC, PDF-Ingestion, Concurrency-Tests). Ist Workspace-Mitglied. | **Neue Crate**: `memfuse-tauri` ist Layer 4 (Desktop-GUI-Grenzschicht). DAG-Eintrag ausstehend. Keine Abhängigkeiten nach oben erlaubt. |

### §0.3 Permanente Architektur-Vetos (unveränderlich — aus `VETOES.md` v2)

**VETO-F02 — Partieller HNSW-Rebuild (conditionally_accepted):**
Status: `conditionally_accepted` bis `2026-10-07`. Reines Tombstone-Pruning (ohne Re-Wiring) ist NICHT vom Veto erfasst. Feature bleibt hinter `physio-nucleation` bis Recall@10-Regressionstest 30 Tage stabil. Scope-Präzisierung im neuen VETOES.md korrekt dokumentiert.

**VETO-F10 — Osmotischer Cross-Tenant-Wissensaustausch (permanent_rejected):**
Bricht TenantId-Isolation, KV-Bridge-Sicherheitsschicht und DeletionProof-Korrektheit. DSGVO Art. 17. Keine Alternative. Absolut.

### §0.4 Sofort-Prioritäten P0 (vor jeder weiteren Feature-Arbeit)

| Priorität | Maßnahme | Begründung | Aufwand |
|---|---|---|---|
| **P0-K13** | `crates/memfuse-db/src/replicator.rs` löschen | Totes Duplikat F-07, P10-Verletzung | 15 Min |
| **P0-K11** | `physio-resonance-fusion = []` in `memfuse-db/Cargo.toml` | F-09 ist sonst in allen Builds tot | 5 Min |
| **P0-K12** | `TenantId::new()` deprecieren, `From<u64>` Guard | INV-TENANT-1 semantisch nicht durchgesetzt | 1h |

---

## §1 Produktvision, Säulen & Architekturprinzipien

### §1.1 Kernaussage

**MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten und wissensintensive Einzelanwender — die Erinnerung nicht nur speichert, sondern konsolidiert, kalibriert, ihre eigene Löschung kryptographisch beweist, Widersprüche immunologisch abwehrt und sich nach physikalisch-biologischen Prinzipien selbst reguliert. Alles läuft auf Nutzerhardware, ohne Cloud-Zwang für den Kernbetrieb.**

### §1.2 Fünf Produktsäulen

**Säule I — Datenhoheit (Sovereign Core):** `memfuse-candle` existiert als technisches Fundament (GGUF-Loader, Inferenz, Embedding, Model-Registry). Pipeline-Integration ausstehend (P3). Status: Fundament bereit, Serving-Pipeline-Verdrahtung offen.

**Säule II — Belegbare Korrektheit:** Jedes Suchergebnis trägt eine nachvollziehbare Herkunftskette (`INV-PROV-1`). Cascade-Tombstone für Supersedes-Kanten ✅ implementiert. Temporal-Validity-Post-Filter ✅. PathRAG-Korrektheit durch Cascade-Tombstone abgesichert.

**Säule III — Hybride Retrieval-Qualität:** 3-Signal-RRF (Vektor + BM25 + Graph) + F-09-Kohärenz-Bonus (⛔ unaktivierbar bis K11-Fix) + PathRAG (Multi-Hop ✅) + PID-geregelter Reranker ✅.

**Säule IV — Gehärtete Kalibrierung & Cache-Sicherheit:** ConfigFingerprint-Zwang ✅. Lyapunov-Drift-Wächter (F-11) ✅. KV-Bridge: Zeroize-Skeleton ✅, Krypto-Increment ausstehend (K14).

**Säule V — Physio-Selbstmanagement:** F-01 ✅, F-03 (Berechnungslogik ✅, Integration fehlt), F-04 ✅, F-05 REM ✅, F-06 ✅ (feature-flagged), F-07 ✅, F-08 ✅, F-09 ✅ (code ✅, Cargo-Fix ausstehend), F-11 ✅. PhysioScheduler (Konsolidierungs-Orchestrierer) ausstehend.

### §1.3 Architekturprinzipien P1–P12

**P1 — DAG-Integrität:** `cargo xtask check-dag` ist CI-Gate. Kein Fachcode in Layer ≥N mit Wissen über Layer >N. `memfuse-tauri` (neu, Layer 4) benötigt DAG-Eintrag.

**P2 — Zero-Panic-Doctrine:** `unsafe` ausschließlich in `distance.rs` (SIMD, ADR-017), `diskann.rs` + `persistence.rs` (Mmap, ADR-017), `kv_bridge/segment.rs` (Zeroize-Test-SAFETY). Jedes `unsafe` trägt `// SAFETY: <Beweis>`. `#![forbid(unsafe_code)]` in allen anderen Crates erzwungen.

**P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. `fsync` nach jedem WAL-Eintrag vor MemTable-Update. DiskANN: WAL-first via `append_to_pending_wal()` vor In-Memory-Push (verifiziert in `05b382d8`).

**P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` (beide `memfuse-core`) sind die einzigen LLM-Abstraktionsgrenzen. Kein Fachcode in Layer 2–4 mit backend-spezifischem Wissen.

**P5 — Kein Cloud-Zwang:** Jede Komponente, deren einziger Betriebspfad externe Netzwerkabhängigkeit ist, benötigt ADR-dokumentierte Ausnahme. Ollama ist Standardpfad, kein Pflichtpfad.

**P6 — Eine Quelle für Architekturentscheidungen:** Ausschließlich `docs/decisions/ADR-*.md`. Kein Parallelismus ohne Rückverweis.

**P7 — Marketing-Aussagen sind an Code-Nachweise gebunden:** Jede quantitative Aussage (Latenz, Recall) benötigt reproduzierbare Messung in `memfuse-bench`. F-09 als "✅ Produktiv" zu bezeichnen ist ein P7-Verstoß bis K11-Fix.

**P8 — Kalibrierungs-Integrität:** Änderung an `prompt_template_hash`, `temperature_bits` oder `quantization` → sofortige Invalidierung aller Kalibrierungsstatistiken. `IsotonicCalibrator::invalidate_on_config_change()` implementiert und verdrahtet. Basis: arXiv:2608.01460.

**P9 — Kein Klartext-Sensitivspeicher:** Tensor-Zustände aus Nutzerdaten liegen nie unverschlüsselt auf persistentem oder auslagerbarem Speicher. Zeroize-on-Drop ✅ (via `ZeroizeOnDrop` in `KvSegment`). AES-256-GCM-SIV-Verschlüsselung: Increment 2 (K14).

**P10 — Reuse-vor-Neubau:** `AdaptiveFusionWeights` in `memfuse-db/src/replicator.rs` ist aktive P10-Verletzung. Löschen ist P0 (K13). Vor jedem neuen AP: expliziter Wiederverwendungs-Check.

**P11 — Latenzbudget-Pflicht für Hot-Path:** Jede Hot-Path-Operation benötigt explizites Latenzbudget mit hartem Deadline-Abbruchpfad. `RerankDeadline` + `RerankPidController` ✅.

**P12 — Physio-Feature-Default-Unsichtbarkeit:** Kein Physio-Feature erzeugt im Zero-IT-Setup-Default sichtbares Verhalten. Alle `physio-*`-Features sind Feature-Flag-deaktivierbar.

---

## §2 Crate-Topologie — Vollständige Ziel-Architektur (18 Crates)

```
Layer 0 — Fundament (kein I/O, keine externen Abhängigkeiten)
│
├── memfuse-core          Traits, Typen, Fehler-Hierarchie, DAG-Guard
│   ├── types/            TxId, DocId, TenantId [✅], CollectionId, EntityId
│   │                     ConfigFingerprint [✅], ModelFingerprint [✅]
│   │                     ⚠️ TenantId::new(0) bypass → K12 P1-Fix
│   ├── traits/           LlmTextGenerator, TextEmbeddingEngine [✅ AFIT]
│   │                     StorageEngine, VectorIndex, TextIndex
│   │                     GraphIndex, CheckpointCoordinator
│   │                     SegmentSynthesizer [✅ für REM-Phase]
│   ├── error.rs          Vollständige Fehler-Hierarchie (thiserror)
│   ├── tx_buffer.rs      MVCC-TxBuffer
│   └── seq_log.rs        Sequenz-Monotonie-Guard (ADR-016)
│
├── memfuse-crypto        Kryptographie-Primitive (keine Netz-I/O)
│   ├── crypto.rs         AES-256-GCM-SIV (RFC 8452), HKDF, KeyManager [✅]
│   ├── deletion_proof.rs DeletionProof [✅] + DeletionLayer-Enum
│   │                     ExcludedScope-Deklaration (maschinenlesbar)
│   ├── kv_cipher.rs      KvSegmentCipher — Increment 2 [K14]
│   └── hmac_chain.rs     WAL-HMAC-Chain-Verifizierung [✅ wal.rs:45-80]
│
└── memfuse-calibration   [✅] Unified Calibration Primitive
    ├── lib.rs            IsotonicCalibrator (PAVA), PlattScaler
    │                     invalidate_on_config_change() [✅ P8]
    ├── pid.rs            PidController (F-08 Anti-Windup) [✅]
    ├── platt.rs          Platt-Scaling
    ├── isotonic.rs       PAVA-Isotonische Regression
    └── replicator.rs     ReplicatorState — EINZIGE F-07-Impl [✅]
                          // KONSOLIDIERUNGS-HINWEIS: Nach K13-Fix

Layer 1 — Storage-Primitiven (I/O, kein LLM)
│
├── memfuse-store         LSM-Tree, WAL v3, SSTable, Mmap
│   ├── wal.rs            HMAC-Chain-WAL v3 (MFW3-Header), Atomic-Commit [✅]
│   │                     Legacy-V1/V2-Replay via legacy_integrity_key() [✅]
│   ├── memtable.rs       16-Shard BTreeMap, parking_lot::RwLock [✅ K1]
│   ├── sstable.rs        Bloom-Filter, CRC32-Verifikation
│   ├── compaction.rs     Background-Compaction, Tombstone-Tracking
│   ├── mmap.rs           Mmap-backed Reads, Sector-aligned
│   └── tenant_codec.rs   TenantKeyCodec [✅] — Prefix-Encoding
│
├── memfuse-index         Vektorindizes
│   ├── hnsw.rs           HNSW M=16 [✅], ef_construction=200 [✅ K2], ef_search=64
│   │                     2-Phasen-CoW-Rebuild [✅ hnsw.rs:1693+1812]
│   │                     rebuild_region() [physio-nucleation, conditionally_accepted]
│   ├── diskann.rs        DiskANN, trigger_background_persist_delta() [✅ #1722]
│   │                     WAL-first per append_to_pending_wal() [✅]
│   │                     spawn_blocking für Disk-I/O [✅ P11]
│   ├── distance.rs       SIMD AVX512/AVX2/NEON (unsafe, ADR-017)
│   │                     81 unsafe-Blöcke, 147 SAFETY-Kommentare [✅]
│   ├── quantize.rs       Q4/Q8 Scalar Quantization
│   └── persistence.rs    Binary-Format, Mmap-Load (unsafe, ADR-017)
│
├── memfuse-text          Volltext-Retrieval
│   ├── bm25.rs           BM25+ Robertson-Spärck-Jones [✅ Bug A19 gefixt]
│   │                     IDF = ln(1 + (N−df+0.5)/(df+0.5)), IDF ≥ 0 garantiert
│   ├── tokenizer.rs      DE-Morphologie (Kompositum-Dekomposition)
│   └── ngram.rs          N-Gramm-Generierung
│
├── memfuse-graph         Graph-Datenstrukturen
│   ├── csr.rs            CSR-Graph, tombstoned_edges (EdgeId-Bitmap) [✅]
│   │                     edges_for_doc() DocId→EdgeId-Index [✅]
│   │                     tombstone_edges_direct() idempotent [✅]
│   ├── cascade.rs        cascade_invalidate_edges_for_superseded_doc() [✅ #1726]
│   │                     INV-GRAPH-PROV-1, idempotent, PathRAG-getestet
│   ├── session_dag.rs    SessionBranchTree, NodesGuard [✅ session_dag.rs:29]
│   │                     Typ-erzwungene Lock-Reihenfolge (nodes→edges)
│   ├── ppr.rs            PPR damping=0.85 bidirektional [✅ ppr.rs:133,343]
│   ├── community.rs      Label-Propagation, SimpleRng LCG deterministisch [✅]
│   ├── path_rag.rs       PathRAGEngine [✅] + Sufficiency-Gate + DocId-Traversal-Guard
│   ├── immune.rs         ImmunMemory [✅] (F-04) Antikörper-Register
│   ├── provenance.rs     EdgeProvenance INV-GRAPH-PROV-1
│   ├── synaptic.rs       SynapticConfig, apply_hebbian_update(), synaptic_score() [✅]
│   │                     [F-03 Berechnungslogik ✅, Integration fehlt — H2]
│   └── percolation.rs    PercolationConfig, compute_percolation_health() [✅ feature-flagged]
│
├── memfuse-checkpoint    Checkpoint-Management (EINE Fassade)
│   └── lib.rs            CheckpointStore
│
└── memfuse-embed         Embedding & Klassifikation
    ├── embedder.rs       ONNX-Embedding (Arc<Mutex<Session>>)
    ├── reranker.rs       CrossEncoderReranker, ConfigFingerprint [✅]
    │                     PlattScaler via memfuse-calibration [✅]
    │                     RerankDeadline + RerankPidController [✅ #1699,#1702]
    └── importance_classifier.rs ImportanceClassifier [deferred — post-LongMemEval]

Layer 2 — Orchestrierung
│
└── memfuse-db            Geschäftslogik-Orchestrierung
    ├── fusion.rs         3-Signal-RRF + F-09 Kohärenz [⛔ K11-Fix] + ProvenanceRecord
    ├── temporal_filter.rs Temporal-Validity-Post-Filter [✅]
    ├── collection/       CollectionEngine, QueryBuilder, SearchEngine
    ├── context.rs        DualProcessMemory (Episodisch + Semantisch)
    ├── context_compaction.rs ContextCompactor (NREM-Phase)
    ├── sleep_cycle.rs    SleepCycleScheduler NREM [✅]
    ├── sleep_cycle_executor.rs NremExecutor [✅]
    ├── rem_phase.rs      REM-Phase: run_rem_phase(), SynthesizedChunk [✅ #1716]
    ├── homeostat.rs      PID-Regler (F-08) [✅ als RerankPidController]
    ├── thermostat.rs     Freie-Energie-Thermostat (F-01) [✅]
    ├── replicator.rs     [⛔ DEAD CODE — K13-Fix: löschen]
    ├── multistep.rs      MultiStepEngine
    ├── reaper.rs         TTL-/Orphan-/Thermostat-/NREM-Reaper [✅]
    ├── transaction.rs    MVCC-Transaktions-Management
    └── [physio_scheduler.rs] [FEHLT — K17, P3]

Layer 3 — Inferenz & Routing
│
├── memfuse-agent         Workflow-Engine [✅]
│   ├── engine.rs         AgentWorkflowEngine
│   ├── dlq.rs            Dead-Letter-Queue
│   ├── step.rs           AgentStep-Definitionen
│   └── audit.rs          Audit-Trail
│
├── memfuse-router        Conformal Router [✅]
│   ├── router.rs         RouterEngine, RoutingDecision, Abstention-Pfad [✅]
│   ├── profile.rs        SlmProfile + ConfigFingerprint [✅]
│   ├── dispatch.rs       Dispatch-Logik
│   └── lyapunov.rs       LyapunovDriftWatcher (F-11) [✅ H1→✅]
│
├── memfuse-ollama        Ollama-Backend (aktueller Standardpfad) [✅]
│   ├── client.rs         LlmTextGenerator-Impl, präventiver Halluzinations-Guard
│   └── importance.rs     [DEPRECATED im Hot-Path]
│
├── memfuse-candle        Pure-Rust-GGUF-Backend [✅ Crate, ⚠️ nicht in Pipeline]
│   ├── lib.rs            CandleLlmClient, CandleEmbedClient
│   ├── gguf_loader.rs    GGUF-Parser
│   ├── inference.rs      LlmTextGenerator-Impl
│   ├── embedding.rs      TextEmbeddingEngine-Impl
│   └── model_registry.rs ModelFingerprint (SHA-256 + Quantisierungsgrad)
│   [P3: Candle-Factory in memfuse-mcp/Cargo.toml + create_embedding_provider()]
│
└── memfuse-py            PyO3-Bindings [✅ Separater Workspace, ADR-064]
    └── lib.rs            panic="unwind" (FFI-Grenze), CI via --manifest-path

Layer 4 — Integrations-Grenzschicht
│
├── memfuse-mcp           MCP JSON-RPC 2.0 [✅]
│   └── lib.rs            E2E-Test #1613 grün, Prompt-Injection-Detection [✅]
│
├── memfuse-kv-bridge     KV-Cache-Bridge Sicherheitsskeleton [✅ Increment 1]
│   ├── lib.rs            Modul-Exporte
│   ├── segment.rs        KvSegment: ZeroizeOnDrop, tenant_id, segment_id, data
│   │                     ⚠️ Kein AES-GCM-SIV, kein ModelFingerprint, kein rope_offset
│   ├── store.rs          TenantIsolatedKvStore: AHashMap<TenantId, Vec<KvSegment>>
│   └── eviction_worker.rs EvictionWorker: dedizierter OS-Thread via mpsc [✅ P9/INV-KV-2]
│                          ⚠️ LRU ist FIFO-Näherung (K16) — Increment 2 Fix
│
└── memfuse-tauri         Desktop-GUI-Grenzschicht [✅ NEU in v6.0]
    ├── commands/         Tauri-Commands
    ├── ingestion/        PDF-Ingestion-Pipeline, ProgressTracker
    ├── state.rs          App-State
    └── ollama.rs         Ollama-Integration für Tauri

Layer 5 — Benchmarking
│
└── memfuse-bench         Evaluation & Regression
    ├── long_mem_eval.rs  LongMemEvalCase-Harness [✅ Harness, kein CI-Gate noch]
    ├── locomo.rs         LoCoMo-Harness [✅ Harness]
    └── main.rs           CLI-Runner

xtask — Build-Automatisierung
    ├── main.rs           check-dag, generate-adr
    ├── check_vetoes.rs   VETOES.md CI-Checker [✅ #1721]
    ├── check_duplicate_symbols.rs Fast-CI-Gate (ADR-064) [✅ #1721]
    ├── check_type_registry.rs
    ├── generate_adr.rs
    └── validate_pr_checklist.rs
```

---

## §3 Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie

### §3.1 Kern-Typen

**TxId** (`memfuse-core/src/types/domain.rs`):
- Monoton steigende logische Sequenznummer (ADR-016). Niemals `SystemTime` als Kausalitätsgarant.
- Konstanten: `TxId::MIN = 0`, `TxId::MAX = u64::MAX`.

**TenantId** — **Kritische Sicherheitslücke K12:**
```rust
pub struct TenantId(pub u64);

impl TenantId {
    pub const SYSTEM: Self = Self(0);   // Legitim — einziger Weg zu id=0
    pub const DEFAULT: Self = Self(0);  // ⚠️ DEPRECATED — semantisch identisch zu SYSTEM
    pub const INVALID: Self = Self(0);  // ⚠️ DEPRECATED — semantisch identisch zu SYSTEM

    /// WARNUNG: Const-fn, kein Guard. Akzeptiert id=0 ohne Fehler.
    /// NACH K12-FIX: #[deprecated] oder Panic bei id=0 in Debug.
    pub const fn new(id: u64) -> Self { Self(id) }

    /// Sicherer Konstruktor. Einziger normativ korrekter Pfad für Produktionscode.
    pub fn try_new(id: u64) -> Result<Self>; // Err bei id==0

    /// ⚠️ NACH K12-FIX: From<u64> muss try_new nutzen oder deprecated werden
    impl From<u64> for TenantId { fn from(id: u64) -> Self { Self(id) } }
}

// INV-TENANT-1 gilt erst als vollständig durchgesetzt nach K12-Fix
```

**Normative Auflösung K12:**
1. `TenantId::DEFAULT` und `TenantId::INVALID` mit `#[deprecated]` markieren — nur `TenantId::SYSTEM` für id=0.
2. `TenantId::new()` mit `#[deprecated(note = "Nutze try_new() für sicheren Konstruktor")]` markieren.
3. `impl From<u64> for TenantId` auf `try_new`-Basis migrieren oder mit Panic bei id=0 versehen.
4. Alle produktiven Aufrufer auf `try_new()` umstellen.

### §3.2 Traits (Layer 0, alle AFIT — 0 `async_trait` im Workspace verifiziert)

```rust
// AFIT: Async Functions In Traits — Rust 1.75+, rust-version = "1.89" ✅

pub trait StorageEngine: Send + Sync {
    async fn get(&self, tenant_id: TenantId, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tenant_id: TenantId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, tenant_id: TenantId, key: &[u8]) -> Result<()>;
    async fn scan_prefix(&self, tenant_id: TenantId, prefix: &[u8])
        -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

pub trait VectorIndex: Send + Sync {
    async fn insert(&self, tx_id: TxId, id: DocId, embedding: &[f32]) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(DocId, f32)>>;
    async fn delete(&self, tx_id: TxId, id: DocId) -> Result<()>;
    async fn commit(&self, tx_id: TxId) -> Result<()>;
}

pub trait GraphIndex: Send + Sync {
    async fn add_entity(&self, tx_id: TxId, entity: Entity) -> Result<()>;
    async fn add_edge(&self, tx_id: TxId, edge: Edge) -> Result<()>;
    async fn neighbors(&self, node: EntityId) -> Result<Vec<EntityId>>;
    async fn commit(&self, tx_id: TxId) -> Result<()>;
}

pub trait SegmentSynthesizer: Send + Sync {
    // Für REM-Phase (rem_phase.rs)
    async fn synthesize(&self, turns: &[String]) -> Result<String>;
}
```

### §3.3 Kalibrierung (`memfuse-calibration`) — ✅ Produktiv

**IsotonicCalibrator (PAVA):** Kalibriert Roh-Scores zu kalibrierten Wahrscheinlichkeiten. Pool Adjacent Violators Algorithm. ECE-Messung nach jeder Rekalibrierung.

**ConfigFingerprint:** Hash über `(prompt_template_hash, temperature_bits, quantization_level)`. Jede Änderung invalidiert alle Kalibrierungsstatistiken. Verdrahtet in: Router ✅, Reranker ✅, Calibration ✅.

```rust
pub struct IsotonicCalibrator {
    pub calibrated: bool,
    pub config_fingerprint: ConfigFingerprint,
    // ...
}

impl IsotonicCalibrator {
    /// P8: Jede Config-Änderung invalidiert sofort.
    pub fn invalidate_on_config_change(&mut self, new_fp: ConfigFingerprint);

    /// Abstention-Pfad: Bei calibrated==false → Err statt 0.5-Fallback (kein stiller Default)
    pub fn calibrate(&self, raw_score: f32) -> Result<f32>;
}
```

**ReplicatorState (F-07):** Multiplicative-Weights-Update (Arora et al. 2012) mit Regret-Bound-Garantie. `#[cfg(feature = "physio-replicator-weights")]`. **EINZIGE** F-07-Implementierung nach K13-Fix.

```rust
/// Update-Regel: w_s(t+1) = w_s(t) * (1 + η * (f_s(t) - f̄(t)))
/// gefolgt von Normalisierung (Σ w_s = 1) und Clamping auf [w_min=0.05, w_max=0.90].
/// Konvergenzgarantie: Nash-Gleichgewicht unter stationären Fitness-Erwartungswerten.
pub struct ReplicatorState { ... }
```

### §3.4 Kryptographie (`memfuse-crypto`)

**AES-256-GCM-SIV (RFC 8452):** Nonce-Misuse-Resistant. `KeyManager` ist die einzige Krypto-Quelle im Workspace (P10). Wiederverwendet von KV-Bridge (Increment 2, K14).

**DeletionProof:** Kryptographischer Löschbeweis für DSGVO Art. 17. Kritische Deckungsgrenze (maschinenlesbar via `ExcludedScope`):

```rust
pub enum ExcludedScope {
    ConsolidatedAndDistilled,  // Fine-Tuning-Input
    LlmParameterMemory,         // Arora: arXiv:2505.16831 — Unlearning ≠ Deletion
}

/// INV-DELETION-1: create() NUR nach vollständiger Layer-Bereinigung.
/// Jede Verletzung macht den Beweis mathematisch wertlos.
pub struct DeletionProof {
    pub covered_layers: Vec<DeletionLayer>,
    pub excluded_scopes: Vec<ExcludedScope>,
    pub scope: DeletionScope,
    pub hmac_signature: [u8; 32],  // Über (scope || sorted_layer_hashes || tx_id)
    // ...
}
```

**WAL HMAC-Chain:** V3 (MFW3-Header) mit `HMAC(seq || op || timestamp || prev_hash)`. V1/V2-Replay via Legacy-Key für Abwärtskompatibilität. `verify_chain()` vor jedem Recovery.

---

## §4 Layer 1 — Storage-Primitiven

### §4.1 WAL v3 — ✅ Produktionsreif

```
Schreibpfad: WalEntry::try_new() → compute_checksum_v3() → File::write() → fsync → MemTable::put()
Recovery:    replay() → HMAC-Chain verifizieren → MemTable rekonstruieren
DiskANN:     append_to_pending_wal() VOR pending_inserts.push() [P3 WAL-First]
```

**Crash-Recovery-Garantie DiskANN:** `recover_pending_delta()` stellt WAL-persistierte, nicht in `persist_delta()` übernommene Vektoren bei Neustart wieder her. Keine Datenverlust-Fenster mehr (historischer Kommentar in `diskann.rs` aktualisiert in Prompt 2 des Konsolidierungsdokuments).

### §4.2 LSM-Tree — ✅ Produktiv (K1-Auflösung bestätigt)

**MemTable-Design:** 16-Shard `BTreeMap<Bytes, Vec<MemTableEntry>>` mit `parking_lot::RwLock`. Lexikographische Sortiergarantie für SSTable-Flush (INVARIANT-3). Shard-Selektion via AHash des vollen Keys.

```rust
const SHARD_COUNT: usize = 16;
type MemTableMap = BTreeMap<Bytes, Vec<MemTableEntry>>;
// INVARIANT-3: Lexikographische Ordnung innerhalb eines Shards = SSTable-Flush-Korrektheit
```

Migration zu `crossbeam-skiplist` nur nach Perf-Nachweis mit echter Last.

### §4.3 HNSW — ✅ Produktionsreif (K2-Auflösung bestätigt)

**Parameter:** `M=16`, `ef_construction=200`, `ef_search=64` (Recall@10 ≈ 0.98).

**Invariante INV-HNSW-1:** `ef_construction >= M`. `HnswConfig::validate()` → Err bei Verletzung. Implementiert in `hnsw.rs:142-147`.

**Tombstone-Management:** `HNSW_REBUILD_DELETION_RATIO = 0.10` (10%). Rebuild-Trigger wenn aktive Vektoren < 90%. Input für F-01-Thermostat. VETO-F02: `rebuild_region()` hinter `physio-nucleation`, conditionally_accepted bis 2026-10-07.

**2-Phasen-CoW-Rebuild** (Produktionspfad): `hnsw.rs:1693+1812`. Read-Phase auf altem Graph, dann atomarer Wechsel.

### §4.4 DiskANN — ✅ Produktiv, hot-path-entkoppelt (#1722)

**Rollentrennung:**
- HNSW: Primärindex für Sammlungen ≤ 100.000 Vektoren
- DiskANN: Out-of-Core für > 500.000 Vektoren (`experimental-diskann` Feature-Flag)
- Transition-Trigger: Auto-Build bei HNSW > 200.000 Vektoren
- `PENDING_FLUSH_THRESHOLD = 50` (reine Performance-Batching-Entscheidung, kein Sicherheits-Kompromiss)

```rust
pub fn trigger_background_persist_delta(&self) {
    // Startet tokio::task::spawn_blocking() — nie blockierend im Async-Hot-Path (P11)
}
pub async fn persist_delta(&self) -> Result<()> {
    // tokio::task::spawn_blocking für Disk-I/O
}
```

### §4.5 CSR-Graph mit Kanten-Provenienz & Cascade-Tombstone — ✅ Vollständig

```rust
// cascade.rs — INV-GRAPH-PROV-1
pub async fn cascade_invalidate_edges_for_superseded_doc(
    graph: &CsrGraph,
    superseded_doc_id: DocId,
    wal_seq: u64,
) -> Result<CascadeInvalidationReport> {
    // 1. DocId → Set<EdgeId> via edges_for_doc()
    // 2. tombstone_edges_direct() — idempotent
    // 3. delete_edge_persistence() falls Storage vorhanden
    // Report: tombstoned_edge_count, affected_node_ids
}
// ✅ PathRAG-Integration-Test: Pfade über tombstonierte Kanten werden korrekt ausgeblendet
// ✅ Idempotenz-Test: Zweiter Aufruf meldet tombstoned_edge_count == 0
```

**INV-GRAPH-PROV-1:** Jede CSR-Kante muss einen `EdgeProvenance`-Eintrag mit WAL-Seq haben. Bi-temporale Kanten (ADR-033), Supersedes (ADR-038).

### §4.6 BM25+ IDF-Korrektur — ✅ Bestätigt

```rust
// Robertson-Spärck-Jones BM25+: mathematische Garantie IDF ≥ 0 für alle df ∈ [0, N]
// bm25.rs:91-96
let idf = {
    let arg = 1.0 + (n - df + 0.5) / (df + 0.5);
    arg.ln()  // IDF ≥ ln(1) = 0 — keine Floor nötig
};
// Property-Test: prop_bm25_idf_non_negative_for_high_df ✅
```

---

## §5 Layer 2 — Orchestrierung & Fusion

### §5.1 Retrieval-Pipeline (vollständige Sequenz)

```
Anfrage
  │
  ▼
[Query-Klassifikation] ─── QueryHopClass: SingleHop | MultiHop
  │
  ▼
[Parallele Index-Abfragen]
  ├── HNSW Vektor-Suche
  ├── BM25+ Volltext-Suche (mit DE-Morphologie)
  └── PPR Graph-Traversal (damping=0.85)
  │
  ▼
[3-Signal-RRF-Fusion] ─── k=60, w_v/w_t/w_g (default: adaptiv via F-07)
  │                        ProvenanceRecord: INV-PROV-1 (sum ≈ rrf_score, |Δ| < 1e-6)
  │
  ▼
[F-09 Resonanz-Kohärenz-Bonus] ⛔ K11-Fix ausstehend
  │  coherence_bonus IMMER separates Feld (INV-PROV-2)
  │  β=0.5 (normativ nach K15-Auflösung)
  │
  ▼
[Temporal-Validity-Post-Filter] ✅ ── nur valid_from ≤ NOW < valid_until
  │
  ▼
[PathRAG Signal] (nur bei MultiHop-Intent) ✅
  │  Sufficiency-Gate: Konfidenz > 0.01 default (max_hops=4)
  │  Tombstone-aware via cascade.rs ✅
  │
  ▼
[RerankPidController] ✅ ── dynamischer Kandidatenpool, RerankDeadline
  │  k_min = 10 default (PidController::default().min_pool_size)
  │  target_latency_ms = 200ms default
  │  Anti-Windup-Integral-Clip ✅
  │
  ▼
Ergebnis mit ProvenanceRecord (INV-PROV-1 + INV-PROV-2)
```

### §5.2 RRF-Fusion mit F-09 Kohärenz-Bonus

```rust
// INV-PROV-1: sum(contributions.rrf_contribution) ≈ rrf_score (|Δ| < 1e-6)
// INV-PROV-2: coherence_bonus ist IMMER separates Feld, NIE in rrf_score gefaltet

pub struct ProvenanceRecord {
    pub contributions: Vec<SignalContribution>,
    pub rrf_score: f32,              // Reine RRF-Summe OHNE Kohärenz-Bonus
    pub coherence_bonus: f32,        // F-09 — separates Feld (INV-PROV-2)
    pub final_score: f32,            // rrf_score + coherence_bonus
    pub rerank_score: Option<f32>,
    pub synaptic_score: Option<f32>, // F-03 (H2 — Integration pending)
}

/// F-09 Kohärenz-Formel (nach K11-Fix aktiv):
/// coherence(d) = (signal_count / total_signals).powf(β), β=0.5
/// coherence_bonus(d) = γ * coherence(d) * rrf_score(d), γ=0.3
/// Feature-Flag: physio-resonance-fusion (nach K11-Fix in Cargo.toml deklariert)
#[cfg(feature = "physio-resonance-fusion")]
pub fn apply_resonance_bonus(results: Vec<SearchResult>, ...) -> Vec<SearchResult>;
```

**Kritische Aktion K11:**
```toml
# crates/memfuse-db/Cargo.toml [features] — FEHLT NOCH:
physio-resonance-fusion = []
```

### §5.3 PID-Reranker (F-08) — ✅ Produktiv

```rust
pub struct PidController {
    pub kp: f32,             // Default: 2.0
    pub ki: f32,             // Default: 0.1
    pub kd: f32,             // Default: 0.5
    pub target_latency_ms: f32,  // Default: 200ms
    pub min_pool_size: usize,    // Default: 10
    pub max_pool_size: usize,    // Default: 500
    integral: f32,           // Anti-Windup auf [-max_integral, +max_integral] geclipped
    prev_error: f32,
    max_integral: f32,       // Default: 100.0
}
// Update-Regel: error = target - measured; PID-Ausgabe → neue Pool-Größe
// Clamped auf [min_pool_size, max_pool_size]
```

**Wissenschaftliche Basis:** Kandidatenpool-Minimum empirisch validiert. P99-Latenz-Ziel: 500ms default (RerankDeadline).

### §5.4 PathRAG Engine — ✅ Produktiv, cascade-sicher

**Basis:** arXiv:2502.14902 (PathRAG AAAI 2026). Sufficiency-Gate verhindert MemGraphRAG-Precision-Kollaps (arXiv:2506.00610).

**Algorithmus:** Bidirektionaler Dijkstra, `max_hops=4`, `sufficiency_threshold=0.01`. Pfade über tombstonierte Kanten werden durch `CsrGraph::tombstoned_edges` ausgeblendet ✅.

**Aktivierung:** Nur bei `QueryHopClass::MultiHop`. PathRAG-Ergebnis fließt als additives 4. Signal in Fusion. Korrektheitsproblem aus v5.0 (cascade-Tombstone) ist vollständig gelöst (#1726).

### §5.5 SleepCycle — NREM ✅, REM ✅

```
NREM (sleep_cycle.rs, ContextCompactor):
  - Sliding-Window-Clustering zeitlich benachbarter Turns (segment_cohesion_threshold=0.70)
  - Near-Duplicate-Detection (near_duplicate_cosine_threshold=0.95) — O(n²) segmentlokal
  - Identifikation verwaister Graph-Kanten → cascade_invalidate_edges_for_superseded_doc()
  - min_turns_per_segment=3, max_turns_per_segment=20

REM (rem_phase.rs — ✅ #1716):
  - Generative Wissenssynthese via SegmentSynthesizer-Trait
  - Ein SynthesizedChunk pro TurnSegment, source_turn_ids für Provenienz
  - min_turns_for_rem=3 (kurze Segmente überspringen)
  - Einzelne Segment-Fehler isoliert (nie Gesamtfehler-Propagation)
  - LycheeMemory V2 (arXiv:2608.12990): Turn-Clustering-Zielwerte
  - Budget-Check via max_llm_calls_per_cycle (P12-Kostenschutz)
```

**Wissenschaftliche Basis:** arXiv:2608.12990 (LycheeMemory V2), arXiv:2603.14517 (SleepGate), arXiv:2605.17625 (Dual-Process).

### §5.6 Freie-Energie-Thermostat (F-01) — ✅ Produktiv

```rust
// T(t) = w1 * tombstone_ratio + w2 * query_rate_inverse ∈ [0,1]
//   w1=0.6 (Speicherdruck), w2=0.4 (Query-Rate) — konfigurierbar in ThermostatConfig
// half_life_eff = half_life_base * (1 + κ * (1 − T(t))),  κ=2.0
// effective_score = base_score * exp(−(ln2 / half_life_eff) * elapsed)
// Semantik: Hohe T → aggressiveres Vergessen. Niedrige T → längeres Behalten.
// Inputs: tombstone_ratio (HNSW-Metrik), query_rate (TxId-Zähldifferenzen, ADR-016)
```

---

## §6 Layer 3 — Inferenz, Routing & Physio-Selbstregulierung

### §6.1 Conformal Router mit ConfigFingerprint & Abstention — ✅ Produktiv

**Abstention-Pfad:** Bei `calibrator.calibrated == false` → Eskalation an stärkstes verfügbares Modell. Kein stiller 0.5-Fallback. Basis: RACER (arXiv:2603.06616).

**Temperatur-Lock:** Temperaturänderung während Warmup → `invalidate_on_config_change()` + Zähler-Reset. P8.

### §6.2 Lyapunov-Drift-Wächter (F-11) — ✅ Implementiert (H1 → ✅)

```rust
// lyapunov.rs — LyapunovDriftWatcher
// D_t = KL(N_t || N_baseline) via 10-Bin-Histogramm der Non-Conformity-Scores
// λ_t = diskrete Lyapunov-Schätzung über gleitendes Fenster (window_size=20)
// Trigger: λ_t > 0.0 → LyapunovResult::DriftDetected → proaktive Re-Kalibrierung

pub enum LyapunovResult {
    Stable { lyapunov_exponent: f32 },
    DriftDetected { lyapunov_exponent: f32, reason: DriftReason },
    InsufficientData,   // < window_size Beobachtungen
}

pub struct LyapunovDriftWatcher {
    pub window_size: usize,        // Default: 20
    pub divergence_history: VecDeque<f32>,
    pub baseline_distribution: Vec<f32>,
    pub latest_result: Option<LyapunovResult>,
}
```

**Additive Orthogonalität:** Lyapunov-Wächter (proaktiv, distributionell) ist vollständig orthogonal zu ConfigFingerprint-Mechanism (reaktiv, konfigurationsbasiert). Beide sind aktiv.

### §6.3 memfuse-candle — Fundament bereit, Pipeline ausstehend (P3)

**Fehlende Factory-Funktion** (Prompt 8 des Konsolidierungsdokuments):
```rust
// In memfuse-mcp/src/config.rs — NOCH NICHT IMPLEMENTIERT
pub fn create_embedding_provider(provider_type: &str, ...) -> Result<Arc<dyn TextEmbeddingEngine>>;
pub fn create_llm_text_generator(provider_type: &str, ...) -> Result<Arc<dyn LlmTextGenerator>>;

// Feature: [features] in memfuse-mcp/Cargo.toml
// candle = ["dep:memfuse-candle"]   — NOCH NICHT DEKLARIERT
```

**Strategisches Risiko:** Ohne Pipeline-Integration ist "Sovereign Core" (Säule I) ein P7-Verstoß. GASP Post-Hoc-Validator (§7.2) erfordert `log_likelihood()` — nur via `memfuse-candle`. GASP bleibt H3 bis P3 abgeschlossen.

### §6.4 ImportanceClassifier — Bewusst zurückgestellt

Freigabe-Kriterium: LongMemEval-CI-Integration (§8). Ziel-Latenz: < 100ms P50 (MemRouter-Referenz: 58ms, arXiv:2605.00356).

---

## §7 Layer 4 — Integrations-Grenzschicht

### §7.1 KV-Cache-Bridge — Increment 1 ✅, Increment 2 ausstehend

**Increment 1 (implementiert, `ade2f12f`):**
- `KvSegment`: `tenant_id`, `segment_id`, `data: Vec<u8>` mit `ZeroizeOnDrop`
- `TenantIsolatedKvStore`: `AHashMap<TenantId, Vec<KvSegment>>` — strukturelle Tenant-Isolation
- `EvictionWorker`: Dedizierter OS-Thread via `std::sync::mpsc` (P9/INV-KV-2) ✅
- LRU-Näherung: FIFO via `Vec.remove(0)` (K16) — für Increment 2 durch echtes LRU ersetzen

**Increment 2 (normative Ziel-API aus v5.0 §7.1):**
```rust
pub struct KvSegment {
    pub tenant_id: TenantId,
    pub model_fingerprint: ModelFingerprint,  // Q4 ≠ Q8 (P8)
    pub encrypted_layers: Vec<EncryptedKvLayer>, // AES-256-GCM-SIV via KeyManager
    pub created_at_tx: TxId,
    pub vram_bytes: usize,
    pub rope_offset: u32,  // RoPE-Positions-Offset, bei inject() korrigiert
    #[zeroize(skip)] pub segment_id: u64,
}

// Schlüsselableitung: KeyManager::derive_kv_segment_key(tenant_id, doc_id) — P10
// LRU: Zeitstempel-basiert oder intrusive VecDeque — K16-Fix
// Sicherheitsbasis: arXiv:2510.17098 (MTI-Angriff) + arXiv:2508.09442 (Inversion)
```

**INV-KV-2-Ergänzung für echten LRU:** `evict_lru_nonblocking()` über den bestehenden EvictionWorker-Kanal. Zeroize bleibt auf OS-Thread — nie im Tokio-Executor blockierend.

### §7.2 GASP Post-Hoc-Halluzinations-Validator — H3

**Abhängigkeit:** `LlmTextGenerator::log_likelihood()` — nur via `memfuse-candle`. Basis: arXiv:2607.04223. Bis P3 abgeschlossen: kein GASP.

### §7.3 MCP JSON-RPC 2.0 — ✅ Produktionsreif

E2E-Test #1613 grün. Prompt-Injection-Detection aktiv.

### §7.4 memfuse-tauri — ✅ NEU, Layer 4 (v6.0)

PDF-Ingestion-Pipeline mit Progress-Tracking. 6.156 LOC. DAG-Eintrag in `xtask` ausstehend (keine Abhängigkeiten nach unten außer `memfuse-db`/`memfuse-mcp`).

---

## §8 Layer 5 — Evaluation & Benchmarking

### §8.1 LongMemEval & LoCoMo — Harness ✅, CI-Gate ausstehend (P2)

**LongMemEval-Harness** (`benchmarks/memfuse-bench/src/long_mem_eval.rs` ✅):
- Vollständiger Case-Loader, 7 Fragetypen (SingleSessionUser, MultiSession, KnowledgeUpdate, TemporalReasoning, Abstention, ...)
- Lose Kopplung über Search-Closure-API
- `LongMemEvalReport`: per_category_accuracy + overall_accuracy

**LoCoMo-Harness** (`benchmarks/memfuse-bench/src/locomo.rs` ✅):
- 5 Kategorieen (MultiHop, Temporal, OpenDomain, SingleHop, Adversarial)

**Offene Lücke P2:** Kein `.github/workflows/`-Job führt LongMemEval als CI-Gate aus. Bei 76 Commits/Tag: stille Qualitätsregressionen ohne CI-Baseline unerkennbar.

```yaml
# .github/workflows/bench.yml — ERGÄNZEN:
- name: LongMemEval Regression Gate
  run: cargo run -p memfuse-bench -- --eval longmemeval --dataset $LME_DATASET
  env:
    RECALL_REGRESSION_THRESHOLD: "0.01"  # Max 1pp Rückgang
```

**Zielwerte** (LycheeMemory V2 arXiv:2608.12990 als SOTA-Referenz):
- LongMemEval-S Overall Accuracy: > 85% (nach H3-Features aktiv)
- LoCoMo MultiHop: > 70%

---

## §9 Physio-Feature-Katalog (F-01 bis F-11)

| Feature | Name | Status | Implementierungsort | Crate-Flag |
|---|---|---|---|---|
| **F-01** | Freie-Energie-Thermostat | ✅ Produktiv | `memfuse-db/src/thermostat.rs` | Default |
| **F-02** | Partieller HNSW-Rebuild | 🟡 VETO-conditionally_accepted | `hnsw.rs` (rebuild_region) | `physio-nucleation` |
| **F-03** | Synaptische Verstärkung | 🔶 Berechnungslogik ✅, Integration H2 | `memfuse-graph/src/synaptic.rs` | `physio-synaptic-edges` |
| **F-04** | Immunologische Widerspruchsprävention | ✅ Produktiv | `memfuse-graph/src/immune.rs` | Default |
| **F-05** | REM-Synthese | ✅ Produktiv (#1716) | `memfuse-db/src/rem_phase.rs` | LLM-Abhängigkeit |
| **F-06** | Perkolations-Gesundheitsmonitor | ✅ Feature-flagged | `memfuse-graph/src/percolation.rs` | `physio-percolation` |
| **F-07** | Replikatordynamik | ✅ Produktiv (nach K13-Fix einzig) | `memfuse-calibration/src/replicator.rs` | `physio-replicator-weights` |
| **F-08** | PID-Latenz-Homöostat | ✅ Produktiv | `memfuse-calibration/src/pid.rs` | Default |
| **F-09** | Resonanz-Kohärenz-Bonus | ⛔ Code ✅, Cargo-Deklaration fehlt (K11) | `memfuse-db/src/fusion.rs` | `physio-resonance-fusion` ← FEHLT |
| **F-10** | Osmotischer Cross-Tenant-Austausch | 🔴 VETO permanent | — | — |
| **F-11** | Lyapunov-Drift-Wächter | ✅ Produktiv | `memfuse-router/src/lyapunov.rs` | Default |

---

## §10 PhysioScheduler & PhysioConfig

### §10.1 PhysioScheduler — AUSSTEHEND (K17, P3)

Ziel-Design (Prompt 4 des Konsolidierungsdokuments, noch nicht implementiert):

```rust
// crates/memfuse-db/src/physio_scheduler.rs [FEHLT]
pub struct PhysioScheduler<S: StorageEngine, V: VectorIndex> {
    config: PhysioConfig,
    thermostat: Arc<RwLock<FreeEnergyThermostat>>,
    replicator: Arc<parking_lot::RwLock<memfuse_calibration::ReplicatorState>>,
    // ... Handles auf bestehende Zustände
}

impl<S, V> PhysioScheduler<S, V> {
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_secs(self.config.tick_interval_secs)
            );
            loop {
                interval.tick().await;
                // SEQUENZIELL — vermeidet Lock-Contention zwischen Subsystemen:
                // a) WAL PhysioTickIntent schreiben
                // b) Thermostat-Update (wenn thermostat_enabled)
                // c) Perkolation (wenn nicht im Active-Session-Fenster)
                // d) Replikator-Update (wenn replicator_enabled)
                // e) F-03 Platzhalter-Hook [#[cfg(feature = "physio-synaptic-edges")]]
                // f) NREM-Trigger (wenn sleep_cycle_enabled)
                // g) WAL Completion-Marker
                // FEHLERBEHANDLUNG: Jeder Teilschritt isoliert, kein Abbruch der anderen
            }
        })
    }
}
```

**Migrations-Strategie:** `start_thermostat_reaper()` und `start_nrem_reaper()` in `reaper.rs` werden mit `#[deprecated]` markiert (nicht gelöscht). `start_expiry_reaper()` und `start_orphan_reaper()` bleiben unverändert (reine Datenhygiene).

### §10.2 PhysioConfig — Ziel-Struct

```rust
// Ziel: crates/memfuse-core/src/physio.rs [noch nicht vorhanden, oder in memfuse-db/src/physio_config.rs]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhysioConfig {
    pub tick_interval_secs: u64,          // Default: 60
    // F-01 Thermostat — Komposition statt Duplizierung (P10)
    pub thermostat_enabled: bool,         // Default: true
    pub thermostat: ThermostatConfig,     // #[serde(flatten)] oder Komposition
    // F-07 Replikatordynamik
    pub replicator_enabled: bool,         // Default: true
    // F-09 Kohärenz-Bonus
    pub coherence_bonus_beta: f32,        // Default: 0.5 (nach K15-Auflösung)
    // F-06 Perkolation — Komposition (nicht duplizieren)
    pub percolation: PercolationConfig,
    // SleepCycle
    pub sleep_cycle_enabled: bool,        // Default: false (erfordert LLM)
    pub sleep_episode_threshold: usize,   // Default: 50
}
```

---

## §11 Invarianten-Verzeichnis (normativ)

| Invariante | Aussage | Durchsetzung |
|---|---|---|
| **INV-WAL-1** | `fsync` nach jedem WAL-Eintrag vor MemTable-Update. `let _ = dir.sync_all()` ist P3-VIO. | WAL-Tests |
| **INV-WAL-2** | HMAC-Chain-Verifikation beim Replay vor jedem Recovery. | `verify_chain()` in Tests |
| **INVARIANT-3** | MemTable-Shard enthält Keys in lexikographischer Ordnung (BTreeMap-Garantie). | `sstable.rs` Tests |
| **INV-PROV-1** | `sum(contributions.rrf_contribution) ≈ rrf_score` (|Δ| < 1e-6). | `fusion.rs` debug_assert! |
| **INV-PROV-2** | `coherence_bonus` ist IMMER separates Feld. NIE in `rrf_score` gefaltet. | `fusion.rs` Tests |
| **INV-TENANT-1** | `TenantId(0)` ist SYSTEM-reserviert. `try_new(0)` → Err. Nach K12-Fix: `new(0)` und `From::from(0)` ebenfalls abgesichert. | `domain.rs` Tests + K12-Fix |
| **INV-TENANT-2** | `scan_prefix()` gibt ausschließlich Keys des zugehörigen Tenants zurück. | `tenant_codec.rs` Tests |
| **INV-HNSW-1** | `ef_construction >= M`. `HnswConfig::validate()` → Err bei Verletzung. | `hnsw.rs:142-147` |
| **INV-GRAPH-PROV-1** | Jede CSR-Kante hat gültigen `EdgeProvenance`-Eintrag mit WAL-Seq. | Property-Test `memfuse-bench` |
| **INV-DISKANN-1** | `persist_delta()` behält atomares Rename-Muster (Tmp→fsync→Rename→Parent-fsync). | `diskann.rs` Tests |
| **INV-DELETION-1** | `DeletionProof::create()` NUR nach vollständiger Layer-Bereinigung aller `covered_layers`. | Integration-Test |
| **INV-KV-1** | KV-Cache-Segmente liegen nie im Klartext auf persistentem Speicher. Zeroize-on-Drop via `ZeroizeOnDrop`. | Security-Test |
| **INV-KV-2** | Zeroize-on-Evict läuft auf Worker-Thread, nie blockierend im Inferenz-Hot-Path. | Latenz-Test |
| **INV-F02-1** | `physio-nucleation` ist in CI deaktiviert bis Recall@10-Regressionstest 30 Tage stabil (VETO-F02 conditionally_accepted bis 2026-10-07). | CI-Config |
| **INV-CASCADE-1** | Bei jedem Supersedes-Event wird `cascade_invalidate_edges_for_superseded_doc()` ausgelöst. Idempotent. | PathRAG-Integration-Test |
| **INV-F09-1** | `physio-resonance-fusion` ist als Feature in `crates/memfuse-db/Cargo.toml` deklariert (K11-Fix). | Compile-Test |
| **INV-F07-1** | `AdaptiveFusionWeights` aus `crates/memfuse-db/src/replicator.rs` ist nicht im Workspace existent (K13-Fix). | `cargo build --workspace` |

---

## §12 Implementierungsstand & Priorisierte Roadmap

### §12.1 Vollständiger Implementierungsstand (HEAD `05b382d8`)

| Komponente | Status | Seit |
|---|---|---|
| TenantId + TenantKeyCodec | ✅ Produktiv (K12 → P1-Fix nötig) | v4.0 |
| ConfigFingerprint | ✅ Router + Calibration + Reranker | v5.0 |
| DeletionProof | ✅ Produktiv | v5.0 |
| memfuse-calibration | ✅ Isotonic + Platt + PID + Replicator | v5.0 |
| PathRAGEngine | ✅ Cascade-tombstone-sicher | v5.0 + #1726 |
| Cascade-Tombstone (INV-CASCADE-1) | ✅ cascade.rs (#1726) | v6.0 neu |
| KV-Bridge Increment 1 (Zeroize-Skeleton) | ✅ Segment + Store + Worker | v6.0 neu |
| Lyapunov-Drift-Wächter (F-11) | ✅ lyapunov.rs in Router | v6.0 neu |
| REM-Phase (F-05) | ✅ rem_phase.rs (#1716) | v6.0 neu |
| LongMemEval + LoCoMo Harness | ✅ Harness existent | v6.0 neu |
| memfuse-tauri | ✅ Desktop-GUI-Crate | v6.0 neu |
| VETOES.md + CI-Check | ✅ #1721 | v6.0 neu |
| DiskANN hot-path-Entkopplung | ✅ spawn_blocking (#1722) | v6.0 neu |
| BM25 IDF Robertson-Spärck-Jones | ✅ | v5.0 |
| WAL v3 HMAC-Chain | ✅ | v5.0 |
| HNSW M=16, ef_construction=200 | ✅ | v5.0 |
| NodesGuard Lock-Reihenfolge | ✅ | v5.0 |
| PPR damping=0.85 bidirektional | ✅ | v5.0 |
| MCP JSON-RPC 2.0 E2E-Test | ✅ | v5.0 |
| F-01 Thermostat | ✅ | v5.0 |
| F-04 ImmunMemory | ✅ | v5.0 |
| F-07 Replikatordynamik (in calibration) | ✅ (nach K13-Fix einzig) | v5.0 |
| F-08 PID-Homöostat | ✅ | v5.0 |
| F-09 Kohärenz-Bonus (Code) | ⛔ K11-Fix → Cargo-Deklaration | v5.0 |
| F-06 Percolation | ✅ feature-flagged | v6.0 |
| F-03 Synaptic (Berechnungslogik) | 🔶 Teilweise | v6.0 |
| memfuse-candle Crate | ✅ Crate, ⚠️ nicht in Pipeline | v5.0 |
| GASP Post-Hoc-Validator | ❌ nicht implementiert | H3 |
| PhysioScheduler | ❌ nicht implementiert | H3 |
| KV-Bridge Increment 2 (Krypto) | ❌ ausstehend | P2 |
| LongMemEval als CI-Gate | ❌ ausstehend | P2 |
| memfuse-candle → Pipeline | ❌ ausstehend | P3 |

### §12.2 Priorisierte Roadmap

#### P0 — Sofort (Stunden, keine Abhängigkeiten)

| Task | Datei | Aufwand |
|---|---|---|
| **K13**: `replicator.rs` in `memfuse-db` löschen | `memfuse-db/src/replicator.rs` + `lib.rs` | 15 Min |
| **K11**: `physio-resonance-fusion = []` in Cargo.toml | `crates/memfuse-db/Cargo.toml` | 5 Min |
| **K12**: `TenantId::new()` + `From<u64>` absichern | `memfuse-core/src/types/domain.rs` | 1h |

#### P1 — Diese Woche

| Task | Datei | Aufwand | Verifikation |
|---|---|---|---|
| **K14 Kommentar**: `KvSegment`-Status klar dokumentieren | `memfuse-kv-bridge/src/segment.rs` | 30 Min | PR-Review |
| **Tauri DAG-Eintrag** | `xtask/src/main.rs` | 1h | `cargo xtask check-dag` grün |
| **F-09 β-Test**: LongMemEval mit β=0.5 vs β=0.15 | `benchmarks/memfuse-bench/` | 1 Tag | Recall-Delta < 1pp |
| **CONSTITUTION.md P8–P12** vollständig kodifizieren | `CONSTITUTION.md` | 3h | PR-Review |

#### P2 — Nächste 2–4 Wochen

| Task | Abhängigkeit | Aufwand |
|---|---|---|
| **LongMemEval + LoCoMo als CI-Gate** | Harness bereits fertig | 1 Tag |
| **KV-Bridge Increment 2**: AES-256-GCM-SIV + ModelFingerprint + rope_offset | P1 (DAG) | 2 Wochen |
| **Echter LRU** in EvictionWorker (K16) | Increment 2 | 1 Tag |

#### P3 — Monat 2

| Task | Abhängigkeit | Aufwand |
|---|---|---|
| **memfuse-candle Factory** (create_embedding_provider, create_llm_text_generator) | LongMemEval für Qualitäts-Gate (P2) | 1 Woche |
| **PhysioScheduler** implementieren (K17) | P0-K13 | 1 Woche |
| **F-03 SynapticUpdateBuffer + Fusion-Integration** | PhysioScheduler (P3) | 2 Wochen |
| **VETO-F02 Review-Deadline 2026-10-07**: Recall@10-Regressionstest | — | 1 Tag |

#### H3 — Quartal 2

| Task | Abhängigkeit |
|---|---|
| GASP Post-Hoc-Validator | memfuse-candle Pipeline (P3) |
| KV Packet Adapter-Training + RoPE-Shift | KV-Bridge Increment 2 (P2) |
| ImportanceEmbeddingClassifier | LongMemEval-CI (P2) |
| LongMemEval Overall Accuracy > 85% | Alle H2-Features aktiv |

---

## §13 Definition of Done

Eine Komponente gilt als „Done" wenn **alle** folgenden Kriterien erfüllt sind:

**Code-Qualität:**
- [ ] Zero `#[async_trait]` — native AFIT (rust-version = "1.89")
- [ ] Kein `unsafe` ohne `// SAFETY: <Beweis>`
- [ ] P3-WAL-First in allen Schreibpfaden (verifiziert via Code-Review)
- [ ] Kein unbegrenztes Warten auf nachgelagerte Operationen (P11)
- [ ] Kein `unwrap()` im Produktionscode

**Korrektheit:**
- [ ] Alle normativen Invarianten (§11) durch Tests abgedeckt
- [ ] Property-Tests für INV-PROV-1, INV-GRAPH-PROV-1
- [ ] Integration-Test für INV-DELETION-1
- [ ] Idempotenz-Test für cascade_invalidate_edges_for_superseded_doc()

**Kalibrierung:**
- [ ] `ConfigFingerprint` verdrahtet (wenn Kalibrierung involviert)
- [ ] `calibrated: false`-Fallback-Pfad explizit (kein stiller 0.5-Fallback)
- [ ] ECE-Test für jede neue `IsotonicCalibrator`-Instanz

**DAG & Abhängigkeiten:**
- [ ] `cargo xtask check-dag` grün
- [ ] `cargo xtask check-duplicate-symbols` grün (ADR-064)
- [ ] Kein zirkulärer Crate-Import

**Benchmarking (nach LongMemEval-CI-Einführung):**
- [ ] Kein Recall@K-Rückgang > 1pp gegenüber vorherigem Stand
- [ ] Latenz-Akzeptanztest für Hot-Path-Komponenten

**Governance:**
- [ ] ADR in `docs/decisions/` (für jede architektonische Entscheidung)
- [ ] Keine Verletzung von `VETOES.md`-Einträgen (verifiziert via `cargo xtask check-vetoes`)
- [ ] P7-konforme Latenz/Recall-Aussagen (reproduzierbare Messung in `memfuse-bench`)
- [ ] Feature-Flags korrekt in `Cargo.toml` deklariert (INV-F09-1)

---

## §14 Governance & Prozessmodell

### §14.1 Veto-Enforcement (aktiv, `VETOES.md` v2)

**VETO-F02:** `conditionally_accepted`, `conditional_review_due: 2026-10-07`. Keywords: `["partial hnsw rebuild", "nucleation", "rebuild_region", "F-02"]`. CI-Check via `cargo xtask check-vetoes` aktiv (#1721).

**VETO-F10:** `permanent_rejected`. Keywords: `["cross-tenant", "osmotic knowledge exchange", "F-10"]`.

**CI-Flow:** Jeder Commit → `check-vetoes` → Keyword-Matching → CI-Red bei Treffer → ADR-Review erforderlich.

### §14.2 Agentensteuerung

Alle Agenten-Sessions erhalten beim Start:
1. `CONSTITUTION.md` (Architekturprinzipien P1–P12, On-Demand)
2. `AGENTS.md` (Operative Regeln, ambient)
3. `VETOES.md` (maschinenlesbar, Feature-IDs, ambient)
4. `docs/decisions/ADR-*.md` (aktuelle ADR-Liste, On-Demand)
5. `WORKING_STATE.md` (Session-Handoff, ambient)

**Neue Pflicht v6.0:** Agenten-Session-Start prüft `cargo xtask check-duplicate-symbols` (ADR-064). Verhindert K13-artigen Drift (zwei F-07-Implementierungen) durch automatische Erkennung.

### §14.3 ADR-Prozess

```bash
# Nächste ADR-Nummer:
ls docs/decisions/ | grep -oP '(?<=ADR-)\d+' | sort -n | tail -1
```

Ausstehende ADRs:
- **ADR-F09-Cargo**: Dokumentiert K11-Fix (physio-resonance-fusion feature declaration)
- **ADR-KV-Bridge-Increment2**: Design für AES-GCM-SIV + LRU-Korrektheit
- **ADR-TenantId-K12**: Sicherheitsüberarbeitung der TenantId-Konstruktoren
- **ADR-Tauri-DAG**: memfuse-tauri Layer-4-Zuordnung

### §14.4 Sprintstruktur

- **P0** (Stunden): K13 löschen, K11 Cargo-Fix, K12 TenantId-Guard
- **P1** (Woche 1): Tauri-DAG, F-09-β-Test, CONSTITUTION-Kodifizierung
- **P2** (Woche 2–4): LongMemEval-CI-Gate, KV-Bridge-Increment-2
- **P3** (Monat 2): candle-Factory, PhysioScheduler, F-03-Integration, VETO-F02-Review
- **H3** (Quartal 2): GASP, KV Packet, ImportanceClassifier, LME > 85%

---

## Anhang A: Wettbewerbspositionierung

### Was MemFuse hat, kein Wettbewerber hat

1. WAL v3 mit HMAC-Chain (kryptographische WAL-Integrität, V1/V2-Abwärtskompatibilität)
2. KV-Cache-Bridge mit Zeroize-on-Drop, Tenant-Isolation (Increment 1 ✅, Increment 2 planned)
3. Conformal Router mit ConfigFingerprint + Lyapunov-Drift-Wächter (F-11) ✅
4. DeletionProof mit maschinenlesbarer ExcludedScope-Deklaration (DSGVO Art. 17, arXiv:2505.16831)
5. Cascade-Tombstone: PathRAG-Pfade über supersedierte Chunks werden korrekt ausgeblendet ✅
6. Physio-Selbstmanagement als kohärentes System (F-01 ✅, F-04 ✅, F-05 ✅, F-07 ✅, F-08 ✅, F-11 ✅)
7. Deutsche Morphologie (Kompositum-Dekomposition) in BM25+
8. Session-DAG mit Typ-erzwungener Lock-Reihenfolge (NodesGuard — keine Deadlocks)
9. DiskANN WAL-first Insert: kein Datenverlust-Fenster mehr (recover_pending_delta)
10. VETOES.md: Maschinenlesbares Veto-Register mit CI-Durchsetzung (ADR-064)

### Was Wettbewerber haben, MemFuse noch nicht hat (und Strategie)

| Wettbewerber-Feature | Wettbewerber | MemFuse-Antwort |
|---|---|---|
| Edge-Vektoren (2. HNSW für Kanten) | MinnsDB | F-03 Synaptische Verstärkung (H2, Berechnungslogik ✅) |
| Statischer Temporal Decay | YantrikDB | F-01 Thermostat — systemzustandsabhängig |
| Autonomer `think()`-Consolidation-Pass | YantrikDB | SleepCycle NREM ✅ + REM ✅ |
| AST-aware Code-Chunking | BrainPalace | H5 (niedrige Priorität) |

---

## Anhang B: Verworfene Features (permanent)

| Feature | Grund | Alternative |
|---|---|---|
| **F-10 Osmotischer Cross-Tenant-Austausch** | Bricht TenantId-Isolation + DeletionProof + KV-Bridge-Sicherheit. DSGVO Art. 17. | Keine — Isolation ist absolut |
| Quanten-Superpositions-Bewertung | Kein analogie-unabhängiges Akzeptanzkriterium | — |
| Lotka-Volterra literal | F-07 liefert identische Homöostase mit stärkeren Konvergenzgarantien | F-07 |
| Genetische Algorithmen für Meta-Hyperparameter | F-07+F-08 mit stärkeren theoretischen Garantien | F-07, F-08 |
| **F-02 Partieller HNSW-Rebuild (volles Re-Wiring)** | Kein echter Delaunay-Rewire. VETO-F02 conditionally_accepted für Tombstone-Pruning-Teilmenge. | 2-Phasen-CoW-Rebuild (`hnsw.rs:1693`) |

---

## Anhang C: ArXiv-Paper-Verzeichnis

### Tier 1 — Unmittelbar architektur-relevant (ADR erforderlich)

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| **2608.01460** | Conformalized LLMs under Configuration Shift | Router ConfigFingerprint (P8) |
| **2604.13226** | KV Packet: Recomputation-Free Context-Independent KV Caching | KV-Cache-Bridge (§7.1) |
| **2510.17098** | Can Transformer Memory Be Corrupted? | KV-Bridge-Sicherheit (P9) |
| **2508.09442** | Privacy Risks of KV-cache in LLM Inference | KV-Bridge-Sicherheit (P9) |
| **2605.00356** | MemRouter: Memory-as-Embedding Routing | ImportanceClassifier (§6.4) |
| **2608.12990** | LycheeMemory V2 | SleepCycle-Zielwerte (§5.5) — aktuellster SOTA |
| **2506.00610** | MemGraphRAG (Recall vs. Precision) | PathRAG Sufficiency-Gate (§5.4) |
| **2506.05690** | When to use Graphs in RAG (ICLR 2026) | PathRAG-Trigger-Logik (§5.4) |
| **2505.16831** | Unlearning Isn't Deletion | DeletionProof ExcludedScope (§3.4) |
| **2502.14902** | PathRAG (AAAI 2026) | PathRAGEngine-Basis (§5.4) |
| **2607.04223** | GASP: Grounding-Aware Sensitivity by Perturbation | Post-Hoc-Validator (§7.2, H3) |

### Tier 2 — Wissenschaftliche Validierung

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| 2603.14517 | SleepGate | SleepCycle-Interferenzhorizont (§5.5) |
| 2605.17625 | Episodic-Semantic Memory Architecture | SleepCycle Dual-Process (§5.5) |
| 2604.01733 | From BM25 to Corrective RAG (T2-RAGBench) | RRF-Konfiguration (§5.1) |
| 2605.18796 | UCCI | Conformal Router / Lyapunov-Basis (§6.2) |
| 2603.06616 | RACER | Abstention-Pfad (§6.1) |
| 2601.02993 | Stable-RAG | Chunk-Injektionsreihenfolge (§5.1) |
| 2603.14828 | Robust Multi-Hop GraphRAG | Graph-Provenienz-Pflicht (§4.5) |
| 2603.15033 | MUNKEY | DeletionProof-Stützung (§3.4) |
| 2602.21514 | I/O Optimizations for Graph-Based ANN | DiskANN Streaming-Insert (§4.4) |
| 2410.10813 | LongMemEval (ICLR 2025) | Benchmark-Integration (§8.1) |

### Tier 3 — Kritische Gegenposition

| ArXiv-ID | Titel | MemFuse-Risiko |
|---|---|---|
| 2604.09666 | Do We Still Need GraphRAG? | PathRAG-Scope-Beschränkung (§5.4) |
| 2603.19664 | The Residual Stream Is All You Need? | KV-Cache-Fallback-Design (§7.1) |

---

## Anhang D: Technische Schulden (Stand v6.0)

| Schuld | Impact | Horizont | Neu in v6.0? |
|---|---|---|---|
| **K11: physio-resonance-fusion fehlt in Cargo.toml** | F-09 in keinem Build aktivierbar — P7-Verstoß | **P0** | ✅ NEU |
| **K12: TenantId::new(0) + From<u64> ohne Guard** | INV-TENANT-1 semantisch unterlaufen | **P1** | ✅ NEU |
| **K13: AdaptiveFusionWeights in memfuse-db** | P10-Verletzung, totes Duplikat | **P0** | ✅ NEU |
| **K14: KvSegment ohne Krypto + ModelFingerprint** | KV-Bridge Increment 1 ist Skeleton, keine Production-Security | P2 | ✅ NEU |
| **K16: FIFO statt LRU im EvictionWorker** | Inkorrekte Eviction-Policy | P2 | ✅ NEU |
| **K17: PhysioScheduler fehlt** | Physio-Subsysteme unkonsolidiert | P3 | ✅ NEU |
| **K18: F-03 ohne Buffer + Fusion-Integration** | 5. Signal nicht verfügbar | H2 | ✅ NEU |
| **K20: memfuse-tauri ohne DAG-Eintrag** | Layer-Verletzung unentdeckbar | P1 | ✅ NEU |
| LongMemEval-CI fehlt als Gate | Keine Recall-Regressions-Baseline | P2 | aus v5.0 |
| memfuse-candle nicht in Pipeline | Sovereign Core = Marketing | P3 | aus v5.0 |
| GASP nicht implementiert | Post-Hoc-Validierung nicht verfügbar | H3 | aus v5.0 |
| F-09 β=0.5 vs Spec 0.15 — empirisch klären | Recall-Impact unbekannt | P1 | ✅ NEU |
| HNSW VETO-F02 Review | Frist: 2026-10-07 | P3 | aus v5.0 |
| ImportanceClassifier nicht trainiert | LLM-Hot-Path-Latenz > Ziel | post-LME | aus v5.0 |
| `unsafe` in KvSegment-Tests ohne vollständige SAFETY-Kommentare | P2-Verletzung | P1 | aus v5.0 |

---

*Dieses Dokument ist die einzige normative Wahrheitsquelle für MemFuse-Architektur, -Features und -Roadmap (v6.0 — „Verified Continuity"). Es ersetzt vollständig v5.0 (`memfuse_gesamtspezifikation_v5_0.md`, HEAD `bb099dc2`). Alle Widersprüche zwischen v5.0 und HEAD `05b382d8` sind in §0.2 normativ aufgelöst und begründet.*

*HEAD-Referenz: `05b382d8` · 07. September 2026 (Abend-Audit) · ~117.200 LOC Rust · 18 Workspace-Crates + xtask + memfuse-py (separater Workspace, ADR-064)*

*Nächste Überarbeitung: nach Abschluss P0-Fixes (K11, K12, K13) und LongMemEval-CI-Gate-Einführung (P2).*
