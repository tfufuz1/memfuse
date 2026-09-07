# MemFuse — Konsolidierte Gesamtspezifikation v5.0

> **Dokument-Typ:** Normative Gesamtspezifikation — einzige maßgebliche Wahrheitsquelle  
> **Version:** 5.0 — „Sovereign Synthesis" (löst v4.0, v4.1 und Principal-Architect-Review vollständig ab)  
> **Stand:** 07. September 2026  
> **HEAD:** `bb099dc2` · 1.248 Commits · ~119.700 LOC Rust · 17 Workspace-Crates  
> **Konsolidiert aus:**  
> — `memfuse_spec.md` v4.0 (Goldstandard, HEAD `36ad007a`, 101.011 LOC, 15 Crates)  
> — `memfuse_architektur_praezisierung.md` v4.1 (Code-Audit, HEAD `36ad007a`, 102.199 LOC)  
> — `memfuse_principal_architect_review_2026-09-07.md` (Live-Verifikation HEAD `bb099dc2`, 119.700 LOC)  
> **Syntheseprinzip:** Jede Aussage ist (a) live-code-verifiziert (HEAD `bb099dc2`) **oder** (b) arXiv-belegt **oder** (c) aus expliziter Konflikt­lösung mit dokumentiertem Entscheid. Keine unverifizierten Übernahmen aus Vorgängerdokumenten.

---

## Inhaltsverzeichnis

- **§0** Methodische Grundlagen, Konfliktlösungen & Quell-Hierarchie
- **§1** Produktvision, Säulen & Architekturprinzipien (P1–P12)
- **§2** Crate-Topologie — Vollständige Ziel-Architektur (17 Crates)
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
- Anhang D: Technische Schulden (nach v5.0-Stand)

---

## §0 Methodische Grundlagen, Konfliktlösungen & Quell-Hierarchie

### §0.1 Hierarchie der Quellen (unveränderlich)

Wo Quellen sich widersprechen, gilt folgende Rangfolge:

1. **Live-Code HEAD `bb099dc2`** (Principal-Architect-Review, 07.09.2026) — schlägt alle Dokumente
2. **Code-Audit HEAD `36ad007a`** (v4.1, 07.09.2026 Morgen) — schlägt v4.0-Behauptungen
3. **v4.0 Spec** — Basis-Referenz, aber explizit korrigiert wo Code abweicht
4. **ArXiv-Paper** (nach Datum, jüngeres schlägt älteres) — für algorithmische Entscheidungen
5. **PRD-Features** — nur soweit durch (a)–(d) stützbar

### §0.2 Vollständige Konfliktlösungsmatrix

Die folgende Tabelle dokumentiert **jeden** Widerspruch zwischen den drei Quelldokumenten und dessen normative Auflösung:

| # | Konflikt | v4.0-Aussage | Code-Realität (bb099dc2) | **Normative Auflösung v5.0** |
|---|---|---|---|---|
| **K1** | MemTable-Impl | SkipList-basiert | 16-Shard `BTreeMap<Bytes, Vec<MemTableEntry>>` mit `parking_lot::RwLock` | **BTreeMap-Sharding** — garantierte lexikographische Sortierung für SSTable-Flush (INVARIANT-3), ausreichend Parallelismus bei 8-Thread-Embed-Pipeline. Migration auf `crossbeam-skiplist` nur nach Perf-Nachweis mit echter Last. |
| **K2** | HNSW-Parameter | `ef_construction=32` | `ef_construction=200`, `ef_search=64` | **ef_construction=200** — Recall@10 ≈ 0.98 (Precision-Optimum auf ANN-Benchmarks). Spec-Wert war Minimum, nicht Default. `M=16` bleibt pending Audit `grep -n "pub m\|\.m =" hnsw.rs`. |
| **K3** | async_trait-Nutzung | 125 Annotationen | **0** — vollständig auf AFIT migriert (`grep` negativ im gesamten Workspace) | **0 async_trait** — vollständige AFIT-Migration ist produktiv. Kein `Box<dyn Future>` mehr im Hot-Path. |
| **K4** | LOC / Crates | 101.011 LOC, 15 Crates | 119.700 LOC, 17 Crates | **119.700 LOC, 17 Crates** — Delta: ~18.700 LOC, 2 neue Crates (86 Commits zwischen den Audits). |
| **K5** | G0-Implementierungsstand | Alle G0-Items als "fehlend" markiert | Praktisch alle G0/H1-Items implementiert | **Vollständig implementiert** — siehe §12.1 für detaillierten Status. Roadmap v5.0 beginnt bei P0-Governance. |
| **K6** | F-02 VETO | Permanentes Architektur-Veto | `physio-nucleation`-Feature-Flag implementiert (`HnswIndex::rebuild_region()`, Commit `6488d715`) | **VETO BLEIBT** — Implementierung ist kein vollständiger partieller Rebuild, sondern nur Tombstone-Bereinigung ohne Ersatzkanten. Feature bleibt **hart deaktiviert** bis Recall@10-Regressionstest existiert. ADR erforderlich. |
| **K7** | Reranking-Fix | `k*3=30` statisch → Fix auf max(k*3, 100) | `RerankPidController` (Commits #1699, #1702) implementiert — dynamisch statt statisch | **PID-Controller** ist die finale Lösung — übertrifft statisches Minimum. Statische `min_rerank_candidates=100` als Untergrenze im PID-Controller bleibt gültig (wissenschaftlich: ≥ 100 für Recall@5 = 0.888). |
| **K8** | memfuse-candle Status | Als "NEU" / fehlend markiert | Crate existiert (488 LOC: GGUF-Loader, Inferenz, Embedding, Model-Registry) **aber NICHT** in Serving-Pipeline verdrahtet | **Fundament steht, Pipeline-Integration fehlt** — `Sovereign Core`-Claim bleibt unvollständig bis Ollama-Ausstieg vollzogen. P3-Priorität. |
| **K9** | ImportanceClassifier | Als "fehlend" markiert, k-NN Variante B empfohlen | Commit `f7600262` konsolidiert auf LLM-Pfad statt Classifier | **Bewusst zurückgestellt** — korrekte Entscheidung angesichts fehlenden gelabelten Evaluationsdatensatzes. Erst nach LongMemEval-CI-Integration (§12) angehen. |
| **K10** | Workspace-Mitglied `memfuse-py` | Im Workspace | Verzeichnis existiert, **nicht** in `Cargo.toml members` | **Sofort in Cargo.toml aufnehmen** — bereits markierter Sicherheits-Fix (`panic=abort`, AGT-PY-d5d2be30 RESOLVED) wird sonst nicht von CI erfasst. |

### §0.3 Permanente Architektur-Vetos (unveränderlich)

**VETO F-02 — Partieller HNSW-Rebuild:**
HNSW ist global verschränkter Graph. `HnswIndex::rebuild_region()` (Commit `6488d715`) führt keinen echten Delaunay-Rewire durch — er entfernt nur Tombstone-Referenzen ohne Ersatzkanten. Betroffene Knoten verlieren dauerhaft Grad ohne Wiederherstellung der Navigierbarkeit. `physio-nucleation` bleibt **hart deaktiviert** bis ein Recall@10-Regressionstest (vor/nach `rebuild_region()` bei realistischer Tombstone-Verteilung) grün ist. Ersatz: 2-Phasen-CoW-Rebuild (ADR-061, `hnsw.rs:1693+1812`, produktionsreif) + F-01-Thermostat als präventives Signal.

**VETO F-10 — Osmotischer Cross-Tenant-Wissensaustausch:**
Bricht `TenantId`-Isolationsgarantie (§3.4), KV-Cache-Sicherheitsschicht (§7.1) und `DeletionProof`-Beweisbarkeit (§3.5). Kryptographische Löschgarantien sind über fließende Mandantengrenzen mathematisch nicht beweisbar. DSGVO Art. 17 ist legal-relevant — kein Feature darf seine Beweisbarkeit zerstören. **Keine Alternative empfohlen.** Mandantenisolation ist absolut.

### §0.4 Sofort-Prioritäten P0 (vor jeder Feature-Arbeit)

| Priorität | Maßnahme | Begründung |
|---|---|---|
| **P0-GOV-1** | `VETOES.md` maschinenlesbar neben `CONSTITUTION.md` einführen | F-02-Veto am Morgen, Umsetzung am Nachmittag desselben Tages — Governance-Lücke bewiesen |
| **P0-GOV-2** | CI-Check: Neue Commits gegen Feature-IDs aus `VETOES.md` (Stichwort-Matching) | Voraussetzung: P0-GOV-1 |
| **P0-F02** | Recall@10-Regressionstest für `rebuild_region()` schreiben | Bis Test grün: `physio-nucleation` CI-Pflicht-disabled |
| **P0-PY** | `memfuse-py` in `Cargo.toml members` aufnehmen | AGT-PY-d5d2be30-Fix sonst nicht von CI erfasst |

---

## §1 Produktvision, Säulen & Architekturprinzipien

### §1.1 Kernaussage

**MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten und wissensintensive Einzelanwender — die Erinnerung nicht nur speichert, sondern konsolidiert, kalibriert, ihre eigene Löschung kryptographisch beweist, Widersprüche immunologisch abwehrt und sich nach physikalisch-biologischen Prinzipien selbst reguliert. Alles läuft auf Nutzerhardware, ohne Cloud-Zwang für den Kernbetrieb.**

### §1.2 Fünf Produktsäulen (nicht verhandelbar)

**Säule I — Datenhoheit (Sovereign Core):** Daten und Inferenz laufen vollständig auf Nutzerhardware. Ohne Cloud-Abhängigkeit für den Kernbetrieb. Status: `memfuse-candle` existiert als technisches Fundament (GGUF-Loader, Inferenz, Embedding, Model-Registry). **Offen:** Pipeline-Integration (Ollama-Ausstieg, P3). Bis dahin ist "Sovereign Core" in der Serving-Pipeline noch nicht vollständig — P7-Anforderung bis P3-Erledigung als "Fundament bereit, Deployment ausstehend" deklarieren.

**Säule II — Belegbare Korrektheit:** Jedes Suchergebnis trägt eine nachvollziehbare Herkunftskette (`INV-PROV-1`: `sum(contributions) ≈ rrf_score`, `INV-PROV-2`: `coherence_bonus` separates Feld). Jeder Schreibvorgang ist WAL-first crash-sicher. Temporal-Validity-Post-Filter implementiert.

**Säule III — Hybride Retrieval-Qualität:** 3-Signal-RRF (Vektor + BM25 + Graph) + Resonanz-Kohärenz-Bonus (F-09) + PathRAG (Multi-Hop) + PID-geregelter Reranker. Outcome-kalibriert via `memfuse-calibration`.

**Säule IV — Gehärtete Kalibrierung & Cache-Sicherheit:** ConfigFingerprint-Zwang (P8) in Router + Reranker + Calibration verdrahtet. Kein Klartext-Sensitivspeicher (P9). Lyapunov-Drift-Wächter (F-11). Unified Calibration Primitive (`memfuse-calibration`, produktiv).

**Säule V — Physikalisch kohärentes Selbstmanagement:** F-01 (Thermostat ✅), F-03 (Synaptische Verstärkung — H2), F-04 (Immunologische Widerspruchsprävention ✅), F-05 (REM-Synthese — H3), F-06 (Perkolationsmonitor — H2), F-07 (Replikatordynamik ✅ in calibration), F-08 (PID-Latenz-Homöostat ✅ als RerankPidController), F-09 (Resonanz-Kohärenz-Bonus ✅), F-11 (Lyapunov-Drift-Wächter — H1). Alle features-flagged, P1-safe Defaults.

### §1.3 Architekturprinzipien P1–P12

**P1 — DAG-Integrität:** `cargo xtask check-dag` ist CI-Gate. Kein Fachcode in Layer ≥N mit Wissen über Layer >N. Kein Merge ohne grünen DAG-Check.

**P2 — Zero-Panic-Doctrine:** `unsafe` ausschließlich in `distance.rs` (SIMD), `diskann.rs` + `persistence.rs` (Mmap), `kv_bridge/security.rs` (Zeroize). Jedes `unsafe`-Block trägt `// SAFETY: <Beweis>`. Libraries dürfen Host-Prozess nicht crashen. Gilt auch für Tests.

**P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. `let _ = dir.sync_all()` ist ein P3-VIO. `fsync` nach jedem WAL-Eintrag vor MemTable-Update.

**P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` (beide in `memfuse-core`) sind die einzigen LLM-Abstraktionsgrenzen. Kein Fachcode in Layer 2–4 mit backend-spezifischem Wissen.

**P5 — Kein Cloud-Zwang:** Jede Komponente, deren einziger Betriebspfad eine externe Netzwerkabhängigkeit ist, benötigt ADR-dokumentierte Ausnahme. Ollama ist Standardpfad, kein Pflichtpfad.

**P6 — Eine Quelle für Architekturentscheidungen:** Ausschließlich `DECISIONS.md`. Keine parallelen Architektur-Dokumente ohne Rückverweis und ADR.

**P7 — Marketing-Aussagen sind an Code-Nachweise gebunden:** Jede quantitative Aussage (Latenz, Recall, Fehlerrate) benötigt eigene, reproduzierbare Messung in `memfuse-bench` oder explizite Kennzeichnung "fremdreferenziert, an MemFuse nicht validiert".

**P8 — Kalibrierungs-Integrität:** Jede Änderung an `prompt_template_hash`, `temperature_bits` oder `quantization` invalidiert automatisch und unmittelbar alle Kalibrierungsstatistiken (Router, Reranker, ImportanceScore). Warmup-Fenster darf nach solcher Änderung nicht übersprungen werden. Implementiert: `IsotonicCalibrator::invalidate_on_config_change()` in `memfuse-calibration`, verdrahtet in Router, Reranker, Calibration. Begründung: arXiv:2608.01460.

**P9 — Kein Klartext-Sensitivspeicher:** Tensor-Zustände aus Nutzerdaten (KV-Cache-Segmente) liegen zu keinem Zeitpunkt unverschlüsselt auf persistentem oder auslagerbarem Speicher. Zeroize-on-Evict ist Pflicht via dediziertem Eviction-Worker-Thread (nicht im Inferenz-Hot-Path blockierend). Begründung: arXiv:2510.17098 (MTI-Angriff) + arXiv:2508.09442 (Inversion).

**P10 — Reuse-vor-Neubau:** Vor jedem neuen Arbeitspaket: expliziter Wiederverwendungs-Check im PR-Template. Bewährte Kandidaten: `score_batch()` (reranker.rs:195), `tombstoned_edges` (csr.rs), `IsotonicCalibrator` (memfuse-calibration), `KeyManager` (kein zweiter Krypto-Stack).

**P11 — Latenzbudget-Pflicht für Hot-Path:** Jede Operation im Retrieval- oder Ingestion-Hot-Path benötigt explizites Latenzbudget mit hartem Deadline-Abbruchpfad. Implementiert: `RerankDeadline` + `RerankPidController` (Commits #1699, #1702).

**P12 — Physio-Feature-Default-Unsichtbarkeit:** Kein physio-inspiriertes Feature erzeugt im Zero-IT-Setup-Default sichtbares, erklärungsbedürftiges Verhalten. Alle `physio-*`-Features sind per Feature-Flag deaktivierbar. Zentrales `PhysioConfig`-Struct in `memfuse-core` (§10.2).

---

## §2 Crate-Topologie — Vollständige Ziel-Architektur (17 Crates)

```
Layer 0 — Fundament (kein I/O, keine externen Abhängigkeiten)
│
├── memfuse-core          Traits, Typen, Fehler-Hierarchie, DAG-Guard
│   ├── types/            TxId, DocId, TenantId [✅], CollectionId
│   │                     ConfigFingerprint [✅], ModelFingerprint [✅]
│   ├── traits/           LlmTextGenerator, TextEmbeddingEngine
│   │                     StorageEngine, VectorIndex, TextIndex
│   │                     GraphIndex, CheckpointCoordinator
│   │                     (alle: AFIT — kein async_trait [✅])
│   ├── error.rs          Vollständige Fehler-Hierarchie (thiserror)
│   ├── tx_buffer.rs      MVCC-TxBuffer
│   ├── physio.rs         PhysioConfig — alle Physio-Parameter [§10.2]
│   └── seq_log.rs        Sequenz-Monotonie-Guard (ADR-016)
│
├── memfuse-crypto        Kryptographie-Primitive (keine Netz-I/O)
│   ├── crypto.rs         AES-256-GCM-SIV (RFC 8452), HKDF, KeyManager [✅]
│   ├── deletion_proof.rs DeletionProof [✅] + DeletionLayer-Enum
│   ├── kv_cipher.rs      KvSegmentCipher — wiederverwendet KeyManager
│   └── hmac_chain.rs     WAL-HMAC-Chain-Verifizierung [✅ produktiv, wal.rs:45-80]
│
└── memfuse-calibration   [✅ existiert] Unified Calibration Primitive (Layer 0/1)
    ├── lib.rs            IsotonicCalibrator (PAVA), PlattScaler
    │                     invalidate_on_config_change()
    └── replicator.rs     Replikatordynamik (F-07) [✅ in calibration verdrahtet]

Layer 1 — Storage-Primitiven (I/O, kein LLM)
│
├── memfuse-store         LSM-Tree, WAL v3, SSTable, Mmap
│   ├── wal.rs            HMAC-Chain-WAL v3, Atomic-Commit [✅ produktiv]
│   ├── memtable.rs       16-Shard BTreeMap, parking_lot::RwLock [✅ K1-Auflösung]
│   ├── sstable.rs        Bloom-Filter, CRC32-Verifikation
│   ├── compaction.rs     Background-Compaction, Tombstone-Tracking
│   ├── mmap.rs           Mmap-backed Reads, Sector-aligned
│   └── tenant_codec.rs   TenantKeyCodec [✅] — Prefix-Encoding
│
├── memfuse-index         Vektorindizes
│   ├── hnsw.rs           HNSW (M=16, ef_construction=200 [✅ K2], ef_search=64)
│   │                     2-Phasen-CoW-Rebuild [✅ hnsw.rs:1693+1812]
│   │                     rebuild_region() [feature-flagged, physio-nucleation, DEAKTIVIERT bis F02-Test]
│   ├── diskann.rs        DiskANN + persist_delta() [✅ diskann.rs:510]
│   │                     Pending-Buffer + Streaming-Insert + atomares Rename
│   ├── distance.rs       SIMD AVX2/NEON (unsafe, ADR-017)
│   ├── quantize.rs       Q4/Q8 Scalar Quantization
│   └── persistence.rs    Binary-Format, Mmap-Load (unsafe, ADR-017)
│
├── memfuse-text          Volltext-Retrieval
│   ├── bm25.rs           BM25+ Robertson-Spärck-Jones IDF: ln(1+(N−df+0.5)/(df+0.5)) [✅ gefixt]
│   ├── tokenizer.rs      DE-Morphologie (Kompositum-Dekomposition)
│   └── ngram.rs          N-Gramm-Generierung
│
├── memfuse-graph         Graph-Datenstrukturen
│   ├── csr.rs            CSR-Graph, tombstoned_edges [✅]
│   ├── session_dag.rs    SessionBranchTree, NodesGuard [✅ session_dag.rs:29]
│   ├── ppr.rs            PPR, damping=0.85, bidirektional [✅ ppr.rs:133,343]
│   ├── community.rs      Label-Propagation, SimpleRng LCG deterministisch [✅ community.rs:55]
│   ├── path_rag.rs       PathRAGEngine [✅] + Query-Klassifikator + Sufficiency-Gate
│   ├── immune.rs         ImmunMemory [✅] (F-04) Antikörper-Register
│   └── provenance.rs     Kanten-Provenienz-Invariante (INV-GRAPH-PROV-1) + Cascade-Tombstone
│                         [⚠️ cascade_tombstone_superseded_edges FEHLT NOCH — P1]
│
├── memfuse-checkpoint    Checkpoint-Management (konsolidierte Fassade)
│   └── lib.rs            CheckpointStore (EINE Fassade) [Konsolidierung: H3]
│
└── memfuse-embed         Embedding & Klassifikation
    ├── embedder.rs       ONNX-Embedding (Arc<Mutex<Session>>)
    ├── reranker.rs       CrossEncoderReranker, ConfigFingerprint [✅ verdrahtet]
    │                     PlattScaler via memfuse-calibration [✅]
    │                     RerankDeadline + RerankPidController [✅ Commits #1699,#1702]
    └── importance_classifier.rs ImportanceClassifier [deferred — post-LongMemEval]

Layer 2 — Orchestrierung
│
└── memfuse-db            Geschäftslogik-Orchestrierung
    ├── fusion.rs         3-Signal-RRF + Resonanz-Kohärenz-Bonus F-09 [✅ Commit #1698]
    ├── temporal_filter.rs Temporal-Validity-Post-Filter [✅]
    ├── collection/       CollectionEngine, QueryBuilder, SearchEngine
    ├── context.rs        DualProcessMemory (Episodisch + Semantisch)
    ├── context_compaction.rs ContextCompactor (NREM-Phase)
    ├── sleep_cycle.rs    SleepCycleScheduler NREM+REM (F-05) [H3]
    ├── homeostat.rs      PID-Regler Retrieval-Homöostase (F-08) [✅ als RerankPidController]
    ├── thermostat.rs     Freie-Energie-Thermostat (F-01) [✅ memfuse-db/src/thermostat.rs]
    ├── replicator.rs     Adaptive RRF-Gewichte (F-07) [✅ in calibration]
    ├── multistep.rs      MultiStepEngine
    ├── reaper.rs         TTL-basierte Eviction
    ├── gasp.rs           GASP Post-Hoc-Halluzinations-Validator [H2, erfordert candle]
    └── transaction.rs    MVCC-Transaktions-Management

Layer 3 — Inferenz & Routing
│
├── memfuse-agent         Workflow-Engine
│   ├── engine.rs         AgentWorkflowEngine
│   ├── dlq.rs            Dead-Letter-Queue
│   ├── step.rs           AgentStep-Definitionen
│   └── audit.rs          Audit-Trail
│
├── memfuse-router        Conformal Router
│   ├── router.rs         RouterEngine, RoutingDecision, Abstention-Pfad [✅]
│   ├── profile.rs        SlmProfile + ConfigFingerprint [✅ verdrahtet]
│   ├── dispatch.rs       Dispatch-Logik
│   └── lyapunov.rs       Lyapunov-Drift-Wächter (F-11) [H1 — Status unklar, Verifikation nötig]
│
├── memfuse-ollama        Ollama-Backend (aktueller Standardpfad)
│   ├── client.rs         LlmTextGenerator-Impl, präventiver Halluzinations-Guard [✅]
│   └── importance.rs     [DEPRECATED im Hot-Path → ImportanceClassifier post-LongMemEval]
│
├── memfuse-candle        Pure-Rust-GGUF-Backend [✅ Crate existiert, NICHT in Pipeline]
│   ├── lib.rs            CandleLlmClient, CandleEmbedClient (488 LOC)
│   ├── gguf_loader.rs    GGUF-Parser
│   ├── inference.rs      LlmTextGenerator-Impl (AFIT, kein async_trait)
│   ├── embedding.rs      TextEmbeddingEngine-Impl
│   └── model_registry.rs ModelFingerprint (SHA-256 + Quantisierungsgrad)
│   [P3: Pipeline-Verdrahtung in memfuse-db/memfuse-ollama/memfuse-router ausstehend]
│
└── memfuse-py            PyO3-Bindings [✅ Crate vorhanden; ⚠️ NICHT in Cargo.toml members — P0]
    └── lib.rs            panic=abort [✅ AGT-PY-d5d2be30 RESOLVED]

Layer 4 — Integrations-Grenzschicht
│
├── memfuse-kv-bridge     KV Packet KV-Cache-Bridge [P2 — Sicherheitsschicht zuerst]
│   ├── lib.rs            KvCacheProvider-Trait, KvSegment
│   ├── kv_packet.rs      KV Packet Adapter-Training + RoPE-Shift [H3]
│   ├── security.rs       VRAM-Verschlüsselung, Zeroize-on-Evict via Worker-Thread [P2]
│   └── tenant_isolation.rs Cross-Tenant-Cache-Isolation
│
├── memfuse-mcp           MCP JSON-RPC 2.0 stdio [✅ produktiv]
│   ├── protocol.rs       JSON-RPC-Handler, E2E-Test (#1613)
│   ├── prompt_injection.rs Injection-Detection
│   └── sandbox.rs        Sandbox-Isolation
│
└── memfuse-tauri         Desktop-UI
    ├── commands/         Tauri-Commands (search, ingest, session_dag, …)
    └── session_dag.rs    Branch-UI-Commands [✅]

Layer 5 — Evaluation
└── memfuse-bench         Benchmark-Harness
    ├── long_mem_eval.rs  LongMemEval-S Integration [P2 — dringendste Infrastruktur]
    ├── locomo.rs         LoCoMo Integration [P2]
    └── metrics.rs        Recall@K, MRR, BenchmarkMetrics
```

**Legende:** ✅ = produktiv in HEAD `bb099dc2` · ⚠️ = Lücke · H1/H2/H3 = Roadmap-Horizont · P0/P1/P2/P3 = Priorität

---

## §3 Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie

### §3.1 Kern-Typen (`memfuse-core/src/types/domain.rs`)

Alle Typen: `Copy + Hash + Eq + Serialize/Deserialize`.

```rust
/// Primäre Schlüsseltypen
pub struct TxId(pub u64);      // Logische Sequenz (ADR-016: kein SystemTime!)
pub struct DocId(pub u64);
pub struct CollectionId(pub String);

/// [✅ implementiert] Mandanten-Identifikator.
/// In memfuse-core (Layer 0) definiert — verhindert zyklische Crate-Abhängigkeit
/// mit KV-Cache-Isolation (Layer 4).
pub struct TenantId(pub u64);

impl TenantId {
    pub const SYSTEM: TenantId = TenantId(0);
    /// INV-TENANT-1: try_new(0) → Err (SYSTEM ist reserviert)
    pub fn try_new(id: u64) -> Result<Self, TenantIdError>;
}

/// [✅ implementiert] Konfigurations-Fingerabdruck für P8-Kalibrierungs-Integrität.
pub struct ConfigFingerprint {
    pub model_id: String,
    pub quantization: Option<String>,       // Q4 ≠ Q8 — Teil des Fingerabdrucks!
    pub prompt_template_hash: [u8; 32],     // Blake3-Hash des Prompt-Templates
    pub temperature_bits: u32,              // f32::to_bits() — Hash-/Eq-fähig
}

/// [✅ implementiert] Modell-Identifikator inklusive Quantisierungsgrad.
/// Q4 → Q8 bei gleichem model_id erzeugt verschiedene Fingerabdrücke.
pub struct ModelFingerprint {
    pub hash: [u8; 32],          // SHA-256(Gewichts-Blob || Quantisierungs-String)
    pub model_id: String,
    pub quantization: String,
}
```

### §3.2 Kern-Traits (`memfuse-core/src/traits/`)

**Kein `#[async_trait]`** — vollständige AFIT-Migration (K3, `grep` negativ im gesamten Workspace, HEAD `bb099dc2`).

```rust
// inference.rs
pub trait LlmTextGenerator: Send + Sync {
    async fn generate_text(&self, model: &str, prompt: &str) -> Result<String>;
    async fn generate_with_system(&self, model: &str, system: &str, prompt: &str) -> Result<String>;
    /// Log-Likelihoods für GASP (§7.2). None bei Ollama-HTTP (keine Logit-API).
    /// Verfügbar: memfuse-candle (native). Nicht verfügbar: Ollama-HTTP.
    async fn log_likelihood(&self, model: &str, prompt: &str, continuation: &str)
        -> Result<Option<f64>>;
    fn config_fingerprint(&self) -> ConfigFingerprint;
}

// embedding.rs
pub trait TextEmbeddingEngine: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn embedding_dimension(&self) -> usize;
    fn model_fingerprint(&self) -> ModelFingerprint;
}

// storage.rs
pub trait StorageEngine: Send + Sync {
    async fn get(&self, tx: &TxId, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tx: &TxId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, tx: &TxId, key: &[u8]) -> Result<()>;
    /// INV-TENANT-2: Gibt AUSSCHLIESSLICH Keys des zugehörigen Tenants zurück
    async fn scan_prefix(&self, tx: &TxId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    async fn begin_tx(&self) -> Result<TxId>;
    async fn commit_tx(&self, tx: TxId) -> Result<()>;
    async fn abort_tx(&self, tx: TxId) -> Result<()>;
}

// vector_index.rs
pub trait VectorIndex: Send + Sync {
    async fn insert(&self, id: DocId, vector: &[f32]) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(DocId, f32)>>;
    async fn delete(&self, id: DocId) -> Result<()>;
    async fn build(&self, vectors: &[(DocId, Vec<f32>)]) -> Result<()>;
    async fn persist(&self, path: &std::path::Path) -> Result<()>;
}
```

### §3.3 Unified Calibration Primitive (`memfuse-calibration`) — ✅ Produktiv

**Status:** Crate existiert, Isotonic + Platt + Replicator-Dynamics in Router, Reranker, Calibration verdrahtet (HEAD `bb099dc2`).

**Wissenschaftliche Basis:** UCCI (arXiv:2605.18796). Drei konsolidierte Kalibrierungsprobleme:

1. `memfuse-router`: Conformal-Router-Fehlerwahrscheinlichkeit ✅
2. `memfuse-embed/src/reranker.rs`: Platt-kalibriert via memfuse-calibration ✅
3. `memfuse-ollama/src/importance.rs`: LLM-Call [DEPRECATED im Hot-Path]

```rust
// crates/memfuse-calibration/src/lib.rs [✅ produktiv]

pub struct IsotonicCalibrator {
    warmup_required: u32,
    observations: VecDeque<(f32, bool)>, // (roh-Score, tatsächliches Outcome)
    max_observations: usize,             // Default: 10.000
    fingerprint: Option<ConfigFingerprint>,
}

impl IsotonicCalibrator {
    /// INV-CAL-1: None wenn warmup < warmup_required. KEIN 0.5-Fallback.
    pub fn calibrated_probability(&self, raw_score: f32) -> Option<f32>;

    /// INV-CAL-2: Setzt Beobachtungen auf 0. Kein partielles Übernehmen.
    /// P8-PFLICHT: Muss bei ConfigFingerprint-Änderung aufgerufen werden.
    pub fn invalidate_on_config_change(&mut self, new_fingerprint: ConfigFingerprint);

    /// ECE-Ziel: < 0.03 (UCCI-Referenz, arXiv:2605.18796)
    pub fn expected_calibration_error(&self) -> Option<f32>;
}

pub struct PlattScaler {
    a: f32, b: f32, is_fitted: bool,
}
```

### §3.4 TenantKeyCodec (`memfuse-store/src/tenant_codec.rs`) — ✅ Produktiv

Schlüsselstruktur: `t:{tenant_id}:{collection_id}:{doc_type}:{doc_id}`

`scan_prefix(b"t:{tenant_id}:")` gibt **ausschließlich** Keys dieses Tenants zurück — O(1) Overhead, kein Cross-Tenant-Leak.

### §3.5 DeletionProof (`memfuse-crypto/src/deletion_proof.rs`) — ✅ Produktiv

**Wissenschaftliche Basis:** MUNKEY (arXiv:2603.15033) + arXiv:2505.16831 ("Unlearning Isn't Deletion").

**KRITISCHE INVARIANTE (INV-DELETION-1):**
`DeletionProof::create()` wird **NUR** nach vollständiger physischer Layer-Bereinigung aller in `covered_layers` deklarierten Schichten aufgerufen. Zu früher Aufruf macht den kryptographischen Löschnachweis mathematisch wertlos.

**PFLICHT-Grenze in Enterprise-Dokumentation:**
Dieser Proof deckt **ausschließlich** die Storage-Ebene ab (LSM, WAL, HNSW, CSR, KV-Cache). Er **kann nicht** garantieren, dass Wissen aus konsolidierten Zusammenfassungen, die zum Fine-Tuning eines Drittmodells verwendet wurden, aus jenem Modell entfernbar ist (DSGVO Art. 17 — Rechtlich relevant).

```rust
pub struct DeletionProof {
    pub scope: DeletionScope,
    pub deleted_keys_hash: [u8; 32],     // Blake3-Hash aller gelöschten Keys (sortiert)
    pub deleted_at_unix_secs: u64,
    pub signature: [u8; 32],             // HMAC-SHA256 über (scope||keys_hash||timestamp)
    pub wal_seq_after_deletion: u64,
    pub covered_layers: Vec<DeletionLayer>,   // Audit-fähig: explizite Deckungsgrenze
    pub excluded_scopes: Vec<ExcludedScope>,  // Explizite Nicht-Abdeckung (DSGVO-Pflicht)
}

pub enum DeletionLayer {
    LsmMemtable, SsTableAllLevels, HnswIndex, WalAllSegments,
    CsrGraph, KvCacheSegments, EmbeddingCache,
}

pub enum ExcludedScope {
    ConsolidatedAndDistilled, // SleepCycle-Synthesen die zum Fine-Tuning genutzt wurden
    LlmParameterMemory,       // LLM-Modellparameter (bei memfuse-candle)
}
```

---

## §4 Layer 1 — Storage-Primitiven

### §4.1 WAL v3 mit HMAC-Chain — ✅ Produktionsreif

**Verifiziert:** `wal.rs:45-80` — HMAC-SHA256-Chain, AES-256-GCM-SIV via `KeyManager::encrypt_auto_nonce`. Chaos-Test-Suite aktiv.

**Invarianten (nicht verhandelbar):**
- WAL-Commit **vor** MemTable-Update (`INV-P3-1`)
- `dir.sync_all()` nach Parent-Directory-Rename — kein `let _ =`
- HMAC-Chain-Verifikation beim Replay vor jedem Recovery
- Atomares Rename: `tmp → fsync → Rename → Parent-fsync`

```rust
pub struct WalWriter {
    file: File,
    prev_hash: [u8; 32],
    key_manager: Arc<KeyManager>,
}

impl WalWriter {
    /// Committed atomar: HMAC → Schreiben → fsync. Gibt WalSeqNum zurück.
    pub fn commit(&mut self, entry: WalEntry) -> Result<WalSeqNum>;
    pub fn verify_chain(&self) -> Result<bool>;
}

pub struct WalEntry {
    pub seq: WalSeqNum,
    pub op: WalOperation,
    pub timestamp_us: u64,  // Mikrosekunden (ADR-016: kein SystemTime)
    pub hmac: [u8; 32],     // HMAC über (seq || op || timestamp || prev_hash)
}
```

### §4.2 LSM-Tree mit TenantKeyCodec — ✅ Produktiv

**MemTable-Implementierung (K1-Auflösung):** 16-Shard `BTreeMap<Bytes, Vec<MemTableEntry>>` mit `parking_lot::RwLock`. Kein Wechsel zu `crossbeam-skiplist` — lexikographische Sortiergarantie für SSTable-Flush (INVARIANT-3) und ausreichender Parallelismus bei 8-Thread-Embed-Pipeline rechtfertigen Status quo.

Jeder Schreibpfad: `TenantKeyCodec::encode_*()` vor WAL-Commit.

### §4.3 DiskANN mit persist_delta() — ✅ Produktiv

**Status:** `diskann.rs:510` — `persist_delta()` implementiert: Pending-Buffer + Streaming-Insert (arXiv:2602.21514 §4) + atomares Rename-Muster. Tests grün.

**Rollentrennung (finale Architektur):**
- HNSW: Primärindex für alle Sammlungen < 100.000 Vektoren (read-write)
- DiskANN: Out-of-Core-Index für Sammlungen > 500.000 Vektoren (Feature-Flag: `experimental-diskann`)
- Transition-Trigger: automatischer Build wenn HNSW > 200.000 Vektoren
- Pending-Buffer Auto-Flush: `PENDING_FLUSH_THRESHOLD = 1.000` Einträge

### §4.4 HNSW — ✅ Produktionsreif

**Parameter (K2-Auflösung):** `ef_construction=200`, `ef_search=64` (produktionssicher, Recall@10 ≈ 0.98). `M=16` (zu verifizieren via `grep -n "pub m\|\.m =" hnsw.rs` vor Spec-Finalisierung).

**Invariante:** `INV-HNSW-1`: `ef_construction >= M`. `HnswConfig::validate()` → Err bei Verletzung.

**Tombstone-Management:** `HNSW_REBUILD_DELETION_RATIO = 0.10`. Tombstone-Ratio ist Input für F-01-Thermostat (kein Code-Eingriff in HNSW nötig — nur Metrik-Exposition).

**VETO F-02:** `rebuild_region()` hinter `physio-nucleation` Feature-Flag — hart deaktiviert bis Recall@10-Regressionstest grün (§0.3, §0.4 P0-F02).

### §4.5 CSR-Graph mit Kanten-Provenienz & Cascade-Tombstone

**Verifiziert:** `csr.rs` — `tombstoned_edges` (EdgeId-Bitmap), bi-temporale Kanten (ADR-033), Supersedes (ADR-038). `NodesGuard` in `session_dag.rs:29` — Typ-erzwungene Lock-Reihenfolge (nodes → edges).

**⚠️ P1-LÜCKE: Cascade-Tombstone fehlt.**
Wenn ein Chunk per Supersedes-Event als veraltet markiert wird, werden abhängige CSR-Graph-Kanten **nicht** automatisch tombgestoned. Da PathRAG jetzt produktiv im Suchpfad hängt, ist dies eine reale Quelle für Retrieval-Halluzination über tote Fakten.

```rust
// crates/memfuse-graph/src/provenance.rs [INV-GRAPH-PROV-1]

/// INVARIANTE: JEDE CSR-Kante MUSS einen EdgeProvenance-Eintrag mit WAL-Seq haben.
pub struct EdgeProvenance {
    pub source_chunk_hash: [u8; 32],  // Blake3-Content-Hash des Quell-Chunks
    pub source_doc_id: DocId,
    pub wal_seq: WalSeqNum,
    pub extraction_method: ExtractionMethod,
    pub created_at_tx: TxId,
}

/// [P1 — IMPLEMENTIEREN] Trigger bei jedem Supersedes-Event in QueryBuilder.
/// Implementierung: DocId → Set<EdgeId>-Rückverfolgung, mitgeschrieben in EdgeProvenance.
pub fn cascade_tombstone_superseded_edges(
    csr: &mut CsrGraph,
    superseded_doc_id: DocId,
    effective_at: TxId,
) -> Result<Vec<EdgeId>>;
```

### §4.6 BM25+ IDF-Korrektur — ✅ Gefixt

**Bug A19 behoben** (`memfuse-text/src/bm25.rs:95`):

```rust
// Robertson-Spärck-Jones BM25+: mathematische Garantie IDF ≥ 0 für alle df ∈ [0, N]
let idf = {
    let arg = 1.0 + (n - df + 0.5) / (df + 0.5);
    arg.ln()  // Kein Floor mehr nötig. 1e-6-Artefakt eliminiert.
};
```

---

## §5 Layer 2 — Orchestrierung & Fusion

### §5.1 Retrieval-Pipeline (vollständige Sequenz)

```
Anfrage
  │
  ▼
[Query-Klassifikation] ─── QueryHopClass: SingleHop | MultiHop
  │                          QuerySourceClass: NumericalTemporal | HistoricalFactual | Analytical
  │
  ▼
[Parallele Index-Abfragen]
  ├── HNSW Vektor-Suche
  ├── BM25+ Volltext-Suche (mit DE-Morphologie)
  └── PPR Graph-Traversal
  │
  ▼
[3-Signal-RRF-Fusion] ─── k=60, wissenschaftlich validiert (arXiv:2604.01733)
  │
  ▼
[F-09 Resonanz-Kohärenz-Bonus] ✅ (β=0.15 default)
  │  coherence_bonus IMMER separates Feld (INV-PROV-2)
  │
  ▼
[Temporal-Validity-Post-Filter] ✅ ── nur valid_from ≤ NOW < valid_until
  │
  ▼
[PathRAG Signal] (nur bei MultiHop-Intent) ✅ PathRAGEngine produktiv
  │  Sufficiency-Gate: Konfidenz > 0.6 (verhindert MemGraphRAG-Precision-Problem)
  │
  ▼
[RerankPidController] ✅ ── dynamischer Kandidatenpool, hartes Zeitbudget
  │  Untergrenze: min_rerank_candidates = 100 (arXiv:2604.01733: Recall@5 = 0.888)
  │  Chunk-Injektionsreihenfolge: Lost-in-the-Middle-Mitigation (arXiv:2601.02993)
  │
  ▼
Ergebnis mit ProvenanceRecord (INV-PROV-1 + INV-PROV-2)
```

### §5.2 3-Signal-RRF mit Resonanz-Kohärenz-Bonus (F-09) — ✅ Produktiv

```rust
// INV-PROV-1: sum(contributions.rrf_contribution) ≈ rrf_score (|Δ| < 1e-6)
// INV-PROV-2: coherence_bonus ist IMMER separates Feld, NIE in rrf_score gefaltet

pub struct ProvenanceRecord {
    pub contributions: Vec<SignalContribution>,
    pub rrf_score: f32,              // Reine RRF-Summe OHNE Kohärenz-Bonus
    pub coherence_bonus: Option<f32>, // F-09 — separates Feld (INV-PROV-2)
    pub final_score: f32,             // rrf_score + coherence_bonus.unwrap_or(0.0)
    pub rerank_score: Option<f32>,    // Post-Fusion, nach Reranker
    pub synaptic_score: Option<f32>,  // F-03 (H2)
}

/// Kohärenz-Formel (F-09):
/// C(d) = 1 − (2/(n*(n-1))) * Σ_{i<j} |r_i(d) − r_j(d)|
/// coherence_bonus(d) = β * C(d) * RRF(d), β=0.15 default
```

### §5.3 PID-Reranker — ✅ Produktiv (Commits #1699, #1702)

```rust
pub struct RerankPidController {
    kp: f32, kd: f32, ki: f32,
    target_p95_latency_ms: f32,
    k_pool: usize,    // Aktueller Kandidatenpool
    k_min: usize,     // Absolute Untergrenze: 100 (arXiv:2604.01733)
    k_max: usize,     // Absolute Obergrenze
}

pub struct RerankDeadline {
    deadline_ms: u64,  // Default: 500ms (hartes P11-Latenzbudget)
}
```

### §5.4 PathRAG Engine — ✅ Produktiv

**Wissenschaftliche Basis:** PathRAG (arXiv:2502.14902, AAAI 2026) + ICLR 2026 "When to use Graphs in RAG" (arXiv:2506.05690).

**Kritische Gegenposition:** MemGraphRAG (arXiv:2506.00610): Recall↑ aber Precision 38.5% vs 62.9% → Sufficiency-Gate ist Pflicht (Default: 0.6).

Aktivierung nur bei `QueryHopClass::MultiHop`. PathRAG-Ergebnis fließt als 4. additives Signal in Fusion.

**Korrektheitsproblem (P1):** Ohne `cascade_tombstone_superseded_edges` (§4.5) können PathRAG-Pfade über tombstonierte Chunks führen → Halluzination über tote Fakten. Dieser Fix hat höchste Priorität nach P0.

### §5.5 SleepCycle — H3

**Wissenschaftliche Basis:** LycheeMemory V2 (arXiv:2608.12990) für Turn-Clustering + Zielwerte. SleepGate (arXiv:2603.14517) für Interferenzhorizont. arXiv:2605.17625 Dual-Process-Architektur.

**Phasen:**
- NREM: Segment-Deduplication (cosine > 0.95 → älteren tombstonen), Verdichtung via ContextCompactor, Graph-Kanten-Tombstone Trigger (§4.5)
- REM (F-05): Generative Wissenssynthese für stabile Communities. Jeder Meta-Chunk trägt `abstracts_from`-Kanten zu Quell-Chunks (Halluzinations-Guard-Kompatibilität). Budget-Check: `max_llm_calls_per_cycle` (P12-Kostenschutz).

### §5.6 Freie-Energie-Thermostat (F-01) — ✅ Produktiv

```rust
// Mathematisches Modell:
// T(t) = w1 * tombstone_ratio + w2 * query_rate_inverse  ∈ [0,1]
//   w1=0.6 (Speicherdruck), w2=0.4 (Query-Rate) — konfigurierbar
// half_life_eff = half_life_base * (1 + κ * (1 − T(t))),  κ=2.0
// effective_score = base_score * exp(−(ln2 / half_life_eff) * elapsed)
//
// Semantik: Hohe T → kurze Half-Life → aggressiveres Vergessen
//           Niedrige T → lange Half-Life → längeres Behalten
//
// Inputs: tombstone_ratio (HNSW), query_rate aus TxId-Zähldifferenzen (ADR-016-konform)
```

---

## §6 Layer 3 — Inferenz, Routing & Physio-Selbstregulierung

### §6.1 Conformal Router mit ConfigFingerprint & Abstention — ✅ Produktiv

**Status:** ConfigFingerprint in Router + Calibration + Reranker verdrahtet (HEAD `bb099dc2`).

**Abstention-Pfad:** Bei `calibrator.is_calibrated() == false` → Eskalation an stärkstes verfügbares Modell, kein Raten. Referenz: RACER (arXiv:2603.06616).

**Temperatur-Lock:** Temperaturänderung während Warmup → `invalidate_on_config_change()` + Zähler-Reset.

### §6.2 Lyapunov-Drift-Wächter (F-11) — Status: Verifikation ausstehend

**Zweck:** Proaktiver Drift-Detektor für Input-Verteilungsänderungen (ConfigFingerprint ist reaktiv — erkennt nur Konfigurationsänderungen).

```
Algorithmus:
D_t = KL(N_t || N_baseline)   [N_t = Non-Conformity-Scores im Sliding Window]
λ_t = (1/w) * Σ log|D_{t-i+1} / D_{t-i}|  [diskrete Lyapunov-Schätzung, w=20]
Trigger: λ_t > 0 für ≥ w Fenster → proaktive Re-Kalibrierung
```

**Handlungsbedarf:** Live-Verifikation in HEAD `bb099dc2` via `grep -rn "LyapunovDrift\|lyapunov" crates/memfuse-router/` — Status unklar, ADR ggf. offen.

### §6.3 memfuse-candle — Fundament bereit, Pipeline-Integration ausstehend

**Status HEAD `bb099dc2`:** Crate existiert (488 LOC: GGUF-Loader, Inferenz, Embedding, Model-Registry). **NICHT** importiert in `memfuse-db`, `memfuse-ollama` oder `memfuse-router`.

**Strategische Bedeutung:** Ohne Pipeline-Verdrahtung ist "Sovereign Core" rhetorisch (P7-Verstoß).

**Strategieentscheidung:** Beide Backends über Feature-Flags:
```toml
[features]
candle-native = ["candle-core", "candle-nn", "candle-transformers"]
candle-mistralrs = ["mistralrs"]
cuda = ["candle-core/cuda"]
metal = ["candle-core/metal"]
```

**Kritischer GASP-Zusammenhang:** GASP Post-Hoc-Validator (§7.2) benötigt `log_likelihood()` — nur via `memfuse-candle` verfügbar (Ollama-HTTP hat keine Logit-API). GASP ist deshalb strategisch mit P3-Pipeline-Integration verbunden.

### §6.4 ImportanceClassifier — Bewusst zurückgestellt

**Ist-Zustand:** Commit `f7600262` konsolidiert auf `memfuse-ollama`-LLM-Pfad. Richtige Entscheidung: ohne gelabelten Evaluationsdatensatz besteht Risiko, unkalibriertes Modell als "Verbesserung" zu deployen.

**Freigabe-Kriterium:** Erst nach LongMemEval-CI-Integration (§12 P2). Dann Distillations-Ansatz (bestehende LLM-Scores als schwaches Label-Signal) mit explizitem `calibrated: false`-Fallback-Pfad.

**Ziel-Latenz:** < 100ms P50 (MemRouter-Referenz: 58ms, arXiv:2605.00356).

---

## §7 Layer 4 — Integrations-Grenzschicht

### §7.1 KV-Cache-Bridge — P2 (Sicherheitsschicht zuerst)

**Wissenschaftliche Basis:** KV Packet (arXiv:2604.13226). Performance: TTFT-Reduktion NIAH 19.45×, Multi-Hop 5.81×, FLOPs: 6.5×10⁻⁶ relativ.

**Sicherheitsanforderungen (P9):** arXiv:2510.17098 (MTI-Angriff) + arXiv:2508.09442 (Inversion).

**Implementierungsstrategie (aus Principal-Architect-Review §3.3):**
KvSegment/EncryptedKvLayer/Zeroize-on-Evict als **eigenständiges erstes Increment** — entkoppelt von memfuse-candle-Backend-Integration. `memfuse-candle` existiert bereits, verkürzt Abhängigkeitskette de facto.

**Kritisches Design-Detail (aus Gegenprüfung):** `evict_lru()` darf den Inferenz-Hot-Path **nicht** blockieren. Zeroize läuft auf **dediziertem Eviction-Worker-Thread** (nicht dieselbe synchrone Routine wie `emergency_wipe()`).

```rust
pub struct KvSegment {
    pub doc_id: DocId,
    pub tenant_id: TenantId,
    pub model_fingerprint: ModelFingerprint,  // Q4 ≠ Q8 (P8)
    pub encrypted_layers: Vec<EncryptedKvLayer>, // AES-256-GCM-SIV
    pub created_at_tx: TxId,                  // ADR-016: kein SystemTime
    pub vram_bytes: usize,
    pub rope_offset: u32,  // RoPE-Positions-Offset, bei inject() korrigiert
}

pub trait KvCacheProvider: Send + Sync {
    async fn ensure_cached(&self, tenant_id: TenantId, doc_ids: &[DocId]) -> Result<()>;
    /// None = stale (Modell gewechselt) ODER Tenant-Mismatch → Text-Fallback (P11)
    async fn inject(&self, tenant_id: TenantId, doc_ids: &[DocId],
                    ctx: &mut InferenceContext) -> Result<Option<InjectionResult>>;
    /// Non-blocking: gibt LRU-Kandidaten zurück, Zeroize via Worker-Thread
    async fn evict_lru_nonblocking(&self, target_free_bytes: usize) -> Result<Vec<DocId>>;
    async fn purge_tenant(&self, tenant_id: TenantId) -> Result<usize>;
}
```

Schlüssel-Ableitung: `KeyManager::derive_kv_segment_key(tenant_id, doc_id)` — wiederverwendet memfuse-crypto (kein zweiter Krypto-Stack, P10).

### §7.2 GASP Post-Hoc-Halluzinations-Validator — H2

**Wissenschaftliche Basis:** GASP (arXiv:2607.04223, Juli 2026). AUC 0.73 (Response-Level), 0.67 (Span-Level). Training-frei.

**Algorithmus:** `GroundingSensitivity(s, C_i) = log P(s|C_full) − log P(s|C_full \ C_i)` für jeden Antwortsatz s und Chunk C_i.

**Abhängigkeit:** `LlmTextGenerator::log_likelihood()` — nur via `memfuse-candle` verfügbar. Graceful Degradation bei Ollama-Backend (None).

GASP ist **komplementär** zum präventiven Halluzinations-Guard in `client.rs` (Prompt-Constraint + Zitierpflicht).

### §7.3 MCP JSON-RPC 2.0 — ✅ Produktionsreif

E2E-Test #1613 grün. Prompt-Injection-Detection aktiv. Ausstehend: Vollständige Onboarding-Dokumentation (Claude Desktop, Cursor, VS Code) + ADR für MCP-Schema-Versionierung.

---

## §8 Layer 5 — Evaluation & Benchmarking

### §8.1 LongMemEval & LoCoMo — P2 (dringendste Infrastruktur)

**Begründung (Principal-Architect-Review §3.6):** Bei 76 Commits pro Tag kann das Projekt nicht zuverlässig zwischen "elegant aussehender Verbesserung" und "stiller Qualitätsregression" unterscheiden ohne automatisierten CI-Recall-Benchmark. F-02 wurde trotz Veto gemergt, ohne dass eine Recall-Metrik das auffing — das ist der konkreteste Beleg dafür.

**SOTA 2026 (LycheeMemory V2, arXiv:2608.12990):**
- LoCoMo: 89.22%
- LongMemEval-S: 92.20%

**MemFuse Zielwerte:**

| Benchmark | H3 (Einstieg) | H5 (SOTA-Parität) |
|---|---|---|
| LoCoMo | > 80% | > 89% |
| LongMemEval-S | > 85% | > 92% |

**Task-Typen LongMemEval (arXiv:2410.10813, ICLR 2025):** InformationExtraction, MultiSessionReasoning, KnowledgeUpdate, TemporalReasoning, Abstain (500 Tasks gesamt).

**Akzeptanzkriterium für CI-Integration:** Vollautomatisierter Lauf in CI nach jedem Merge in `main`. Ergebnisse in `memfuse-bench/results/` versioniert. Recall-Regression > 1pp → CI-Red.

---

## §9 Physio-Feature-Katalog (F-01 bis F-11)

Alle physio-Features sind Feature-Flag-geschützt. Defaults sind P1-sicher. Drei-Kriterien-Nachweis erforderlich: (a) geschlossene Formel, (b) bestehende Datenstruktur nutzbar, (c) analogie-unabhängiges Akzeptanzkriterium.

| Feature | Status | Priorität | Naturvorbild | Kernformel / Mechanismus |
|---|---|---|---|---|
| **F-01 Freie-Energie-Thermostat** | ✅ Produktiv | — | Thermodynamik | T(t) = w1·tombstone + w2·query_rate_inv; half_life_eff = base·(1+κ·(1−T)) |
| **F-02 Partieller HNSW-Rebuild** | ⛔ VETO (implementiert, hart deaktiviert) | P0-Test | — | Kein echter Delaunay-Rewire; nur Tombstone-Bereinigung — KEIN Recall-Gewinn ohne Test |
| **F-03 Synaptische Verstärkung** | 📋 H2 | H2 | Hebbian + Stigmergie | w_ij += η·co_activation − δ·w_ij; τ_ij += (1−ρ)τ + Q/path_len |
| **F-04 Immunologisches Kontradiktions-Gedächtnis** | ✅ Produktiv | — | Klonale Selektion | affinity = max_k cos_sim(e, centroid_k); auto-reject > θ_reject=0.9 |
| **F-05 REM-Wissenssynthese** | 📋 H3 | H3 | REM-Schlaf | LLM-Synthese stabiler Communities; jeder Meta-Chunk trägt abstracts_from-Kanten |
| **F-06 Perkolations-Gesundheitsmonitor** | 📋 H2 | H2 | Perkolationstheorie | φ(t) via BFS-Sampling auf MVCC-Snapshot; Trigger bei Unterschreitung kritischer Schwelle |
| **F-07 Replikatordynamik (adaptive RRF-Gewichte)** | ✅ In memfuse-calibration | — | Evolutionäre Spieltheorie | dw_i/dt = w_i·(π_i − π̄); konvergiert zu dominanter Strategie |
| **F-08 PID-Latenz-Homöostat** | ✅ RerankPidController | — | PID-Regelung | u(t) = Kp·e + Ki·∫e + Kd·de/dt; Zielgröße: P95-Latenz |
| **F-09 Resonanz-Kohärenz-Bonus** | ✅ Produktiv (Commit #1698) | — | Konstruktive Interferenz | C(d) = 1−(2/(n(n-1)))·Σ\|r_i−r_j\|; bonus = β·C·RRF |
| **F-10 Osmotischer Cross-Tenant-Austausch** | ⛔ VETO (permanent) | — | — | Bricht DeletionProof + TenantId-Isolation + DSGVO |
| **F-11 Lyapunov-Drift-Wächter** | ⚠️ Status unklar | H1 | Lyapunov-Stabilität | λ_t = (1/w)·Σ log\|D_{t-i+1}/D_{t-i}\|; Trigger: λ_t > 0 |

### F-03: Synaptische Verstärkung (5. Fusionssignal) — H2

**Architektur-Schlüsselentscheidung:** CSR ist Read-Heavy optimiert — direkte Writes im Hot-Path = Contention. Lösung: `SynapticUpdateBuffer` (DashMap, lock-frei) im Hot-Path, asynchroner Flush in Background-Worker (WAL-First, P3-konform).

**Homöostatische Normalisierung:** `Σ_j w_ij ≤ W_max` für jeden Knoten — verhindert Hub-Explosion.

**Akzeptanzkriterium:** Nach 30-Tage-Replay-Simulation: SynapticScore als 5. Signal verbessert RRF-Recall@10 um ≥ 3pp, ohne Hub-Übergewicht (Homöostase-Test).

### F-04: Immunologisches Kontradiktions-Gedächtnis — ✅ Produktiv

```rust
// Antikörper-Population: kompakter Vec<Antibody> (keine eigener ANN-Index — lineare Suche reicht)
// Trigger-Kaskade: Supersedes-Event → cascade_tombstone (§4.5, P1) → ImmunMemory-Reinforcement
pub struct Antibody {
    pub centroid: Vec<f32>,
    pub avidity: f32,               // Steigt mit Wiederholung
    pub entity_pattern: (String, String),
    pub created_at_tx: TxId,
}
// Immunologisches Vergessen: λ_immun = λ_regular / 10 (default)
```

---

## §10 PhysioScheduler & PhysioConfig

### §10.1 PhysioScheduler

Zentraler Hintergrund-Worker in `memfuse-db`, koordiniert alle Physio-Features:

```
Tick-Interval: konfigurierbar (Default: 60s)
├── F-01 Thermostat-Update (tombstone_ratio + query_rate)
├── F-03 SynapticUpdateBuffer.flush_to_csr() (wenn Buffer > threshold)
├── F-06 Perkolations-BFS-Sampling (wenn nicht im Active-Session-Fenster)
├── F-07 Replikatordynamik-Update (adaptive RRF-Gewichte)
├── F-09 Kohärenz-Bonus-Parameter-Adaption (wenn F-07 aktiv)
├── F-11 LyapunovDriftWatcher.update() (Non-Conformity-Scores aus Router)
└── SleepCycle-Trigger (wenn active_agent_sessions() == 0)
```

Alle Scheduler-Aktionen: WAL-Intent vor Arbeit (`write_consolidation_intent()`), abschließend `complete_consolidation_intent()`.

### §10.2 PhysioConfig — Alle Parameter mit Defaults

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysioConfig {
    // F-01 Thermostat
    pub thermostat_enabled: bool,       // Default: true
    pub thermostat_w1: f32,             // Default: 0.6
    pub thermostat_w2: f32,             // Default: 0.4
    pub thermostat_kappa: f32,          // Default: 2.0
    pub thermostat_base_half_life: u64, // Default: 604_800 (7 Tage in Sekunden)

    // F-03 Synaptische Verstärkung
    pub synaptic_enabled: bool,         // Default: false (experimentell)
    pub synaptic_eta: f32,              // Default: 0.01
    pub synaptic_delta: f32,            // Default: 0.001
    pub synaptic_rho: f32,              // Default: 0.05
    pub synaptic_q: f32,                // Default: 1.0
    pub synaptic_alpha: f32,            // Default: 0.5
    pub synaptic_w_max_factor: f32,     // Default: 5.0

    // F-04 Immunologisches Gedächtnis
    pub immune_enabled: bool,           // Default: true
    pub immune_theta_reject: f32,       // Default: 0.9
    pub immune_theta_flag: f32,         // Default: 0.7
    pub immune_beta: f32,               // Default: 0.1
    pub immune_a_max: f32,              // Default: 1.0
    pub immune_lambda_ratio: f32,       // Default: 10.0 (immun 10× langsamer als normal)

    // F-05 REM-SleepCycle
    pub sleep_cycle_enabled: bool,      // Default: false (erfordert LLM)
    pub sleep_episode_threshold: usize, // Default: 50
    pub sleep_rem_enabled: bool,        // Default: false
    pub sleep_max_llm_calls: usize,     // Default: 10 (P12 Kostenschutz)
    pub sleep_stability_cycles: u32,    // Default: 3

    // F-07 Replikatordynamik
    pub replicator_enabled: bool,       // Default: true
    pub replicator_lr: f32,             // Default: 0.05

    // F-09 Kohärenz-Bonus
    pub coherence_bonus_beta: f32,      // Default: 0.15

    // F-11 Lyapunov
    pub lyapunov_enabled: bool,         // Default: true
    pub lyapunov_window_size: usize,    // Default: 20

    // PathRAG
    pub pathrag_enabled: bool,          // Default: true
    pub pathrag_sufficiency_threshold: f64, // Default: 0.6
}
```

**Änderung von `PhysioConfig` erfordert Re-Validierung via `memfuse-bench`. Versionierung analog zu Prompt-Templates (P8-analoge Regel).**

---

## §11 Invarianten-Verzeichnis (normativ)

Alle Invarianten müssen in Tests und Dokumentation durchgesetzt werden:

| ID | Invariante | Testdatei |
|---|---|---|
| **INV-P3-1** | WAL-Commit VOR MemTable-Update. `let _ = dir.sync_all()` ist P3-VIO. | `wal.rs` HMAC-Tests |
| **INV-P8-1** | Fingerprint-Änderung → sofortige Kalibrierungs-Invalidierung. | `calibration::tests` |
| **INV-CAL-1** | `calibrated_probability()` → None wenn Warmup < warmup_required. **KEIN 0.5-Fallback.** | `calibration::tests` |
| **INV-CAL-2** | `invalidate_on_config_change()` setzt Observations auf 0. Kein partielles Übernehmen. | `calibration::tests` |
| **INV-PROV-1** | `sum(contributions.rrf_contribution)` ≈ `rrf_score` (\|Δ\| < 1e-6). | `fusion.rs` tests |
| **INV-PROV-2** | `coherence_bonus` ist IMMER separates Feld. NIE in `rrf_score` gefaltet. | `fusion.rs` tests |
| **INV-TENANT-1** | `TenantId(0)` ist SYSTEM-reserviert. `try_new(0)` → Err. | `domain.rs` tests |
| **INV-TENANT-2** | `scan_prefix()` gibt **ausschließlich** Keys des zugehörigen Tenants zurück. | `tenant_codec.rs` tests |
| **INV-HNSW-1** | `ef_construction >= M`. `HnswConfig::validate()` → Err bei Verletzung. | `hnsw.rs:137-143` |
| **INV-GRAPH-PROV-1** | Jede CSR-Kante hat gültigen `EdgeProvenance`-Eintrag mit WAL-Seq. | Property-Test `memfuse-bench` |
| **INV-DISKANN-1** | `persist_delta()` behält atomares Rename-Muster (Tmp→fsync→Rename→Parent-fsync). | `diskann.rs` tests |
| **INV-DELETION-1** | `DeletionProof::create()` **NUR** nach vollständiger Layer-Bereinigung aller `covered_layers`. | Integration-Test |
| **INV-KV-1** | KV-Cache-Segmente liegen **nie** im Klartext auf persistentem Speicher. | Security-Test |
| **INV-KV-2** | Zeroize-on-Evict läuft auf Worker-Thread, **nie** blockierend im Inferenz-Hot-Path. | Latenz-Test |
| **INV-F02-1** | `physio-nucleation` ist in CI **deaktiviert** bis Recall@10-Regressionstest grün. | CI-Config |

---

## §12 Implementierungsstand & Priorisierte Roadmap

### §12.1 Vollständiger Implementierungsstand (HEAD `bb099dc2`)

| Komponente | Status | Quelle |
|---|---|---|
| TenantId + TenantKeyCodec | ✅ Produktiv | `memfuse-core/src/types/domain.rs` |
| ConfigFingerprint | ✅ Router + Calibration + Reranker | `router/profile.rs`, `calibration/*.rs` |
| DeletionProof | ✅ Produktiv | `memfuse-crypto/src/deletion_proof.rs` |
| memfuse-calibration Crate | ✅ Isotonic + Platt + Replicator | `crates/memfuse-calibration/` |
| PathRAGEngine | ✅ In Graph + DB + QueryBuilder | `memfuse-graph/src/path_rag.rs` |
| memfuse-candle Crate | ✅ Crate, ⚠️ nicht in Pipeline | `crates/memfuse-candle/src/` |
| BM25 IDF-Fix | ✅ Robertson-Spärck-Jones | `memfuse-text/src/bm25.rs:95` |
| DiskANN persist_delta() | ✅ Pending-Buffer + Streaming | `memfuse-index/src/diskann.rs:510` |
| Reranking-Fix | ✅ RerankPidController (übertrifft statisches Fix) | Commits #1699, #1702 |
| F-01 Thermostat | ✅ Produktiv | `memfuse-db/src/thermostat.rs` |
| F-04 ImmunMemory | ✅ Produktiv | `memfuse-graph/src/immune.rs` |
| F-09 Resonanz-Fusion | ✅ Produktiv | Commit #1698 |
| F-07 Replikatordynamik | ✅ In memfuse-calibration | Calibration-Crate |
| F-08 PID-Homöostat | ✅ Als RerankPidController | Commits #1699, #1702 |
| Temporal-Validity-Filter | ✅ Produktiv | `memfuse-db/src/temporal_filter.rs` |
| async_trait → AFIT | ✅ 0 Treffer | Gesamter Workspace |
| WAL v3 HMAC-Chain | ✅ Produktiv | `wal.rs:45-80` |
| HNSW 2-Phasen-CoW-Rebuild | ✅ Produktiv | `hnsw.rs:1693+1812` |
| NodesGuard Lock-Reihenfolge | ✅ Typ-erzwungen | `session_dag.rs:29` |
| PPR damping=0.85 bidirektional | ✅ Produktiv | `ppr.rs:133,343` |
| Label-Propagation deterministisch | ✅ SimpleRng LCG | `community.rs:55` |
| MCP JSON-RPC 2.0 | ✅ E2E-Test grün | `memfuse-mcp/` |

### §12.2 Verbleibende Lücken (priorisiert)

#### Priorität P0 (vor jedem weiteren Merge in betroffenen Bereichen)

| Task | Begründung | Aufwand |
|---|---|---|
| **`VETOES.md` einführen** (maschinenlesbar, neben CONSTITUTION.md) | F-02: Veto morgens, Umsetzung nachmittags — Governance-Lücke bewiesen | 30 Min |
| **CI-Check gegen VETOES.md** (Feature-ID-Matching) | Verhindert Wiederholung des F-02-Falls | 2h |
| **Recall@10-Regressionstest `rebuild_region()`** | `physio-nucleation` bleibt bis dahin CI-disabled | 1 Tag |
| **`memfuse-py` in Cargo.toml aufnehmen** | AGT-PY-d5d2be30-Fix sonst nicht von CI erfasst | 15 Min |

#### Priorität P1 (diese Woche)

| Task | Datei | Aufwand | Verifikation |
|---|---|---|---|
| **Cascade-Tombstone Supersedes→Graph** | `memfuse-graph/src/provenance.rs` | 1 Tag | PathRAG-Integration-Test mit tombstoniertem Chunk |
| **CONSTITUTION.md Kodifizierung P8–P12** | `CONSTITUTION.md` | 3h | PR-Review |

#### Priorität P2 (nächste 2–4 Wochen)

| Task | Begründung | Aufwand |
|---|---|---|
| **LongMemEval-S + LoCoMo CI-Integration** | Dringendste Infrastruktur — Voraussetzung für jedes ML-lastige Feature | 3 Tage |
| **KV-Cache-Bridge Sicherheitsschicht** (`KvSegment`, `EvictionWorker`, Zeroize) | Entkoppelt von Pipeline-Integration — erstes Increment startbar | 2 Wochen |

#### Priorität P3 (mittelfristig)

| Task | Abhängigkeit | Aufwand |
|---|---|---|
| **memfuse-candle → Serving-Pipeline** (Ollama-Ausstieg) | P2 (LongMemEval für Qualitäts-Gate) | 2 Wochen |
| **Lyapunov F-11 Live-Verifikation + ggf. ADR** | — | 1 Tag Verifikation + 1 Woche Impl |
| **F-03 Synaptische Verstärkung** | P2 (LongMemEval für Akzeptanztest) | 2 Wochen |

#### Horizont H3 (Quartal 2)

| Task | Abhängigkeit |
|---|---|
| SleepCycle REM-Phase (F-05) | memfuse-candle Pipeline + LongMemEval |
| KV Packet Adapter-Training + RoPE-Shift | KV-Bridge Sicherheitsschicht |
| F-06 Perkolations-Gesundheitsmonitor | LongMemEval |
| GASP Post-Hoc-Validator | memfuse-candle Pipeline |
| ImportanceEmbeddingClassifier | LongMemEval (Daten + Regression) |
| LongMemEval-S > 85% Parität | Alle H2-Features aktiv |

---

## §13 Definition of Done

Eine Komponente gilt als "Done" wenn **alle** folgenden Kriterien erfüllt sind:

**Code-Qualität:**
- [ ] Zero `#[async_trait]` — native AFIT
- [ ] Kein `unsafe` ohne `// SAFETY: <Beweis>`
- [ ] P3-WAL-First in allen Schreibpfaden (verifiziert via Code-Review)
- [ ] Kein unbegrenztes Warten auf nachgelagerte Operationen (P11)

**Korrektheit:**
- [ ] Alle normativen Invarianten (§11) durch Tests abgedeckt
- [ ] Property-Tests für kritische Invarianten (INV-PROV-1, INV-GRAPH-PROV-1)
- [ ] Integration-Test für INV-DELETION-1

**Kalibrierung:**
- [ ] `ConfigFingerprint` verdrahtet (wenn Kalibrierung involviert)
- [ ] `calibrated: false`-Fallback-Pfad explizit (kein stiller 0.5-Fallback)
- [ ] ECE-Test für jede neue `IsotonicCalibrator`-Instanz

**DAG & Abhängigkeiten:**
- [ ] `cargo xtask check-dag` grün
- [ ] Kein zirkulärer Crate-Import

**Benchmarking (nach LongMemEval-CI-Einführung):**
- [ ] Kein Recall@K-Rückgang > 1pp gegenüber vorherigem Stand
- [ ] Latenz-Akzeptanztest für Hot-Path-Komponenten

**Governance:**
- [ ] ADR in `DECISIONS.md` (für jede architektonische Entscheidung)
- [ ] Keine Verletzung von `VETOES.md`-Einträgen
- [ ] P7-konforme Latenz/Recall-Aussagen (reproduzierbare Messung)

---

## §14 Governance & Prozessmodell

### §14.1 Veto-Enforcement (neu — P0-Maßnahme)

**`VETOES.md` Struktur:**
```markdown
# MemFuse — Architektur-Vetos

## VETO-F02: Partieller HNSW-Rebuild
- Feature-IDs: F-02, nucleation-trigger, rebuild_region, physio-nucleation
- Begründung: ...
- Ersatz: 2-Phasen-CoW-Rebuild (hnsw.rs:1693)
- Freigabe-Kriterium: Recall@10-Regressionstest vor/nach rebuild_region()

## VETO-F10: Cross-Tenant-Wissensaustausch  
- Feature-IDs: F-10, osmotic, cross-tenant-knowledge
- Begründung: DSGVO Art. 17, DeletionProof-Beweisbarkeit
- Keine Alternative
```

CI-Check: `grep`-basiertes Stichwort-Matching gegen Feature-IDs. Bei Treffer in neuem Commit → CI-Red, ADR-Review erforderlich.

### §14.2 Agentensteuerung

Alle Agenten-Sessions erhalten beim Start:
1. `CONSTITUTION.md` (Architekturprinzipien P1–P12)
2. `VETOES.md` (maschinenlesbar, Feature-IDs)
3. `.jules/COMMON_LLM_ERRORS.md` (Code-Anti-Patterns)
4. `DECISIONS.md` (aktuelle ADR-Liste)

### §14.3 Sprintstruktur

- **P0** (vor allem): VETOES.md, CI-Check, F02-Recall-Test, memfuse-py (1–2 Tage)
- **P1** (Woche 1): Cascade-Tombstone, CONSTITUTION-Kodifizierung (3–5 Tage)
- **P2** (Woche 2–4): LongMemEval-CI, KV-Bridge-Sicherheitsschicht (2–3 Wochen)
- **P3** (Monat 2): memfuse-candle Pipeline, F-11-Verifikation, F-03 Synaptic
- **H3** (Quartal 2): REM-SleepCycle, GASP, ImportanceClassifier, KV Packet Training

---

## Anhang A: Wettbewerbspositionierung

### Was MemFuse hat, kein Wettbewerber hat

1. WAL v3 mit HMAC-Chain (kryptographische WAL-Integrität)
2. KV-Cache-Bridge mit Positions-Unabhängigkeit (KV Packet), verschlüsselt + mandantenisoliert
3. Conformal Router mit ConfigFingerprint-Zwang + Lyapunov-Drift-Wächter (F-11)
4. DeletionProof mit maschinenlesbarer Nicht-Abdeckungs-Deklaration (DSGVO Art. 17)
5. Physikalisch-biologisches Selbstmanagement als kohärentes System (F-01, F-04, F-07, F-08, F-09)
6. Deutsche Morphologie (Kompositum-Dekomposition) in BM25+
7. PathRAG + Sufficiency-Gate (verhindert MemGraphRAG-Precision-Kollaps)
8. Session-DAG mit Typ-erzwungener Lock-Reihenfolge (NodesGuard — keine Deadlocks)
9. GASP Post-Hoc-Halluzinations-Validator (training-frei, arXiv:2607.04223)

### Was Wettbewerber haben, MemFuse noch nicht hat (und Strategie)

| Wettbewerber-Feature | Wettbewerber | MemFuse-Antwort |
|---|---|---|
| Edge-Vektoren (2. HNSW für Kanten) | MinnsDB | F-03 Synaptische Verstärkung (H2) — ohne zweiten Index, via Nutzungsdynamik |
| Statischer Temporal Decay | YantrikDB | F-01 Thermostat — systemzustandsabhängig, dynamischer |
| Autonomer `think()`-Consolidation-Pass | YantrikDB | SleepCycle NREM+REM (H3) — generative Synthese übertrifft reaktives Scanning |
| AST-aware Code-Chunking | BrainPalace | H5 (niedrige Priorität) |

---

## Anhang B: Verworfene Features (permanent)

| Feature | Grund | Alternative |
|---|---|---|
| **F-02 Partieller HNSW-Rebuild** | Kein echter Delaunay-Rewire möglich; implementierter Code entfernt nur Tombstone-Refs ohne Ersatzkanten → Grad-Verlust ohne Recall-Verbesserung. `RwLock`-Contention unlösbar bei vollem Rebuild. | 2-Phasen-CoW-Rebuild (hnsw.rs:1693) |
| **F-10 Osmotischer Cross-Tenant-Austausch** | Bricht TenantId-Isolation + DeletionProof + KV-Cache-Sicherheit. DSGVO Art. 17 — kryptographische Löschgarantien über fließende Mandantengrenzen mathematisch nicht beweisbar. | Keine — Isolation ist absolut |
| Quanten-Superpositions-Bewertung | Kein analogie-unabhängiges Akzeptanzkriterium erfüllbar — reimplementiert nur verzögerte Score-Aggregation | — |
| Lotka-Volterra literal | Replikatordynamik (F-07) liefert identische Homöostase mit stärkeren Konvergenzgarantien | F-07 |
| Genetische Algorithmen für Meta-Hyperparameter | F-07+F-08 lösen dasselbe Problem mit stärkeren theoretischen Garantien | F-07, F-08 |

---

## Anhang C: ArXiv-Paper-Verzeichnis

### Tier 1 — Unmittelbar architektur-relevant (ADR erforderlich)

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| **2608.01460** | Conformalized LLMs under Configuration Shift | Router ConfigFingerprint, P8 — **kritischste Quelle** |
| **2604.13226** | KV Packet: Recomputation-Free Context-Independent KV Caching | KV-Cache-Bridge (§7.1) |
| **2510.17098** | Can Transformer Memory Be Corrupted? | KV-Bridge-Sicherheit (P9) |
| **2508.09442** | Privacy Risks of KV-cache in LLM Inference | KV-Bridge-Sicherheit (P9) |
| **2605.00356** | MemRouter: Memory-as-Embedding Routing | ImportanceClassifier (§6.4) |
| **2608.12990** | LycheeMemory V2 | SleepCycle-Zielwerte (§5.5, §8.1) — **aktuellster SOTA** |
| **2506.00610** | MemGraphRAG (Recall vs. Precision) | PathRAG Sufficiency-Gate (§5.4) |
| **2506.05690** | When to use Graphs in RAG (ICLR 2026) | PathRAG-Trigger-Logik (§5.4) |
| **2505.16831** | Unlearning Isn't Deletion | DeletionProof-Grenze (§3.5) |
| **2502.14902** | PathRAG (AAAI 2026) | PathRAGEngine-Basis (§5.4) |
| **2607.04223** | GASP: Grounding-Aware Sensitivity by Perturbation | Post-Hoc-Validator (§7.2) |

### Tier 2 — Wissenschaftliche Validierung

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| 2603.14517 | SleepGate | SleepCycle-Interferenzhorizont (§5.5) |
| 2605.17625 | Episodic-Semantic Memory Architecture | SleepCycle Dual-Process (§5.5) |
| 2604.01733 | From BM25 to Corrective RAG (T2-RAGBench) | RRF-Konfiguration, Rerank-Fenster (§5.2) |
| 2605.18796 | UCCI | Conformal Router, ECE-Ziel (§3.3) |
| 2603.06616 | RACER | Abstention-Pfad (§6.1) |
| 2601.02993 | Stable-RAG | Chunk-Injektionsreihenfolge (§5.2) |
| 2603.14828 | Robust Multi-Hop GraphRAG | Graph-Provenienz-Pflicht (§4.5) |
| 2603.15033 | MUNKEY | DeletionProof-Stützung (§3.5) |
| 2602.21514 | I/O Optimizations for Graph-Based ANN | DiskANN Streaming-Insert (§4.3) |
| 2410.10813 | LongMemEval (ICLR 2025) | Benchmark-Integration (§8.1) |
| 2605.00356 | MemRouter | ImportanceClassifier-Referenz (§6.4) |

### Tier 3 — Kritische Gegenposition

| ArXiv-ID | Titel | MemFuse-Risiko |
|---|---|---|
| 2604.09666 | Do We Still Need GraphRAG? | PathRAG-Scope-Beschränkung (§5.4) |
| 2603.19664 | The Residual Stream Is All You Need? | KV-Cache-Fallback-Design (§7.1) |

---

## Anhang D: Technische Schulden (Stand v5.0)

| Schuld | Impact | Horizont |
|---|---|---|
| Cascade-Tombstone Supersedes→Graph fehlt | PathRAG-Halluzination über tote Fakten | P1 |
| `memfuse-py` nicht in Cargo.toml | Sicherheits-Fix nicht CI-erfasst | P0 (15 Min) |
| LongMemEval-CI fehlt | Keine Recall-Regressions-Baseline bei 76 Commits/Tag | P2 |
| memfuse-candle nicht in Pipeline | Sovereign Core = Marketing ohne Code-Nachweis (P7) | P3 |
| F-11 Lyapunov — Status unklar | Proaktiver Drift-Schutz möglicherweise nicht aktiv | H1 |
| F-03 Synaptisch fehlt | Kein 5. Fusionssignal; MinnsDB-Lücke offen | P3 |
| Checkpoint: 3 Abstraktionen → 1 Fassade | Kognitive Redundanz, Sync-Risiko | H3 |
| HNSW M-Parameter nicht live-verifiziert | Spec sagt M=16, Code-Verifizierung ausstehend | P1 (grep) |
| Benchmark: synthetischer Korpus (9 Docs) | Statistisch bedeutungslos | P2 (LongMemEval ersetzt) |
| `unsafe` in Tests ohne SAFETY-Kommentare | P2-Verletzung | H3 |
| ImportanceClassifier nicht trainiert | LLM-Hot-Path-Latenz 970ms vs. Ziel 58ms | post-LongMemEval |
| GASP nur mit memfuse-candle verfügbar | Post-Hoc-Validierung bei Ollama-Nutzern nicht aktiv | H3 |

---

*Dieses Dokument ist die einzige normative Wahrheitsquelle für MemFuse-Architektur, -Features und -Roadmap (v5.0 — „Sovereign Synthesis"). Es ersetzt vollständig v4.0 (memfuse_spec.md), v4.1 (memfuse_architektur_praezisierung.md) und den Principal-Architect-Review (2026-09-07). Alle Widersprüche zwischen den drei Quell-Dokumenten sind in §0.2 normativ aufgelöst und begründet. Nächste Überarbeitung: nach Abschluss P0-Governance-Maßnahmen und P1-Cascade-Tombstone-Fix, sobald LongMemEval-CI-Baseline etabliert ist.*

*HEAD-Referenz: `bb099dc2` · 07. September 2026 · ~119.700 LOC · 17 Crates*
