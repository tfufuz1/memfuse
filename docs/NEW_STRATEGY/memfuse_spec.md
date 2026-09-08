# MemFuse — Mikrofeingranulare Geschäftslogik-Spezifikation
## Gesamtspezifikation aller Komponenten · v4.0 · Goldstandard

> **Dokument-Typ:** Normative, vollständige Schnittstellenspezifikation der gesamten Geschäftslogik des finalen MemFuse-Produkts  
> **Version:** 4.0 (Galaktische Synthese — alle Vorgänger vereint)  
> **Stand:** 07. September 2026  
> **Konsolidiert aus:** spec_v21.md · Physis-der-Erinnerung PRD v1 · memfuse_master_spezifikation_v3 · MemFuse_vs_Competitors_Detailed · ArXiv-Forschungsberichte v1+v2 · Direktem Code-Audit HEAD `36ad007a` (101.011 LOC, 15 Crates, 62 ADRs)  
> **Autor-Rolle:** Principal Senior Rust Architect, Embedded AI-Infrastruktur  
> **Syntheseprinzip:** Jede Aussage ist (a) code-verifiziert mit Datei:Zeile, oder (b) arXiv-belegt mit ID, oder (c) aus mathematisch geschlossenem PRD-Feature mit Drei-Kriterien-Nachweis. Keine Aussage ohne einen dieser drei Belege.

---

## Inhaltsverzeichnis

- **§0** Methodische Grundlagen & Prioritäten
- **§1** Produktvision, Säulen & Architekturprinzipien (P1–P12)
- **§2** Crate-Topologie — Vollständige Ziel-Architektur
- **§3** Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie
- **§4** Layer 1 — Storage-Primitiven
- **§5** Layer 2 — Orchestrierung & Fusion
- **§6** Layer 3 — Inferenz, Routing & Physio-Selbstregulierung
- **§7** Layer 4 — Integrations-Grenzschicht (KV-Bridge, MCP, Tauri)
- **§8** Layer 5 — Evaluation & Benchmarking
- **§9** Physio-Feature-Katalog (F-01 bis F-11, vollständig)
- **§10** PhysioScheduler & PhysioConfig — Unified Background Engine
- **§11** Wettbewerbspositionierung (technische Tiefe)
- **§12** Roadmap mit Sprint-Struktur & Abhängigkeitsgraph
- **§13** Definition of Done (alle Dimensionen)
- **§14** Vollständiger ADR-Backlog
- **§15** Governance & Prozessmodell
- Anhang A: Technische Schulden (priorisiert)
- Anhang B: Verworfene Features (mit Begründung)
- Anhang C: ArXiv-Paper-Verzeichnis (Tier 1–3)
- Anhang D: PhysioConfig-Referenz (alle Parameter mit Defaults)
- Anhang E: Wettbewerber-Featureδ-Analyse

---

## §0 Methodische Grundlagen & Prioritäten

### §0.1 Hierarchie der Quellen

Wo Quellen sich widersprechen, gilt diese unveränderliche Rangfolge:

1. **Direkter Code-Befund** (HEAD `36ad007a`) schlägt jede Spezifikation
2. **Jüngeres Konferenz-Paper** (Datum + "akzeptiert") schlägt gleichdatiertes Preprint
3. **Jüngeres Preprint** schlägt älteres Preprint
4. **Vorige Spezifikationsversion** wird explizit widerlegt, nie stillschweigend überschrieben

### §0.2 Veto-Entscheidungen (unveränderlich)

Zwei Features aus dem Physik-PRD erhalten permanentes Architektur-Veto:

**VETO F-02 (Nukleations-Trigger / Partieller HNSW-Rebuild):** HNSW ist ein global verschränkter Graph. Partielle Rebuilds einer Teilregion zerstören Delaunay-ähnliche Nachbarschaftsbeziehungen zu Randknoten. Greedy-Search würde in lokale Minima laufen → Recall-Kollaps. Unter `RwLock`-Bedingungen in Rust ist partieller Graph-Rebuild ein Lock-Contention-Albtraum. **Ersatz:** 2-Phasen-CoW-Rebuild (ADR-061, bereits produktiv) + F-01-Thermostat als präventives Signal.

**VETO F-10 (Osmotischer Cross-Tenant-Wissensaustausch):** Bricht fundamental die TenantId-Isolationsgarantie (§3.4), die KV-Cache-Bridge-Sicherheitsschicht (§7.1) und die DeletionProof-Korrektheit (§3.5). Kryptographische Löschgarantien sind über verschwimmende Mandantengrenzen mathematisch nicht beweisbar. Der `DeletionProof`-Mechanismus ist legal-relevant (DSGVO Art. 17) — ein Feature, das seine Beweisbarkeit zerstört, ist ein Compliance-Risiko, kein Feature. **Keine Alternative empfohlen** — Mandantenisolation ist absolut.

### §0.3 Sofort-Prioritäten (vor allem anderen)

Folgende fünf Punkte müssen vor jeder neuen Feature-Arbeit abgeschlossen sein:

| Priorität | Maßnahme | Begründung |
|---|---|---|
| **P-SOFORT-1** | ADR für P8/P9/P10/P11/P12-Kodifizierung in CONSTITUTION.md | Voraussetzung für alle Folgearbeiten |
| **P-SOFORT-2** | Router-ConfigFingerprint (§6.12) | arXiv:2608.01460 = Coverage-Kollaps ohne dies; „SOFORT"-Einstufung |
| **P-SOFORT-3** | Reranking-Kandidatenfenster-Fix (§5.1.3) | `k*3=30` Kandidaten → Recall@5 = 0.458 (nahe wertlos) |
| **P-SOFORT-4** | memfuse-calibration Grundgerüst (§3.3) | Blockiert ImportanceClassifier, Router-Migration, Reranker-Fix |
| **P-SOFORT-5** | DiskANN `persist_delta()` (§4.3) | Lifecycle-Lücke: kein inkrementeller Update-Pfad |

---

## §1 Produktvision, Säulen & Architekturprinzipien

### §1.1 Kernaussage

**MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten und wissensintensive Einzelanwender — die Erinnerung nicht nur speichert, sondern konsolidiert, kalibriert, ihre eigene Löschung beweist, Widersprüche immunologisch abwehrt und sich nach physikalisch-biologischen Prinzipien selbst reguliert.**

### §1.2 Fünf Produktsäulen (nicht verhandelbar)

**Säule I — Datenhoheit (Sovereign Core):** Daten und Inferenz laufen vollständig auf Nutzerhardware. Ohne Cloud-Abhängigkeit für den Kernbetrieb. Beweis: `memfuse-candle` (§7-neu) macht dies technisch wahr, nicht nur rhetorisch.

**Säule II — Belegbare Korrektheit:** Jedes Suchergebnis trägt eine nachvollziehbare Herkunftskette (INV-PROV-1: `sum(contributions) ≈ rrf_score`, INV-PROV-2: `coherence_bonus` explizit im `ProvenanceRecord`). Jeder Schreibvorgang ist WAL-first crash-sicher.

**Säule III — Hybride Retrieval-Qualität:** 3-Signal-RRF (Vektor + BM25 + Graph) + Kohärenz-Bonus (F-09) + Synaptisches Signal (F-03) + optionaler Cross-Encoder-Reranker. Outcome-kalibriert, nicht statisch-geraten.

**Säule IV — Gehärtete Kalibrierung & Cache-Sicherheit:** Config-Fingerprint-Zwang (P8). Kein Klartext-Sensitivspeicher (P9). Proaktiver Lyapunov-Drift-Wächter (F-11). Unified Calibration Primitive (`memfuse-calibration`).

**Säule V — Physikalisch kohärentes Selbstmanagement:** Adaptiver Verfall (F-01), Synaptische Verstärkung (F-03), Immunologische Widerspruchsprävention (F-04), REM-Wissenssynthese (F-05), Perkolations-Gesundheitsmonitor (F-06), Adaptive Fusionsgewichte (F-07), PID-Latenz-Homöostat (F-08), Resonanz-Kohärenz-Bonus (F-09), Lyapunov-Stabilitätswächter (F-11). Alle features-flagged, P1-safe Defaults.

### §1.3 Architekturprinzipien P1–P12

**P1 — DAG-Integrität:** `cargo xtask check-dag` ist CI-Gate. Kein Fachcode in Layer ≥N mit Wissen über Layer >N. Kein Merge ohne grünen DAG-Check.

**P2 — Zero-Panic-Doctrine:** `unsafe` ausschließlich in `distance.rs` (SIMD), `diskann.rs` + `persistence.rs` (Mmap). Jedes `unsafe`-Block trägt `// SAFETY: <Beweis>`. Libraries dürfen Host-Prozess nicht crashen.

**P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. `let _ = dir.sync_all()` ist ein Verstoß (P3-VIO). `fsync` nach jedem WAL-Eintrag vor MemTable-Update.

**P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` (beide in `memfuse-core`) sind die einzigen LLM-Abstraktionsgrenzen. Kein Fachcode in Layer 2–4 mit backend-spezifischem Wissen.

**P5 — Kein Cloud-Zwang:** Jede Komponente, deren einziger Betriebspfad eine externe Netzwerkabhängigkeit ist, benötigt ADR-dokumentierte Ausnahme. Ollama ist Standardpfad, kein Pflichtpfad.

**P6 — Eine Quelle für Architekturentscheidungen:** Ausschließlich `DECISIONS.md`. Keine parallelen Architektur-Dokumente ohne Rückverweis und ADR.

**P7 — Marketing-Aussagen sind an Code-Nachweise gebunden:** Jede quantitative Aussage (Latenz, Recall, Fehlerrate) benötigt eigene, reproduzierbare Messung in `memfuse-bench` oder explizite "fremdreferenziert, an MemFuse nicht validiert"-Kennzeichnung.

**P8 — Kalibrierungs-Integrität [v2.0]:** Jede Änderung an `prompt_template_hash`, `temperature_bits` oder `quantization` invalidiert automatisch und unmittelbar alle Kalibrierungsstatistiken (Router, Reranker, ImportanceScore). Warmup-Fenster darf nach solcher Änderung nicht übersprungen werden. Implementierung: `IsotonicCalibrator::invalidate_on_config_change()` in `memfuse-calibration`. Begründung: arXiv:2608.01460.

**P9 — Kein Klartext-Sensitivspeicher [v2.0]:** Tensor-Zustände aus Nutzerdaten (KV-Cache-Segmente) liegen zu keinem Zeitpunkt unverschlüsselt auf persistentem oder auslagerbarem Speicher. Zeroize-on-Evict ist Pflicht, nicht Option. Begründung: arXiv:2510.17098 (MTI-Angriff) + arXiv:2508.09442 (Inversion).

**P10 — Reuse-vor-Neubau [v3.0]:** Vor jedem neuen §-Arbeitspaket ist ein expliziter Wiederverwendungs-Check durchzuführen. Befund dokumentiert im PR-Template. Bekannte Kandidaten: `score_batch()` (reranker.rs:195), `tombstoned_edges` (csr.rs), `persist_calibration_state` (router.rs).

**P11 — Latenzbudget-Pflicht für Hot-Path [v3.0]:** Jede Operation im Retrieval- oder Ingestion-Hot-Path benötigt explizites Latenzbudget mit hartem Deadline-Abbruchpfad. Kein unbeschränktes Warten auf nachgelagerte Operationen. Begründung: Reranker-Latenzrisiko (Gegenprüfung §1) + Zeroize-Hot-Path-Blockierung (D4).

**P12 — Physio-Feature-Default-Unsichtbarkeit [v3.0]:** Kein physio-inspiriertes Feature erzeugt im Zero-IT-Setup-Default sichtbares, erklärungsbedürftiges Verhalten. Alle `physio-*`-Features sind per Feature-Flag deaktivierbar. Alle Parameter haben P1-sichere Defaults, die 95% der Nutzer nie anpassen müssen. Zentrales `PhysioConfig`-Struct in `memfuse-core` (§10.2).

---

## §2 Crate-Topologie — Vollständige Ziel-Architektur

```
Layer 0 — Fundament (kein I/O, keine externen Abhängigkeiten)
│
├── memfuse-core          Traits, Typen, Fehler-Hierarchie, DAG-Guard
│   ├── types/            TxId, DocId, TenantId [NEU], CollectionId
│   │                     ConfigFingerprint [NEU], ModelFingerprint [NEU]
│   ├── traits/           LlmTextGenerator, TextEmbeddingEngine
│   │                     StorageEngine, VectorIndex, TextIndex
│   │                     GraphIndex, CheckpointCoordinator
│   ├── error.rs          Vollständige Fehler-Hierarchie (thiserror)
│   ├── tx_buffer.rs      MVCC-TxBuffer
│   ├── physio.rs         PhysioConfig [NEU] — alle Physio-Parameter
│   └── seq_log.rs        Sequenz-Monotonie-Guard (ADR-016)
│
├── memfuse-crypto        Kryptographie-Primitive (keine Netz-I/O)
│   ├── crypto.rs         AES-256-GCM-SIV (RFC 8452), HKDF, KeyManager
│   ├── deletion_proof.rs DeletionProof [NEU] + DeletionLayer-Enum
│   ├── kv_cipher.rs      KvSegmentCipher [NEU] — wiederverwendet KeyManager
│   └── hmac_chain.rs     WAL-HMAC-Chain-Verifizierung
│
└── memfuse-calibration   [NEU — Layer 0/1] Unified Calibration Primitive
    └── lib.rs            IsotonicCalibrator, PlattScaler
                          invalidate_on_config_change()

Layer 1 — Storage-Primitiven (I/O, kein LLM)
│
├── memfuse-store         LSM-Tree, WAL v3, SSTable, Mmap
│   ├── wal.rs            HMAC-Chain-WAL v3, Atomic-Commit
│   ├── memtable.rs       SkipList-basiert, Thread-safe
│   ├── sstable.rs        Bloom-Filter, CRC32-Verifikation
│   ├── compaction.rs     Background-Compaction, Tombstone-Tracking
│   ├── mmap.rs           Mmap-backed Reads, Sector-aligned
│   └── tenant_codec.rs   TenantKeyCodec [NEU] — Prefix-Encoding
│
├── memfuse-index         Vektorindizes
│   ├── hnsw.rs           HNSW (M=16, ef_construction=32, Diversity-Heuristik)
│   ├── diskann.rs        DiskANN + persist_delta() [NEU]
│   ├── distance.rs       SIMD AVX2/NEON (unsafe, ADR-017)
│   ├── quantize.rs       Q4/Q8 Scalar Quantization
│   └── persistence.rs    Binary-Format, Mmap-Load (unsafe, ADR-017)
│
├── memfuse-text          Volltext-Retrieval
│   ├── bm25.rs           BM25 mit IDF-Smoothing
│   ├── tokenizer.rs      DE-Morphologie (Kompositum-Dekomposition)
│   └── ngram.rs          N-Gramm-Generierung
│
├── memfuse-graph         Graph-Datenstrukturen
│   ├── csr.rs            CSR-Graph, tombstoned_edges
│   ├── session_dag.rs    SessionBranchTree, NodesGuard (Lock-Order-Erzwingung)
│   ├── ppr.rs            Personalized PageRank
│   ├── community.rs      Label-Propagation (ADR-027, deterministisch)
│   ├── path_rag.rs       PathRAGEngine [NEU] + Query-Klassifikator + Sufficiency-Gate
│   ├── immune.rs         ImmunMemory [NEU — F-04] Antikörper-Register
│   └── provenance.rs     Kanten-Provenienz-Invariante [NEU] WAL-Bindung
│
├── memfuse-checkpoint    Checkpoint-Management (konsolidiert)
│   └── lib.rs            CheckpointStore (EINE Fassade, ADR-refactor)
│
└── memfuse-embed         Embedding & Klassifikation
    ├── embedder.rs       ONNX-Embedding (Arc<Mutex<Session>>)
    ├── reranker.rs       CrossEncoderReranker (Platt-kalibriert via memfuse-calibration)
    └── importance_classifier.rs ImportanceClassifier [NEU — MemRouter-Prinzip]

Layer 2 — Orchestrierung
│
└── memfuse-db            Geschäftslogik-Orchestrierung
    ├── fusion.rs         3-Signal-RRF + Resonanz-Kohärenz-Bonus (F-09)
    ├── collection/       CollectionEngine, QueryBuilder, SearchEngine
    ├── context.rs        DualProcessMemory (Episodisch + Semantisch)
    ├── context_compaction.rs ContextCompactor (NREM-Phase)
    ├── sleep_cycle.rs    SleepCycleScheduler [NEU] NREM+REM-Phase (F-05)
    ├── homeostat.rs      PID-Regler für Retrieval-Homöostase [NEU — F-08]
    ├── thermostat.rs     Freie-Energie-Thermostat [NEU — F-01]
    ├── replicator.rs     Adaptive RRF-Gewichte [NEU — F-07]
    ├── multistep.rs      MultiStepEngine
    ├── reaper.rs         TTL-basierte Eviction
    ├── transaction.rs    MVCC-Transaktions-Management
    └── tenant_codec.rs   TenantKeyCodec-Integration

Layer 3 — Inferenz & Routing
│
├── memfuse-agent         Workflow-Engine
│   ├── engine.rs         AgentWorkflowEngine
│   ├── dlq.rs            Dead-Letter-Queue
│   ├── step.rs           AgentStep-Definitionen
│   └── audit.rs          Audit-Trail
│
├── memfuse-router        Conformal Router
│   ├── router.rs         RouterEngine, RoutingDecision, ConfidenceMetrics
│   ├── profile.rs        SlmProfile + ConfigFingerprint [ERWEITERT]
│   ├── dispatch.rs       Dispatch-Logik, Abstention-Pfad [NEU]
│   └── lyapunov.rs       Lyapunov-Drift-Wächter [NEU — F-11]
│
├── memfuse-ollama        Ollama-Backend
│   ├── client.rs         LlmTextGenerator-Impl, Halluzinations-Guard
│   └── importance.rs     [DEPRECATED im Hot-Path] → ImportanceClassifier
│
├── memfuse-candle        [NEU] Pure-Rust-GGUF-Backend
│   ├── lib.rs            CandleLlmClient, CandleEmbedClient
│   ├── gguf_loader.rs    GGUF-Parser (candelabra-inspiriert)
│   ├── inference.rs      LlmTextGenerator-Impl (native async fn, kein async_trait)
│   ├── embedding.rs      TextEmbeddingEngine-Impl
│   └── model_registry.rs ModelFingerprint (SHA-256 + Quantisierungsgrad)
│
└── memfuse-py            PyO3-Bindings (eigener Workspace, panic=unwind)

Layer 4 — Integrations-Grenzschicht
│
├── memfuse-kv-bridge     [NEU] KV Packet KV-Cache-Bridge
│   ├── lib.rs            KvCacheProvider-Trait, KvSegment
│   ├── kv_packet.rs      KV Packet Adapter-Training + RoPE-Shift
│   ├── security.rs       VRAM-Verschlüsselung, Zeroize-on-Evict
│   └── tenant_isolation.rs Cross-Tenant-Cache-Isolation
│
├── memfuse-mcp           MCP JSON-RPC 2.0 stdio
│   ├── protocol.rs       JSON-RPC-Handler
│   ├── prompt_injection.rs Injection-Detection
│   └── sandbox.rs        Sandbox-Isolation
│
└── memfuse-tauri         Desktop-UI
    ├── commands/         Tauri-Commands (search, ingest, session_dag, …)
    └── session_dag.rs    Branch-UI-Commands (bereits implementiert)

Layer 5 — Evaluation
└── memfuse-bench         Benchmark-Harness
    ├── long_mem_eval.rs  LongMemEval-S Integration [NEU]
    ├── locomo.rs         LoCoMo Integration [NEU]
    └── metrics.rs        Recall@K, MRR, BenchmarkMetrics
```

---

## §3 Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie

### §3.1 Kern-Typen (`memfuse-core/src/types/`)

```rust
// memfuse-core/src/types/domain.rs — ERWEITERT

/// Unveränderliche primäre Schlüsseltypen.
/// Alle Copy + Hash + Eq + Serialize/Deserialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocId(pub u64);

/// [NEU v3.0] — Mandanten-Identifikator.
/// MUSS in memfuse-core (Layer 0) definiert sein, NICHT in memfuse-store.
/// Grund: TenantId ist Abhängigkeit für KV-Cache-Isolation (Layer 4),
/// die ohne zyklische Crate-Abhängigkeit (P1-Verletzung) nicht möglich wäre,
/// wenn TenantId erst in Layer 1 definiert würde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub u64);

impl TenantId {
    pub const SYSTEM: TenantId = TenantId(0);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectionId(pub String);

/// [NEU v3.0] — Konfigurations-Fingerabdruck für P8-Kalibrierungs-Integrität.
/// Jede Änderung an diesem Struct invalidiert alle Kalibrierungsstatistiken.
/// Wird von RouterEngine, CrossEncoderReranker und ImportanceClassifier genutzt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigFingerprint {
    /// LLM-Modell-ID (z. B. "llama-3.2-3b-instruct")
    pub model_id: String,
    /// Quantisierungsgrad: Q4_K_M, Q8_0, F16 — Teil des Fingerabdrucks!
    /// Q4 ≠ Q8 bei identischem model_id (arXiv:2608.01460-Konsequenz)
    pub quantization: Option<String>,
    /// Blake3-Hash des Prompt-Template-Textes — jede Textänderung invalide
    pub prompt_template_hash: [u8; 32],
    /// Sampling-Temperatur als Bit-Repräsentation für Hash/Eq-Fähigkeit
    pub temperature_bits: u32, // f32::to_bits()
}

impl ConfigFingerprint {
    pub fn temperature(&self) -> f32 {
        f32::from_bits(self.temperature_bits)
    }
    
    pub fn set_temperature(&mut self, t: f32) {
        self.temperature_bits = t.to_bits();
    }
}

/// [NEU v3.0] — Modell-Fingerabdruck für KV-Cache-Invalidierung.
/// SHA-256 des Modellgewichts-Blobs UND des Quantisierungsgrades.
/// Ein Wechsel von Q4 → Q8 bei gleichem Modell muss verschiedene Fingerabdrücke erzeugen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelFingerprint {
    /// SHA-256(Gewichts-Blob || Quantisierungs-String)
    pub hash: [u8; 32],
    pub model_id: String,
    pub quantization: String,
}
```

### §3.2 Kern-Traits (`memfuse-core/src/traits/`)

```rust
// memfuse-core/src/traits/inference.rs

/// Primäre Abstraktion für Text-Generierung.
/// KEIN #[async_trait] — nach AFIT-Migration (§6.9) native async fn.
/// Aktuell noch async_trait bis Migration abgeschlossen.
pub trait LlmTextGenerator: Send + Sync {
    async fn generate_text(&self, model: &str, prompt: &str) -> Result<String>;
    async fn generate_with_system(
        &self,
        model: &str,
        system: &str,
        prompt: &str,
    ) -> Result<String>;
    /// Liefert Log-Likelihoods für GASP Post-Hoc-Validator (§7.3).
    /// None wenn das Backend keine Logit-Access-API hat (z. B. Ollama-HTTP).
    /// Verfügbar bei: memfuse-candle (native), nicht bei Ollama-HTTP.
    async fn log_likelihood(
        &self,
        model: &str,
        prompt: &str,
        continuation: &str,
    ) -> Result<Option<f64>>;
    
    fn config_fingerprint(&self) -> ConfigFingerprint;
}

/// Primäre Abstraktion für Text-Embedding.
pub trait TextEmbeddingEngine: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn embedding_dimension(&self) -> usize;
    fn model_fingerprint(&self) -> ModelFingerprint;
}

// memfuse-core/src/traits/storage.rs

pub trait StorageEngine: Send + Sync {
    async fn get(&self, tx: &TxId, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tx: &TxId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, tx: &TxId, key: &[u8]) -> Result<()>;
    async fn scan_prefix(&self, tx: &TxId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    async fn begin_tx(&self) -> Result<TxId>;
    async fn commit_tx(&self, tx: TxId) -> Result<()>;
    async fn abort_tx(&self, tx: TxId) -> Result<()>;
}

// memfuse-core/src/traits/vector_index.rs

pub trait VectorIndex: Send + Sync {
    async fn insert(&self, id: DocId, vector: &[f32]) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(DocId, f32)>>;
    async fn delete(&self, id: DocId) -> Result<()>;
    async fn build(&self, vectors: &[(DocId, Vec<f32>)]) -> Result<()>;
    async fn persist(&self, path: &std::path::Path) -> Result<()>;
    async fn load(path: &std::path::Path) -> Result<Box<dyn VectorIndex>>;
}
```

### §3.3 Unified Calibration Primitive (`memfuse-calibration`)

**Wissenschaftliche Basis:** UCCI (arXiv:2605.18796) für den Router-Fall; direkt übertragbar auf Reranker-Konfidenz und ImportanceScore.

**Motivation:** Drei unabhängige, teils unvollständige Kalibrierungsprobleme im aktuellen Code:
1. `memfuse-router`: Conformal-Router-Fehlerwahrscheinlichkeit (teilweise implementiert)
2. `memfuse-embed/src/reranker.rs:311-314`: Sigmoid ohne Platt/Temperatur-Skalierung (AGT-EMBED-62093e61)
3. `memfuse-ollama/src/importance.rs`: LLM-Call ohne Kalibrierungsnachweis (AGT-OLLAMA-14c0c140)

Alle drei brauchen dieselbe mathematische Operation. Statt dreier paralleler Implementierungen (Kollisionsrisiko wie Commit `46a20b22`): eine gemeinsame Crate.

```rust
// crates/memfuse-calibration/src/lib.rs [NEU — Layer 0/1]
// Keine Abhängigkeit auf Ollama/Router/Embed — pure Mathematik

use std::collections::VecDeque;

/// Isotonische Regression über (raw_score, outcome)-Paaren.
/// Liefert kalibrierte Fehlerwahrscheinlichkeiten für beliebige Score-Systeme.
pub struct IsotonicCalibrator {
    warmup_required: u32,
    observations: VecDeque<(f32, bool)>, // (roh-Score, tatsächliches Outcome)
    max_observations: usize,
    /// Aktueller ConfigFingerprint — bei Änderung wird reset() erzwungen (P8)
    fingerprint: Option<ConfigFingerprint>,
}

impl IsotonicCalibrator {
    pub fn new(warmup_required: u32) -> Self {
        Self {
            warmup_required,
            observations: VecDeque::new(),
            max_observations: 10_000,
            fingerprint: None,
        }
    }
    
    pub fn record_outcome(&mut self, raw_score: f32, outcome: bool) {
        if self.observations.len() >= self.max_observations {
            self.observations.pop_front();
        }
        self.observations.push_back((raw_score, outcome));
    }
    
    /// None wenn warmup_required nicht erreicht (erzwingt calibrated: false).
    /// Ein None ist kein Fehler — es ist ein expliziter Hinweis auf fehlende Kalibrierung.
    pub fn calibrated_probability(&self, raw_score: f32) -> Option<f32> {
        if (self.observations.len() as u32) < self.warmup_required {
            return None; // Explizit: noch nicht kalibriert
        }
        Some(self.isotonic_regression_predict(raw_score))
    }
    
    pub fn is_calibrated(&self) -> bool {
        (self.observations.len() as u32) >= self.warmup_required
    }
    
    /// P8-PFLICHT: MUSS bei ConfigFingerprint-Änderung aufgerufen werden.
    /// Setzt Beobachtungen zurück, nicht nur den Fingerabdruck.
    pub fn invalidate_on_config_change(&mut self, new_fingerprint: ConfigFingerprint) {
        if self.fingerprint.as_ref() != Some(&new_fingerprint) {
            self.observations.clear();
            self.fingerprint = Some(new_fingerprint);
        }
    }
    
    fn isotonic_regression_predict(&self, raw_score: f32) -> f32 {
        // Pool-Adjacent Violators Algorithm (PAVA)
        // Implementierung: O(n log n) für sortierte Eingaben
        // ...
        todo!("PAVA implementation")
    }
    
    pub fn expected_calibration_error(&self) -> Option<f32> {
        // ECE-Berechnung über M Bins (M=10 Standard)
        // Ziel: ECE = 0.03 (UCCI-Referenz, arXiv:2605.18796)
        if !self.is_calibrated() { return None; }
        todo!("ECE calculation")
    }
}

/// Platt-Scaling als Alternative zu Isotonischer Regression.
/// Schneller (parametrisch), weniger flexibel.
pub struct PlattScaler {
    a: f32,
    b: f32,
    is_fitted: bool,
}

impl PlattScaler {
    pub fn fit(&mut self, scores: &[f32], labels: &[bool]) { todo!() }
    pub fn predict(&self, score: f32) -> Option<f32> {
        if !self.is_fitted { return None; }
        Some(1.0 / (1.0 + (-self.a * score - self.b).exp()))
    }
}
```

### §3.4 TenantKeyCodec (`memfuse-store/src/tenant_codec.rs`)

```rust
// crates/memfuse-store/src/tenant_codec.rs [NEU]

/// Encodiert LSM-Schlüssel mit Mandanten-Präfix.
/// Neue Schlüsselstruktur: t:{tenant_id}:{collection_id}:{doc_type}:{doc_id}
/// 
/// Isolation-Garantie: scan_prefix(b"t:{tenant_id}:") gibt AUSSCHLIESSLICH
/// Keys dieses Tenants zurück — O(1) Overhead, kein Cross-Tenant-Leak, kein Locking.
pub struct TenantKeyCodec {
    tenant_id: TenantId,
}

impl TenantKeyCodec {
    pub fn new(tenant_id: TenantId) -> Self {
        Self { tenant_id }
    }
    
    pub fn encode_chunk_key(&self, collection: &CollectionId, doc_id: DocId) -> Vec<u8> {
        format!("t:{}:{}:chunk:{}", self.tenant_id.0, collection.0, doc_id.0).into_bytes()
    }
    
    pub fn encode_graph_key(&self, collection: &CollectionId, entity_id: u64) -> Vec<u8> {
        format!("t:{}:{}:graph:{}", self.tenant_id.0, collection.0, entity_id).into_bytes()
    }
    
    pub fn scan_prefix(&self) -> Vec<u8> {
        format!("t:{}:", self.tenant_id.0).into_bytes()
    }
    
    pub fn collection_prefix(&self, collection: &CollectionId) -> Vec<u8> {
        format!("t:{}:{}:", self.tenant_id.0, collection.0).into_bytes()
    }
    
    /// Dekodiert TenantId aus einem Key.
    /// None wenn Key kein gültiges Tenant-Präfix hat.
    pub fn decode_tenant_id(key: &[u8]) -> Option<TenantId> {
        let s = std::str::from_utf8(key).ok()?;
        if !s.starts_with("t:") { return None; }
        let id_end = s[2..].find(':')? + 2;
        s[2..id_end].parse::<u64>().ok().map(TenantId)
    }
}
```

### §3.5 DeletionProof (`memfuse-crypto/src/deletion_proof.rs`)

**Wissenschaftliche Basis:** MUNKEY (arXiv:2603.15033), PrivUn (arXiv:2604.22076) stützen kryptographische Schlüssel-Löschung als einzig verifizierbare Löschmethode. Kontext: arXiv:2505.16831 ("Unlearning Isn't Deletion") zeigt, dass Modell-Parameter-Ebene außerhalb dieser Garantie liegt — diese Grenze ist Pflichtbestandteil der Enterprise-Dokumentation.

```rust
// crates/memfuse-crypto/src/deletion_proof.rs [NEU]

/// Kryptographisch verifizierbarer Löschbeweis für die STORAGE-Ebene.
/// 
/// KRITISCHE GRENZE (muss in Enterprise-Dokumentation sichtbar sein):
/// Dieser Proof deckt AUSSCHLIESSLICH die Storage-Ebene ab (LSM, WAL, HNSW, CSR, KV-Cache).
/// Er KANN NICHT garantieren, dass Wissen aus konsolidierten Zusammenfassungen,
/// die zum Fine-Tuning eines Drittmodells verwendet wurden, aus jenem Modell entfernbar ist.
/// Diese Grenze ist rechtlich relevant (DSGVO Art. 17 — "Recht auf Vergessenwerden").
/// Referenz: arXiv:2505.16831 (Unlearning Isn't Deletion, Mai 2026)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionProof {
    pub scope: DeletionScope,
    /// Blake3-Hash aller gelöschten Dokumentschlüssel (sortiert → deterministisch)
    pub deleted_keys_hash: [u8; 32],
    pub deleted_at_unix_secs: u64,
    /// HMAC-SHA256 über (scope || deleted_keys_hash || deleted_at_unix_secs)
    /// signiert mit HKDF-abgeleiteten Löschbeweis-Schlüssel des Tenants
    pub signature: [u8; 32],
    /// WAL-Sequenznummer nach der kein gelöschtes Datum mehr erscheint
    pub wal_seq_after_deletion: u64,
    /// [v3.0] Explizite maschinenlesbare Auflistung der abgedeckten Storage-Layer
    /// Macht Deckungsgrenze audit-fähig statt implizit
    pub covered_layers: Vec<DeletionLayer>,
    /// [v3.0] Explizite Nicht-Abdeckungs-Deklaration für rechtliche Klarheit
    pub excluded_scopes: Vec<ExcludedScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletionScope {
    Document { doc_id: DocId, tenant_id: TenantId },
    Collection { collection_id: CollectionId, tenant_id: TenantId },
    Tenant { tenant_id: TenantId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletionLayer {
    LsmMemtable,
    SsTableAllLevels,
    HnswIndex,
    WalAllSegments,
    CsrGraph,
    KvCacheSegments,  // [v3.0] Bindung an §7.1-Segmente desselben doc_id/tenant_id
    EmbeddingCache,
}

/// Explizite Dokumentation der Nicht-Abdeckung.
/// Pflicht-Bestandteil für DSGVO-Compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExcludedScope {
    /// Wissen das in konsolidierten Zusammenfassungen (SleepCycle) aufgegangen ist
    /// und als Input für externes Model-Fine-Tuning verwendet wurde
    ConsolidatedAndDistilled,
    /// LLM-Modellparameter (falls MemFuse-Candle verwendet wird)
    LlmParameterMemory,
}

impl DeletionProof {
    /// Verifiziert Signatur und bestätigt dass alle deklarierten Layer tatsächlich
    /// bereinigt wurden (via WAL-Scan).
    pub fn verify(&self, key_manager: &KeyManager) -> Result<bool> {
        todo!("HMAC-Verifikation + WAL-Scan")
    }
    
    /// Exportiert Proof als JSON für Compliance-Dokumentation.
    /// Enthält maschinenlesbare ExcludedScope-Deklaration.
    pub fn export_for_audit(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }
}
```

---

## §4 Layer 1 — Storage-Primitiven

### §4.1 WAL v3 mit HMAC-Chain (produktionsreif, verifiziert)

**Verifiziert:** `memfuse-store/src/wal.rs` — HMAC-SHA256-Chain, WAL-Eintrag bindet Hash des Vorgängers. AES-256-GCM-SIV via `KeyManager::encrypt_auto_nonce` (crypto.rs:148-179). Chaos-Test-Suite: `chaos_matrix.rs`, `chaos_memory_pressure.rs`, `chaos_task_massacre.rs`.

**Invarianten (nicht verhandelbar):**
- WAL-Commit vor MemTable-Update (P3)
- `dir.sync_all()` nach Parent-Directory-Rename (P3 — kein `let _ =`)
- HMAC-Chain-Verifikation beim Replay vor jedem Recovery
- Atomares Rename: tmp-Datei → endgültiger Name → Parent-fsync

```rust
// Schnittstelle (vereinfacht, Kernmethoden)
pub struct WalWriter {
    file: File,
    prev_hash: [u8; 32],
    key_manager: Arc<KeyManager>,
}

impl WalWriter {
    /// Committed einen Eintrag atomar: HMAC berechnen → Schreiben → fsync.
    /// Gibt WalSeqNum zurück die im DeletionProof verwendet wird.
    pub fn commit(&mut self, entry: WalEntry) -> Result<WalSeqNum>;
    pub fn rotate(&mut self) -> Result<()>;
    pub fn verify_chain(&self) -> Result<bool>;
}

pub struct WalEntry {
    pub seq: WalSeqNum,
    pub op: WalOperation,
    pub timestamp_us: u64,  // Mikrosekunden (ADR-016: deterministisch, kein SystemTime)
    pub hmac: [u8; 32],     // HMAC über (seq || op || timestamp || prev_hash)
}
```

### §4.2 LSM-Tree mit TenantKeyCodec (teilweise neu)

**Verifiziert:** `memfuse-store/src/lsm.rs` — MemTable (SkipList), SSTable (Bloom-Filter, CRC32), Background-Compaction, Tombstone-Tracking für DiskANN-Rebuild-Trigger.

**Neue Anforderung (v3.0):** `TenantKeyCodec` wird in jeden LSM-Schreibpfad integriert. Kein Key wird ohne Tenant-Präfix persistiert (wenn TenantId nicht SYSTEM ist).

```rust
pub struct LsmEngine {
    memtable: Arc<RwLock<MemTable>>,
    sstables: Arc<RwLock<Vec<SSTable>>>,
    wal: Arc<Mutex<WalWriter>>,
    /// [NEU] Tenant-Codec für Key-Isolation
    tenant_codec: Option<TenantKeyCodec>,
    compaction_engine: Arc<CompactionEngine>,
}

impl StorageEngine for LsmEngine {
    async fn put(&self, tx: &TxId, key: &[u8], value: &[u8]) -> Result<()> {
        // 1. WAL-Commit (P3-Pflicht vor MemTable-Update)
        let encoded_key = self.tenant_codec
            .as_ref()
            .map(|c| c.encode_raw(key))
            .unwrap_or_else(|| key.to_vec());
        self.wal.lock().await.commit(WalEntry::put(tx, &encoded_key, value))?;
        // 2. Erst nach WAL-Commit: MemTable-Update
        self.memtable.write().insert(encoded_key, value.to_vec());
        Ok(())
    }
}
```

### §4.3 DiskANN — Inkrementeller Persist-Pfad (`persist_delta()`)

**Verifizierter Ist-Zustand (C4):** `diskann.rs:394` — `build()` persistiert bereits vollständig atomar via `write_to_file()` (Zeile 499): Tmp → fsync → Rename → Parent-fsync. Test `test_write_to_file_uses_tmp_and_atomic_rename` (Zeile 1331) grün. **Die tatsächliche Lücke:** `insert()` (Zeile 990) = `Err("DiskAnn is a read-only out-of-core index")` — kein inkrementeller Update-Pfad nach Delta-Batch.

```rust
// crates/memfuse-index/src/diskann.rs [ERGÄNZUNG]

impl DiskAnnIndex {
    /// Baut AUSSCHLIESSLICH den Delta-Teil (neue Vektoren) und mergt ihn
    /// mit dem bestehenden On-Disk-Graph via atomaren Write-Pfad.
    /// 
    /// ALGORITHMUS:
    /// Phase 1: Delta-Graph-Konstruktion analog build() Phase 1 (Greedy + Vamana)
    ///          Nur für new_vectors, mit Nachbarschafts-Links zu bestehenden Knoten
    ///          (via read-only mmap des bestehenden Graphen)
    /// Phase 2: Merge: Bestehende On-Disk-Struktur + Delta-Graph via
    ///          identisches Tmp-Write → fsync → Rename-Muster wie write_to_file()
    /// 
    /// KOSTEN: O(|new_vectors| * log(N)) statt O(N) für Vollrebuild.
    /// 
    /// BEGRÜNDUNG: arXiv:2602.21514 liefert vollständigen Lifecycle-Rahmen
    /// für Disk-basiertes ANN inklusive dieses Patterns.
    pub async fn persist_delta(
        &self,
        new_vectors: &[(DocId, Vec<f32>)],
    ) -> Result<()> {
        if new_vectors.is_empty() {
            return Ok(());
        }
        
        // Phase 1: Delta-Graph-Konstruktion (nur neue Vektoren)
        // Nachbarschaftssuche in bestehendem Graph via mmap (read-only)
        let delta_neighbors = self.build_delta_neighbors(new_vectors).await?;
        
        // Phase 2: Atomares Merge + Persist (identisches Muster zu write_to_file())
        let tmp_path = self.path.with_extension("delta.tmp");
        self.write_merged_to_file(&tmp_path, &delta_neighbors).await?;
        // fsync der tmp-Datei
        File::open(&tmp_path)?.sync_all()?;
        // Atomares Rename
        std::fs::rename(&tmp_path, &self.path)?;
        // fsync des Parent-Directories
        File::open(self.path.parent().unwrap())?.sync_all()?;
        
        Ok(())
    }
}
```

### §4.4 HNSW — Produktionsreif mit Tombstone-Integration

**Verifiziert:** `memfuse-index/src/hnsw.rs` — M=16, ef_construction=32, Diversity-Heuristik (nicht-greedy), SQ8-Quantisierung (4× RAM-Reduktion), RoaringTreemap-Tombstone-Bitmap. Rebuild-Trigger: `HNSW_REBUILD_DELETION_RATIO = 0.10` (global).

**Interaktion mit F-01 (Thermostat):** Tombstone-Ratio ist Input für SystemTemperature-Berechnung (§9.1). Keine Code-Änderung an HNSW nötig — nur Metrik-Exposition.

**Interaktion mit F-06 (Perkolationsmonitor):** BFS-Sampling für φ(t)-Berechnung läuft auf MVCC-Snapshot (nicht im Hot-Path).

```rust
// Bestehende Interface-Kern-Methoden (produktionsreif)
impl HnswIndex {
    pub async fn insert(&self, id: DocId, vector: &[f32]) -> Result<()>;
    pub async fn search(&self, query: &[f32], k: usize, ef: usize) -> Result<Vec<(DocId, f32)>>;
    pub async fn soft_delete(&self, id: DocId) -> Result<()>;
    /// Gibt aktuellen Tombstone-Ratio zurück — Input für F-01-Thermostat
    pub fn tombstone_ratio(&self) -> f32;
    /// Atomarer 2-Phasen-Rebuild (ADR-061): Read-Only-Snapshot + neuer Graph + Swap
    pub async fn rebuild_if_needed(&self) -> Result<bool>;
}
```

### §4.5 CSR-Graph mit Kanten-Provenienz-Pflicht

**Verifiziert:** `memfuse-graph/src/csr.rs` — `tombstoned_edges` (EdgeId-Bitmap), bi-temporale Kanten (ADR-033), Supersedes (ADR-038). `NodesGuard<'a>(MutexGuard<'a, HashMap<NodeIdx, AgentStateNode>>)` in session_dag.rs:29 — Typ-erzwungene Lock-Reihenfolge.

**Neue Anforderung (v3.0) — Kanten-Provenienz-Pflicht:**

```rust
// crates/memfuse-graph/src/provenance.rs [NEU]

/// Provenienz-Eintrag für eine CSR-Graph-Kante.
/// 
/// INVARIANTE (INV-GRAPH-PROV-1, v3.0):
/// JEDE CSR-Kante MUSS einen gültigen EdgeProvenance-Eintrag haben,
/// der auf einen WAL-committed, Blake3-verifizierten Chunk verweist.
/// 
/// MOTIVATION: arXiv:2603.14828 zeigt, dass ungeprüfte LLM-Inferenz-Ausgaben
/// als Graph-Kanten "Retrieval Drift" und "Retrieval Hallucination" produzieren.
/// Kanten aus verifizierten Quellen sind das Mittel dagegen.
/// 
/// ERZWINGUNG: Property-Test in memfuse-bench prüft, dass kein Persist-Aufruf
/// eine Kante ohne begleitenden WAL-Eintrag schreibt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeProvenance {
    /// Blake3-Content-Hash des Quell-Chunks (bereits in Pipeline vorhanden)
    pub source_chunk_hash: [u8; 32],
    /// DocId des Quell-Chunks
    pub source_doc_id: DocId,
    /// WAL-Sequenznummer des Commits der den Quell-Chunk persistiert hat
    pub wal_seq: WalSeqNum,
    /// Extraktionsmethode (NER, LLM-Extraktion, manuell)
    pub extraction_method: ExtractionMethod,
    pub created_at_tx: TxId,
}

/// [NEU — §6.16 Integration] Trigger-Pfad von Supersedes-Event zu Kanten-Tombstone.
/// 
/// Problem (Gegenprüfung §4, v2.1): Wenn Chunk durch Supersedes-Displacement
/// als veraltet markiert wird, werden abhängige Graph-Kanten NICHT automatisch
/// tombstoned — dieser Trigger-Pfad fehlte bisher.
/// 
/// Lösung: Jede Supersedes-Operation in QueryBuilder ruft diese Funktion auf.
pub fn cascade_tombstone_superseded_edges(
    csr: &mut CsrGraph,
    superseded_doc_id: DocId,
    effective_at: TxId,
) -> Result<Vec<EdgeId>> {
    // 1. Alle Kanten finden, deren EdgeProvenance auf superseded_doc_id verweist
    let affected_edges = csr.edges_from_source(superseded_doc_id);
    
    // 2. Kanten tombstonen mit WalEntry (P3-Pflicht)
    let mut tombstoned = Vec::new();
    for edge_id in affected_edges {
        csr.tombstone_edge(edge_id, effective_at)?;
        tombstoned.push(edge_id);
    }
    
    Ok(tombstoned)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtractionMethod {
    Ner { model_id: String },
    LlmExtraction { model_id: String, prompt_hash: [u8; 32] },
    Manual,
    RuleBasedPattern { pattern_id: String },
}
```

---

## §5 Layer 2 — Orchestrierung & Fusion

### §5.1 3-Signal-RRF mit Kohärenz-Bonus (F-09)

**Verifizierter Ist-Zustand (C2):** `fusion.rs:51-58` — `SignalKind::{Vector, Text, Graph}` (drei Varianten, kein Rerank). Rerank ist Post-Fusion in `search.rs:440ff`.

```rust
// crates/memfuse-db/src/fusion.rs [ERWEITERT um F-09]

pub enum SignalKind {
    Vector,   // HNSW semantische Ähnlichkeit
    Text,     // BM25 + DE-Morphologie
    Graph,    // PPR + Community Detection
    // [INTERN] PathRag als additives 4. Signal (nur bei MultiHop, §5.4)
    PathRag,
}

#[derive(Debug, Clone)]
pub struct FusionResult {
    pub doc_id: DocId,
    pub rrf_score: f32,
    pub provenance: ProvenanceRecord,
}

/// INV-PROV-1 (unveränderlich): sum(contributions.rrf_contribution) ≈ rrf_score (|Δ| < 1e-6)
/// INV-PROV-2 (NEU v3.0, F-09): coherence_bonus ist separates Feld, NICHT in rrf_score gefaltet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub contributions: Vec<SignalContribution>,
    pub rrf_score: f32,      // Reine RRF-Summe OHNE Kohärenz-Bonus
    /// [NEU v3.0 — F-09] Kohärenz-Bonus als eigenständiges, sichtbares Feld.
    /// INV-PROV-2: rrf_score + coherence_bonus = final_score (exakt rekonstruierbar)
    pub coherence_bonus: Option<f32>,
    pub final_score: f32,    // rrf_score + coherence_bonus.unwrap_or(0.0)
    pub rerank_score: Option<f32>, // Nachgelagert, nach Fusion
    pub synaptic_score: Option<f32>, // [NEU F-03] Kantengewicht-Score
}

/// Standard-RRF (k=60): Wissenschaftlich validiert (T2-RAGBench, arXiv:2604.01733)
/// Konvex-Kombination (α=0.5) übertrifft RRF nur um +1pp auf T2-RAGBench →
/// kein zwingender Wechsel, aber konfigurierbar.
pub fn reciprocal_rank_fusion(
    ranked_lists: &HashMap<SignalKind, Vec<DocId>>,
    k: f32, // Default: 60.0
) -> Vec<FusionResult> {
    // Kernformel: RRF(d) = Σ_s 1/(k + rank_s(d))
    todo!("Standard-RRF")
}

pub fn weighted_reciprocal_rank_fusion(
    ranked_lists: &HashMap<SignalKind, Vec<DocId>>,
    weights: &HashMap<SignalKind, f32>, // Adaptiv via F-07 (§6.24)
    k: f32,
) -> Vec<FusionResult> {
    todo!("Gewichtetes RRF")
}

/// [NEU v3.0 — F-09 Resonanz-Fusion]
/// 
/// NATURVORBILD: Konstruktive Interferenz — wenn n unabhängige Signale
/// für denselben Kandidaten übereinstimmend hohe Ränge vergeben,
/// ist das stärkeres Indiz als die additive RRF-Summe abbildet.
/// 
/// MATHEMATISCHES MODELL:
/// Normalisierte Ränge: r_i(d) ∈ [0,1] (0 = höchster Rang)
/// Kohärenz: C(d) = 1 − (2/(n*(n-1))) * Σ_{i<j} |r_i(d) − r_j(d)|
/// Bonus: coherence_bonus(d) = β * C(d) * RRF(d)  (β Default: 0.15)
/// 
/// INVARIANTE (INV-PROV-2): coherence_bonus wird IMMER als separates Feld
/// gespeichert, NIE unsichtbar in rrf_score gefaltet.
/// 
/// AKZEPTANZKRITERIUM: Kohärenzgewichtete Fusion übertrifft reine RRF-Baseline
/// bei Recall@5 um ≥ 1pp auf T2-RAGBench-artigem Evaluationskorpus.
pub fn apply_coherence_bonus(
    results: &mut Vec<FusionResult>,
    beta: f32, // Default: 0.15, konfigurierbar in PhysioConfig
) {
    let n = results.len() as f32;
    if n < 2.0 { return; }
    
    // Für jeden Kandidaten: Kohärenz über alle aktiven Signale berechnen
    for result in results.iter_mut() {
        let coherence = compute_signal_coherence(&result.provenance.contributions);
        let bonus = beta * coherence * result.rrf_score;
        result.provenance.coherence_bonus = Some(bonus);
        result.provenance.final_score = result.rrf_score + bonus;
    }
    
    // Neu sortieren nach final_score
    results.sort_by(|a, b| b.provenance.final_score.partial_cmp(&a.provenance.final_score)
        .unwrap_or(std::cmp::Ordering::Equal));
}

fn compute_signal_coherence(contributions: &[SignalContribution]) -> f32 {
    let n = contributions.len();
    if n < 2 { return 1.0; }
    
    // Normalisierte Rang-Differenzen zwischen allen Signalpaaren
    let mut total_diff = 0.0_f32;
    for i in 0..n {
        for j in (i+1)..n {
            total_diff += (contributions[i].normalized_rank - contributions[j].normalized_rank).abs();
        }
    }
    
    let pairs = (n * (n - 1)) as f32 / 2.0;
    1.0 - (total_diff / pairs)
}
```

### §5.2 Reranking mit kalibriertem Kandidatenfenster (Fix + PID)

**Verifizierter Ist-Zustand (C3):** `search.rs:450`: `let pre_rerank_k = if reranker.is_some() { k * 3 } else { k };` → bei k=10 = 30 Kandidaten.

**Wissenschaftlicher Befund:** arXiv:2604.01733 (T2-RAGBench): Recall@5 bei 20 Kandidaten = 0.458 (fast wertlos), bei 50 = 0.826, bei 100 = 0.888.

```rust
// crates/memfuse-db/src/collection/search.rs [FIX + PID-Erweiterung]

/// SOFORT-FIX (P-SOFORT-3): Kandidatenpool-Minimum auf 100 setzen.
/// Konfigurierbar (P11-Latenzbudget: auf schwacher Hardware ggf. reduzieren).
pub struct RerankConfig {
    /// Minimum-Kandidaten für Reranker (wissenschaftlich: ≥ 100 für Recall@5 = 0.888)
    /// Default: 100 (arXiv:2604.01733)
    pub min_rerank_candidates: usize,
    /// Hartes Zeit-Budget für Reranker (P11-Pflicht): nach Ablauf → Abbruch mit verfügbaren Ergebnissen
    pub deadline_ms: u64,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            min_rerank_candidates: 100,
            deadline_ms: 500, // Gegenprüfung §1: harter Deadline-Fix
        }
    }
}

// SOFORT-FIX (ersetzt search.rs:450):
let pre_rerank_k = if reranker.is_some() {
    (k * 3).max(config.min_rerank_candidates).min(MAX_SEARCH_K)
} else {
    k
};

/// [F-08 Langfristziel — §9.8] PID-geregelte Kandidatenpoolgröße.
/// Ersetzt die statische min_rerank_candidates durch dynamische Regelung
/// nach P95-Latenz-Feedback. Wird erst nach Deadline-Fix (Sofort-Fix) eingeführt.
pub struct RerankPidController {
    kp: f32, kd: f32, ki: f32,
    target_p95_latency_ms: f32,
    integral: f32,
    last_error: f32,
    k_pool: usize, // Aktueller Kandidatenpool
    k_min: usize,  // Absolute Untergrenze (wissenschaftlich: 50 Minimum)
    k_max: usize,  // Absolute Obergrenze
}

/// Chunk-Injektionsreihenfolge (§5.3) — nach Reranker-Score anwenden
pub fn order_chunks_for_injection(mut chunks: Vec<ScoredChunk>) -> Vec<ScoredChunk> {
    // Absteigend nach Reranker-Score (höchster Score → relevantester Chunk)
    chunks.sort_by(|a, b| b.rerank_score.partial_cmp(&a.rerank_score)
        .unwrap_or(std::cmp::Ordering::Equal));
    
    // Lost-in-the-Middle Mitigation (Stable-RAG, arXiv:2601.02993):
    // Bei > 4 Chunks: höchste Scores an Anfang UND Ende verteilen
    if chunks.len() > 4 {
        interleave_edges(chunks)
    } else {
        chunks
    }
}

fn interleave_edges(chunks: Vec<ScoredChunk>) -> Vec<ScoredChunk> {
    let n = chunks.len();
    let mut result = Vec::with_capacity(n);
    let mut left = 0;
    let mut right = n - 1;
    let mut from_front = true;
    
    while left <= right {
        if from_front {
            result.push(chunks[left].clone());
            left += 1;
        } else {
            result.push(chunks[right].clone());
            if right == 0 { break; }
            right -= 1;
        }
        from_front = !from_front;
    }
    result
}
```

### §5.3 Temporal Validity Post-Filter

**Wissenschaftliche Basis:** MinnsDB-Analyse (MemFuse_vs_Competitors) zeigt Temporal Validity Filter als kritisches Feature: Post-Fusion Masking — nur `valid_from <= NOW() < valid_until`.

**Bezug zu bestehenden ADRs:** ADR-033 (bi-temporal Kanten), ADR-038 (Supersedes).

```rust
// crates/memfuse-db/src/collection/search.rs [NEU §5.3]

/// Post-Fusion Temporal Validity Filter.
/// Entfernt Kandidaten deren Bi-Temporal-Fenster abgelaufen ist.
/// Wird NACH RRF-Fusion, VOR dem Reranker angewendet.
/// 
/// WICHTIG: Dieser Filter arbeitet auf tx_valid_to + business_valid_to (ADR-033).
/// Ein abgelaufener Chunk wird nicht gelöscht, nur aus dem Ergebnis-Set maskiert.
pub fn apply_temporal_validity_filter(
    results: Vec<FusionResult>,
    current_tx: TxId,
    query_timestamp: Option<u64>, // None → aktuelle Zeit
) -> Vec<FusionResult> {
    results.into_iter().filter(|r| {
        match get_chunk_validity_window(r.doc_id, current_tx) {
            Some(window) => window.is_valid_at(query_timestamp.unwrap_or(now_unix_secs())),
            None => true, // Kein Validity-Window → immer gültig
        }
    }).collect()
}

/// Erlaubt historische Queries (analog Graphiti's Episode-Pinning):
/// "Was war der Informationsstand am Datum X?"
pub fn apply_temporal_validity_filter_at(
    results: Vec<FusionResult>,
    as_of_timestamp: u64,
) -> Vec<FusionResult> {
    todo!("Historical query support")
}
```

### §5.4 PathRAG Engine mit Query-Klassifikator & Sufficiency-Gate

**Wissenschaftliche Basis:** PathRAG (arXiv:2502.14902, AAAI 2026), ICLR 2026 "When to use Graphs in RAG" (arXiv:2506.05690) — GraphRAG lohnt NUR bei Multi-Hop-Intent.

**Kritische Gegenposition:** MemGraphRAG (arXiv:2506.00610): Recall↑ aber Precision massiv↓ (38.5% vs 62.9%) → Sufficiency-Gate ist Pflicht.

```rust
// crates/memfuse-graph/src/path_rag.rs [NEU]

/// Query-Hop-Klassifikation — entscheidet ob PathRAG aktiviert wird.
/// Trainiert auf gelabelten Single-/Multi-Hop-Test-Sets.
/// Akzeptanzkriterium: ≥ 85% Klassifikationsgenauigkeit.
#[derive(Debug, Clone)]
pub enum QueryHopClass {
    /// Einfacher Fact-Lookup → Dense+BM25+Rerank kostengünstiger UND präziser
    SingleHop,
    /// Erfordert Verkettung mehrerer Fakten → PathRAG als additives Signal rechtfertigt
    MultiHop { estimated_hops: u8 },
}

/// Query-Typ für Quellauswahl im Dual-Process-Gedächtnis (§5.5).
/// Motiviert durch arXiv:2605.17625: Dual-Process übertrifft RAG bei
/// numerisch/temporal (65-90%), verliert bei historisch-faktisch (60-85%).
#[derive(Debug, Clone)]
pub enum QuerySourceClass {
    NumericalTemporal,   // → Dual-Process Gedächtnis bevorzugt
    HistoricalFactual,   // → Standard-RAG bevorzugt
    Analytical,          // → PathRAG wenn Multi-Hop-Intent
}

pub struct PathRagEngine<'a> {
    graph: &'a CsrGraph,
    session_dag: &'a SessionBranchTree,
    /// Sufficiency-Gate: Konfidenz-Schwelle für Pfad-Akzeptanz
    /// Default: 0.6 (verhindert MemGraphRAG-Precision-Problem)
    sufficiency_threshold: f64,
}

#[derive(Debug, Clone)]
pub struct CausalPath {
    pub nodes: Vec<NodeIdx>,
    /// Menschenlesbarer Erklärungstext für RAG-Kontext-Injektion
    pub explanation: String,
    /// Gesamtkonfidenz des Pfades (Produkt der Kantengewichte)
    pub confidence: f64,
    /// Provenienz-Kette: Jeder Knoten → source_chunk_hash (INV-GRAPH-PROV-1)
    pub provenance_chain: Vec<EdgeProvenance>,
}

impl<'a> PathRagEngine<'a> {
    /// Klassifiziert Query-Intent VOR Dijkstra-Aufruf.
    /// Dijkstra läuft NUR bei MultiHop-Klassifikation.
    pub fn classify_query(&self, query_embedding: &[f32]) -> QueryHopClass {
        // Lightweight Classifier: k-NN über Embedding-Space
        // gegen pre-labeled Multi-Hop vs. Single-Hop Anker-Exemplare
        todo!("Classifier implementation")
    }
    
    /// Bidirektionaler Dijkstra auf CSR-Graph.
    /// LÄUFT NUR wenn classify_query() → MultiHop liefert.
    pub fn find_causal_path(
        &self,
        source: NodeIdx,
        target: NodeIdx,
    ) -> Option<CausalPath> {
        // Bidirektionaler Dijkstra: O((V+E) log V) — effizienter als unidirektional
        todo!("Bidirectional Dijkstra")
    }
    
    /// Sufficiency-Gate: Verwirft Pfade unter Konfidenz-Schwelle VOR RAG-Injektion.
    /// Verhindert das in arXiv:2506.00610 dokumentierte Precision-Problem.
    pub fn sufficiency_check(&self, path: &CausalPath) -> bool {
        path.confidence > self.sufficiency_threshold
    }
    
    /// Flow-basiertes Pruning nach PathRAG-Methodik (arXiv:2502.14902).
    /// Gibt nur die top-k kausal relevantesten Pfade zurück.
    pub fn flow_pruned_paths(
        &self,
        query_nodes: &[NodeIdx],
        max_paths: usize,
    ) -> Vec<CausalPath> {
        todo!("Flow-based pruning per PathRAG paper")
    }
    
    /// Formatiert einen kausalen Pfad für RAG-Kontext-Injektion.
    /// Format: "Schritt A → [verursachte] → Schritt B → [resultierte in] → Schritt C"
    pub fn format_for_rag(&self, path: &CausalPath) -> String {
        todo!("RAG context formatting")
    }
    
    /// Integration in RRF: PathRag als additives 4. Signal (nur bei MultiHop).
    /// Gibt (DocId, Score)-Liste zurück für weighted_reciprocal_rank_fusion().
    pub fn to_rrf_signal(&self, paths: &[CausalPath]) -> Vec<(DocId, f32)> {
        todo!("Convert paths to RRF-compatible ranked list")
    }
}
```

### §5.5 SleepCycle — NREM + REM-Phase

**Wissenschaftliche Basis:** SleepGate (arXiv:2603.14517), Dual-Process (arXiv:2605.17625), LycheeMemory V2 (arXiv:2608.12990, SOTA August 2026: 92.20%/89.22%). LycheeMemory V2 zeigt Segment-Level-Konsolidierung reduziert Construction-Tokens um 86.0%.

**Kritischer Befund:** Bottleneck ist Prompt-Design, nicht Modellgröße (GPT-4o-mini → GPT-4o: +0.07%). Der Konsolidierungs-Prompt ist der wichtigste Hebel.

```rust
// crates/memfuse-db/src/sleep_cycle.rs [NEU]

pub struct SleepCycleConfig {
    pub episode_threshold: usize,       // Default: 100 (NREM-Trigger)
    pub check_interval_secs: u64,       // Default: 3600
    pub max_chunks_per_cycle: usize,    // Default: 50 (Token-Kostenschutz)
    pub min_age_secs: u64,              // Default: 86400 (aktive Sessions schützen)
    pub entropy_threshold: f32,         // Default: 0.7 (SleepGate-Trigger)
    
    // [v3.0 — LycheeMemory-Prinzip] Segment-Level statt Turn-Level
    pub segment_min_turns: usize,       // Default: 3
    pub segment_max_turns: usize,       // Default: 12
    
    // [v3.0] Versionierter Konsolidierungs-Prompt (P8-analoge Regel für Prompts)
    // Änderung erfordert Re-Validierung gegen memfuse-bench
    pub nrem_prompt_template_version: u32,
    pub rem_prompt_template_version: u32,  // [F-05] REM-Phase
    
    // [F-05] REM-Synthese-Phase
    pub rem_enabled: bool,              // Feature-Flag: physio-rem-synthesis
    pub rem_max_llm_calls_per_cycle: usize, // Default: 10 (Kostenschutz für P1)
    pub rem_community_cohesion_threshold: f32, // Default: 0.7
    pub rem_community_min_size: usize,  // Default: 5 Chunks
    pub rem_community_stability_cycles: u32, // Default: 3 (Stabilitätskriterium)
}

/// Metriken für Observability
pub struct SleepCycleMetrics {
    pub cycles_completed: u64,
    pub chunks_consolidated: u64,
    pub errors: u64,
    pub last_run_duration_ms: u64,
    pub interference_horizon_before: usize,
    pub interference_horizon_after: usize,
    /// Token-Reduktion vs. naive Turn-für-Turn-Extraktion
    /// Zielwert: 86% (LycheeMemory V2 Referenz, arXiv:2608.12990)
    pub construction_token_reduction_pct: f32,
    /// [F-05] REM-Phase Metriken
    pub rem_meta_chunks_created: u64,
    pub rem_llm_calls: u64,
}

pub struct SleepCycleScheduler {
    config: SleepCycleConfig,
    db: Arc<MemFuseDb>,
    calibration_metrics: Arc<SleepCycleMetrics>,
}

impl SleepCycleScheduler {
    /// Hauptloop — läuft als Tokio-Background-Task.
    /// 
    /// AKTIVIERUNGSBEDINGUNG: db.active_agent_sessions() == 0
    /// CRASH-SICHERHEIT: Konsolidierungs-Intent im WAL VOR Arbeitsbeginn
    /// IDEMPOTENZ: Bei Neustart: orphan Intents bereinigen (via maintenance.rs)
    pub async fn run_loop(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(self.config.check_interval_secs)).await;
            
            if self.db.active_agent_sessions().await > 0 {
                continue; // Aktive Sessions nicht stören
            }
            
            let episode_count = self.db.episodic_chunk_count().await;
            if episode_count < self.config.episode_threshold {
                continue;
            }
            
            // WAL-Intent schreiben (P3-Pflicht vor Arbeit)
            let intent_seq = self.db.write_consolidation_intent().await?;
            
            // Phase 1: NREM — Segment-Level-Deduplication und Kompression
            self.run_nrem_phase().await?;
            
            // Phase 2: REM — Generative Synthese (wenn aktiviert)
            if self.config.rem_enabled {
                self.run_rem_phase().await?;
            }
            
            // Intent als abgeschlossen markieren
            self.db.complete_consolidation_intent(intent_seq).await?;
        }
    }
    
    /// NREM-Phase: Segment-Level Deduplication und Kompression.
    /// Implementiert LycheeMemory V2 Kernprinzip: Semantisch zusammenhängende
    /// Turn-Cluster zu EINEM Segment gruppieren vor Extraktion.
    async fn run_nrem_phase(&self) -> Result<()> {
        // 1. Turns zu Segmenten gruppieren (min_turns bis max_turns)
        // 2. Near-Duplicate-Merging: cosine > 0.95 → älteres tombstonen
        // 3. Semantische Verdichtung via ContextCompactor
        // 4. graph Kanten-Tombstone für veraltete Chunks (§4.5 Trigger-Pfad)
        todo!()
    }
    
    /// REM-Phase (F-05): Generative Wissenssynthese.
    /// NUR für stabile Communities (stability_cycles Kriterium).
    /// PFLICHT: Jeder Meta-Chunk trägt abstracts_from-Kanten zu Quell-Chunks.
    async fn run_rem_phase(&self) -> Result<()> {
        // 1. Communities via Label-Propagation (ADR-027) identifizieren
        // 2. Filter: Kohäsion > threshold AND Größe >= min_size AND stabil >= stability_cycles
        // 3. Budget-check: max_llm_calls_per_cycle (P12: Kostenschutz für P1)
        // 4. Für gefilterte Stable-Communities: LLM-Synthese-Aufruf
        // 5. Meta-Chunk mit abstracts_from-Kantentyp persistieren (WAL-First, P3)
        // 6. Jeder Meta-Chunk: "abstrahiert aus N Quellen"-Markierung PFLICHT
        //    (Halluzinations-Guard-Kompatibilität, §7.3)
        todo!()
    }
}
```

### §5.6 Freie-Energie-Thermostat (F-01)

**Naturvorbild:** Freie Energie F = U − T·S. Systemtemperatur T bestimmt adaptiv die Half-Life für Chunk-Retention.

**Architektur-Anforderung:** Deterministisch (kein SystemTime — ADR-016), abgeleitet aus TxId-Zähldifferenzen und Tombstone-Ratio.

```rust
// crates/memfuse-db/src/thermostat.rs [NEU — F-01]

/// Input-Signale für Systemtemperatur-Berechnung.
/// BEIDE Signale sind bereits im System vorhanden — keine neue Infrastruktur nötig.
pub struct ThermostatInputs {
    /// Aus memfuse-store: HNSW-Tombstone-Ratio (0.0–1.0)
    pub tombstone_ratio: f32,
    /// Aus memfuse-router: Query-Rate (inverse = niedriger bei hoher Last)
    /// Berechnet über TxId-Zähldifferenzen (ADR-016-konform, kein SystemTime)
    pub query_rate_inverse: f32,
    /// Gewichte (konfigurierbar in PhysioConfig)
    pub w1: f32, // Default: 0.6 (Speicherdruck hat mehr Gewicht)
    pub w2: f32, // Default: 0.4
}

/// MATHEMATISCHES MODELL (aus Physik-PRD, exakt):
/// T(t) = w1 * memory_pressure(t) + w2 * query_rate_inverse(t)  ∈ [0,1]
/// half_life_eff = half_life_base * (1 + κ * (1 − T(t)))
/// effective_score = base_score * exp(−(ln2 / half_life_eff) * elapsed)
/// 
/// SEMANTIK:
/// Hohe T (voller Speicher, viele Queries) → kurze Half-Life → aggressiveres Vergessen
/// Niedrige T (freier Speicher, wenige Queries) → lange Half-Life → längeres Behalten
pub struct FreeEnergyThermostat {
    pub kappa: f32, // Verstärkungsfaktor, Default: 2.0 (konfigurierbar)
}

impl FreeEnergyThermostat {
    pub fn system_temperature(&self, inputs: &ThermostatInputs) -> f32 {
        (inputs.w1 * inputs.tombstone_ratio + inputs.w2 * inputs.query_rate_inverse)
            .clamp(0.0, 1.0)
    }
    
    pub fn effective_half_life(&self, base_half_life: u64, temperature: f32) -> f32 {
        base_half_life as f32 * (1.0 + self.kappa * (1.0 - temperature))
    }
    
    pub fn effective_score(&self, base_score: f32, elapsed_secs: f32, inputs: &ThermostatInputs, base_half_life: u64) -> f32 {
        let temperature = self.system_temperature(inputs);
        let half_life_eff = self.effective_half_life(base_half_life, temperature);
        base_score * (-(std::f32::consts::LN_2 / half_life_eff) * elapsed_secs).exp()
    }
}
```
---

## §6 Layer 3 — Inferenz, Routing & Physio-Selbstregulierung

### §6.1 ImportanceClassifier — Reform des Hot-Path

**Verifizierter Ist-Zustand (C6):** `importance.rs:39` trägt AI-TAG AGT-OLLAMA-14c0c140 (MAJOR). Pro Chunk: ein vollständiger LLM-Forward-Pass für eine Skalarzahl. Stiller 0.5-Fallback bei Parse-Fehler ohne Log.

**Wissenschaftliche Basis:** MemRouter (arXiv:2605.00356): Embedding-Classifier (12M Params) → F1 52.0 vs 45.6 (LLM), P50-Latenz 58ms vs 970ms.

```rust
// crates/memfuse-embed/src/importance_classifier.rs [NEU]

/// Ersetzt memfuse-ollama/src/importance.rs im Hot-Path.
/// 
/// IMPLEMENTIERUNGS-STRATEGIE:
/// Variante A (empfohlen): ONNX-Kopfnetz auf bestehenden Chunk-Embeddings.
///   - Wiederverwendet Arc<Mutex<Session>> aus embedder.rs
///   - Kein zweiter Forward-Pass durchs Basismodell (Zero-Overhead)
///   - Nur der schlanke Klassifikationskopf ist neu
/// 
/// Variante B (trainierungsfrei, schneller zu deployen):
///   - k-NN/Centroid-Klassifikator gegen Anker-Exemplare
///   - Kein ONNX-Training nötig
///   - Trainingsdaten: bestehende score_importance()-Aufrufe als schwaches Label-Signal (Distillation)
/// 
/// ADR-PFLICHT: Vor Implementierung ADR wählt zwischen A und B.
/// Diese Spezifikation empfiehlt Variante B für Phase 1 (schneller deploybar),
/// Variante A für Phase 2 (höhere Qualität nach Distillations-Training).
pub struct ImportanceClassifier {
    /// Variante A: ONNX-Klassifikationskopf
    session: Option<Arc<parking_lot::Mutex<ort::Session>>>,
    /// Variante B: k-NN Centroid-Klassifikator
    anchors: Option<Vec<(Vec<f32>, f32)>>, // (embedding, importance_label)
    model_fingerprint: ModelFingerprint,
    /// Kalibrierung via memfuse-calibration (schließt AGT-EMBED-62093e61 analog)
    calibrator: Arc<Mutex<IsotonicCalibrator>>,
}

/// Kalibriertes Importance-Score-Ergebnis.
/// Ersetzt den stillen 0.5-Fallback durch explizite Kalibrierungsmetriken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibratedImportanceScore {
    pub score: f32,
    /// false wenn calibrator.is_calibrated() == false → explizit, kein 0.5-Fallback
    pub calibrated: bool,
    pub model_id: String,
    pub scored_at_tx: TxId, // ADR-016: TxId statt SystemTime
    /// None wenn calibrated == false
    pub error_probability: Option<f32>,
    pub calibration_samples_seen: u32,
}

impl ImportanceClassifier {
    /// Bewertet einen Chunk.
    /// ZIEL-LATENZ: < 100ms (P50), Referenz: MemRouter 58ms.
    /// KEIN LLM-CALL im Hot-Path.
    pub fn score(&self, chunk_embedding: &[f32]) -> Result<CalibratedImportanceScore> {
        let raw_score = match (&self.session, &self.anchors) {
            (Some(session), _) => self.score_with_onnx(session, chunk_embedding)?,
            (_, Some(anchors)) => self.score_with_knn(anchors, chunk_embedding)?,
            _ => return Err(MemFuseError::ClassifierNotInitialized),
        };
        
        let calibrated_prob = self.calibrator.lock().calibrated_probability(raw_score);
        
        Ok(CalibratedImportanceScore {
            score: raw_score,
            calibrated: calibrated_prob.is_some(),
            model_id: self.model_fingerprint.model_id.clone(),
            scored_at_tx: TxId(0), // Wird von Caller gesetzt
            error_probability: calibrated_prob,
            calibration_samples_seen: self.calibrator.lock().observation_count() as u32,
        })
    }
    
    /// Batch-API — wiederverwendet score_batch()-Infrastruktur aus reranker.rs:195.
    /// PFLICHT: Kein sequenzieller Aufruf von score() für jeden Chunk in einem Dokument.
    /// Bei N Chunks in einem Dokument muss score_batch() genutzt werden.
    pub fn score_batch(&self, embeddings: &[Vec<f32>]) -> Result<Vec<CalibratedImportanceScore>> {
        // Batch-Inferenz: Ein ONNX-Session-Aufruf für alle Chunks
        todo!()
    }
    
    /// Akzeptanzkriterien:
    /// - P50-Latenz < 100ms (Ziel: ~58ms wie MemRouter-Referenz)
    /// - F1 auf gelabeltem Chunk-Set >= LLM-Baseline (45.6)
    /// - KEIN stiller 0.5-Fallback mehr
    /// - Explizit calibrated: false bei fehlender Kalibrierung
    fn score_with_knn(&self, anchors: &[(Vec<f32>, f32)], embedding: &[f32]) -> Result<f32> {
        // k-NN über Cosine-Distanz zu Anker-Exemplaren
        todo!()
    }
    
    fn score_with_onnx(&self, session: &Arc<parking_lot::Mutex<ort::Session>>, embedding: &[f32]) -> Result<f32> {
        todo!()
    }
}

/// Migration: Der LLM-Pfad in importance.rs bleibt für:
/// - Rückwärtskompatibilität (Deployment ohne memfuse-embed-Feature)
/// - Trainings-Datengenerierung (schwaches Labeling für Distillation)
/// 
/// Markierung: #[deprecated(since = "v3.0", note = "Use ImportanceClassifier::score()")]
/// AI-TAG AGT-OLLAMA-14c0c140 wird erst nach ImportanceClassifier-Rollout als [RESOLVED] markiert.
```

### §6.2 Conformal Router mit ConfigFingerprint & Lyapunov-Drift

**Verifizierter Ist-Zustand (C8):** `profile.rs:9-19` — `SlmProfile` hat KEINE Felder für Temperatur, Prompt-Template-Hash oder Quantisierungsgrad. Coverage-Kollaps-Risiko bei Konfigurationsänderung (arXiv:2608.01460 = kritischste Quelle im gesamten Bericht, "SOFORT"-Einstufung).

```rust
// crates/memfuse-router/src/profile.rs [ERWEITERT — P-SOFORT-2]

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash)]
pub struct SlmProfile {
    pub name: String,
    pub mcp_endpoint: String,
    pub domain_communities: std::collections::HashSet<u64>,
    pub token_budget: TokenBudget,
    pub min_relevance_score: f32,
    
    /// [NEU P-SOFORT-2] Pflichtfeld für P8-Kalibrierungs-Integrität.
    /// RouterEngine MUSS bei Fingerprint-Wechsel automatisch
    /// calibration_stats() für dieses Profil zurücksetzen.
    /// Breaking Change für persistierte Kalibrierungszustände → ADR-Pflicht.
    pub config_fingerprint: ConfigFingerprint,
}

// crates/memfuse-router/src/router.rs [ERWEITERT]

impl RouterEngine {
    /// [ERWEITERT] Bei jedem Profil-Update: ConfigFingerprint-Vergleich erzwungen.
    /// Falls Fingerprint geändert: calibrator.invalidate_on_config_change() PFLICHT (P8).
    pub async fn update_profiles(&mut self, profiles: Vec<SlmProfile>) -> Result<()> {
        for profile in &profiles {
            if let Some(old_profile) = self.profiles.get(&profile.name) {
                if old_profile.config_fingerprint != profile.config_fingerprint {
                    // P8-Pflicht: Kalibrierung sofort invalidieren
                    if let Some(calibrator) = self.calibrators.get_mut(&profile.name) {
                        calibrator.invalidate_on_config_change(profile.config_fingerprint.clone());
                    }
                    tracing::warn!(
                        "Profile '{}' config fingerprint changed — calibration reset forced (P8)",
                        profile.name
                    );
                }
            }
        }
        // Profile aktualisieren
        for profile in profiles {
            self.profiles.insert(profile.name.clone(), profile);
        }
        Ok(())
    }
    
    /// [NEU] Abstention-Pfad (P11): Bei fehlender Kalibrierung KEINE geratene
    /// Routing-Entscheidung — zwingend an stärkstes verfügbares Modell eskalieren.
    /// Referenz: RACER (arXiv:2603.06616) bestätigt dieses Pattern als SOTA.
    pub async fn route(&self, request: &RoutingRequest) -> RoutingDecision {
        let profile = self.select_profile(request);
        
        let calibrator = self.calibrators.get(&profile.name).unwrap();
        
        if !calibrator.is_calibrated() {
            // Abstention: Eskalation an stärkstes Modell, nicht Raten
            return RoutingDecision::Escalate {
                reason: EscalationReason::CalibrationIncomplete {
                    samples_seen: calibrator.observation_count() as u32,
                    samples_required: self.warmup_window,
                },
                target_profile: self.strongest_profile(),
            };
        }
        
        // Normale Routing-Logik
        todo!()
    }
    
    /// [NEU] Temperatur-Lock während Warmup.
    /// Temperaturänderung während des Warmup-Fensters setzt Zähler zurück.
    pub fn set_temperature(&mut self, profile_name: &str, temperature: f32) -> Result<()> {
        if let Some(profile) = self.profiles.get_mut(profile_name) {
            let new_temp_bits = temperature.to_bits();
            if new_temp_bits != profile.config_fingerprint.temperature_bits {
                // Neue Temperatur → neuer Fingerabdruck → Reset erzwingen
                profile.config_fingerprint.temperature_bits = new_temp_bits;
                if let Some(calibrator) = self.calibrators.get_mut(profile_name) {
                    calibrator.invalidate_on_config_change(profile.config_fingerprint.clone());
                }
            }
        }
        Ok(())
    }
}

// crates/memfuse-router/src/lyapunov.rs [NEU — F-11]

/// Lyapunov-Drift-Wächter.
/// 
/// PROBLEM: ConfigFingerprint ist REAKTIV — erkennt Konfigurationsänderungen.
/// Was wenn sich die INPUT-VERTEILUNG ändert (Nutzer fragt plötzlich andere Themen)?
/// Der Fingerprint bleibt unverändert — stilles Fehlverhalten.
/// 
/// LÖSUNG: Proaktiver Drift-Detektor über Non-Conformity-Score-Verteilung.
/// 
/// MATHEMATISCHES MODELL:
/// N_t = Non-Conformity-Score-Verteilung im Sliding Window t (aus ConfidenceMetrics)
/// D_t = KL(N_t || N_baseline)   [KL-Divergenz von Kalibrierungs-Baseline]
/// λ_t = (1/w) * Σ log|D_{t-i+1} / D_{t-i}|   [diskrete Lyapunov-Schätzung]
/// 
/// Trigger wenn λ_t > 0 für ≥ w aufeinanderfolgende Fenster:
/// → Proaktive Re-Kalibrierung mit Grund "distributional_drift"
pub struct LyapunovDriftWatcher {
    window_size: usize, // Default: 20 Fenster
    divergence_history: VecDeque<f32>,
    baseline_distribution: Vec<f32>, // Non-Conformity-Scores während Kalibrierung
}

impl LyapunovDriftWatcher {
    pub fn update(&mut self, current_scores: &[f32]) -> LyapunovResult {
        let d_t = self.kl_divergence_from_baseline(current_scores);
        self.divergence_history.push_back(d_t);
        
        if self.divergence_history.len() > self.window_size {
            self.divergence_history.pop_front();
        }
        
        if self.divergence_history.len() < self.window_size {
            return LyapunovResult::InsufficientData;
        }
        
        let lambda = self.compute_lyapunov_exponent();
        
        if lambda > 0.0 {
            LyapunovResult::DriftDetected {
                lyapunov_exponent: lambda,
                reason: EscalationReason::DistributionalDrift {
                    kl_divergence: d_t,
                    lyapunov_exponent: lambda,
                },
            }
        } else {
            LyapunovResult::Stable { lyapunov_exponent: lambda }
        }
    }
    
    fn compute_lyapunov_exponent(&self) -> f32 {
        // λ_t = (1/w) * Σ_{i=1}^{w} log|D_{t-i+1} / D_{t-i}|
        let w = self.divergence_history.len() as f32;
        let history: Vec<f32> = self.divergence_history.iter().copied().collect();
        
        let sum: f32 = history.windows(2)
            .map(|w| {
                let ratio = w[1] / w[0].max(1e-10); // Division-by-zero schutz
                ratio.abs().ln()
            })
            .sum();
        
        sum / w
    }
    
    fn kl_divergence_from_baseline(&self, current: &[f32]) -> f32 {
        // KL(P || Q) = Σ P(x) * log(P(x)/Q(x))
        // Approximation via Histogram-Binning (M=10 Bins)
        todo!()
    }
}

#[derive(Debug)]
pub enum LyapunovResult {
    Stable { lyapunov_exponent: f32 },
    DriftDetected { lyapunov_exponent: f32, reason: EscalationReason },
    InsufficientData,
}
```

### §6.3 memfuse-candle — Native Pure-Rust-Inferenz

**Strategische Bedeutung:** Ohne diesen Adapter ist "Sovereign Core" eine Marketingaussage ohne Code-Nachweis (P7-Verstoß).

**Ökosystem-Befund (Forschungssynthese v2):** Folgende Projekte sind als Blaupause zu nutzen (nicht nachbauen):
- **candelabra** (0.2.0): GGUF LLAMA/Qwen3, Desktop-fokussiert, HF-Hub-Integration → GGUF-Loader-Referenz
- **Crane** (Aug. 2026 aktiv): ROCm+CUDA+Metal, GGUF, Q4/Q8 KV-Quant, 5–7× Prefill-Speedup
- **OxiLLaMa**: 20 Architekturen, Pure-Rust ohne FFI → Architektur-Referenz
- **mistral.rs**: PagedAttention in Rust → Alternative Strategie B

**Strategieentscheidung (§6.1.1 aus spec_v21):**
- **Strategie A** (empfohlen für KV-Bridge-Pfad): Eigener Trait-Adapter um `candle-core`/`candle-transformers` mit candelabra als GGUF-Loader-Referenz. Gibt RoPE-Shift-Zugriff auf Modell-interne Gewichte (Voraussetzung für §7.1 KV-Bridge).
- **Strategie B** (schneller für Sovereign-Core ohne KV-Bridge): `mistral.rs`-Backend hinter `LlmTextGenerator`-Trait. Kein eigener GGUF-Loader, kein Attention-Level-Eingriff.

Beide Strategien können über Feature-Flags parallel existieren: `candle-native` (A) vs. `candle-mistralrs` (B).

```rust
// crates/memfuse-candle/Cargo.toml
// [dependencies]
// candle-core = "0.6"
// candle-nn = "0.6"
// candle-transformers = "0.6"
// memfuse-core = { workspace = true }
// memfuse-crypto = { workspace = true }  // Für ModelFingerprint
// [features]
// cuda = ["candle-core/cuda"]
// metal = ["candle-core/metal"]
// mistralrs-backend = ["mistralrs"]

// crates/memfuse-candle/src/lib.rs

pub enum CandleQuantization {
    Q4KM,   // 4-bit k-means, ~4GB RAM für 7B-Modell
    Q8_0,   // 8-bit, ~8GB RAM
    F16,    // Half-precision, ~14GB RAM
}

pub enum InferenceBackend {
    Ollama { model: String, base_url: String },
    #[cfg(feature = "candle-native")]
    Candle { model_path: PathBuf, quantization: CandleQuantization },
    #[cfg(feature = "candle-mistralrs")]
    MistralRs { model_path: PathBuf, quantization: CandleQuantization },
}

pub struct CandleLlmClient {
    // Strategie A: candle-core Tensoren
    device: candle_core::Device,
    model: Arc<Mutex<Box<dyn CandleModel>>>, // LLAMA/Qwen/Mistral-Impl
    fingerprint: ModelFingerprint,
    tokenizer: tokenizers::Tokenizer,
}

/// Native async fn — KEIN #[async_trait] (AFIT-Migration §6.9 bereits berücksichtigt)
impl LlmTextGenerator for CandleLlmClient {
    async fn generate_text(&self, _model: &str, prompt: &str) -> Result<String> {
        todo!("GGUF-basierte Textgenerierung")
    }
    
    /// [KRITISCH für GASP §7.3] Log-Likelihood ist mit candle-core direkt verfügbar.
    /// (Im Gegensatz zu Ollama-HTTP, das keine Logit-API hat)
    async fn log_likelihood(&self, _model: &str, _prompt: &str, continuation: &str) -> Result<Option<f64>> {
        // Native Logit-Zugriff via candle-core — ermöglicht GASP Post-Hoc-Validator
        todo!()
    }
    
    fn config_fingerprint(&self) -> ConfigFingerprint {
        ConfigFingerprint {
            model_id: self.fingerprint.model_id.clone(),
            quantization: Some(self.fingerprint.quantization.clone()),
            prompt_template_hash: [0u8; 32], // Wird vom Caller gesetzt
            temperature_bits: 1.0_f32.to_bits(), // Default-Temperatur
        }
    }
}

// crates/memfuse-candle/src/model_registry.rs

pub struct ModelRegistry;

impl ModelRegistry {
    /// Berechnet ModelFingerprint: SHA-256(Gewichts-Blob || Quantisierungs-String).
    /// Stellt sicher: Q4 ≠ Q8 bei identischem model_id (P8-Konsequenz).
    pub fn compute_fingerprint(
        model_path: &Path,
        quantization: &CandleQuantization,
    ) -> Result<ModelFingerprint> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        // Hashe das Gewichts-Blob
        let mut f = File::open(model_path)?;
        std::io::copy(&mut f, &mut hasher)?;
        // UND den Quantisierungsgrad (P8: Q4 ≠ Q8)
        hasher.update(format!("{:?}", quantization).as_bytes());
        
        Ok(ModelFingerprint {
            hash: hasher.finalize().into(),
            model_id: model_path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            quantization: format!("{:?}", quantization),
        })
    }
}
```

---

## §7 Layer 4 — Integrations-Grenzschicht

### §7.1 KV-Cache-Bridge mit KV Packet

**Wissenschaftliche Basis:** KV Packet (arXiv:2604.13226, Chen et al., TU München/TU Darmstadt/Zhejiang).

**Kernproblem gelöst:** KV-Zustände sind positionsabhängig. KV Packet löst dies via lightweight trainierte Adapter-Tokens (Header H, Trailer T), die Grenzartefakte bei der Konkatenation absorbieren.

**Performance (Llama-3.1-8B / Qwen-3-4B):**
- TTFT-Reduktion NIAH: 19.45× (vs. CacheBlend SOTA: ~3×)
- TTFT-Reduktion Multi-Hop: 5.81×
- FLOPs relativ: 6.5×10⁻⁶ (vs. CacheBlend = 1.0)
- F1 Biography: 0.96 (vs. Full Recompute: 0.98)

**Sicherheitsanforderungen (P9):**
- arXiv:2510.17098 (MTI-Angriff): KV-Segmente nicht im Klartext auf Disk/VRAM
- arXiv:2508.09442 (Inversion): Mandantenbasierte Isolierung zwingend
- Zeroize-on-Evict: Nach LRU-Eviction kein Restdaten im Speicher

```rust
// crates/memfuse-kv-bridge/src/lib.rs [NEU]

/// Vorberechnetes, verschlüsseltes KV-Cache-Segment für ein Dokument.
/// 
/// SICHERHEITSINVARIANTE (P9):
/// encrypted_layers liegen NUR in verschlüsselter Form auf persistentem Speicher.
/// Entschlüsselung erfolgt ausschließlich im aktiven Inferenzprozess, niemals auf Disk.
#[derive(Debug, Clone)]
pub struct KvSegment {
    pub doc_id: DocId,
    pub tenant_id: TenantId,
    /// SHA-256(Gewichts-Blob || Quantisierungsgrad) — Q4 ≠ Q8 (P8-Konsequenz)
    pub model_fingerprint: ModelFingerprint,
    /// KV-Tensoren, AES-256-GCM-SIV-verschlüsselt.
    /// Schlüssel: KeyManager::derive_file_key(tenant_id || doc_id)
    /// Wiederverwendet memfuse-crypto::KeyManager (kein zweiter Krypto-Stack).
    pub encrypted_layers: Vec<EncryptedKvLayer>,
    pub created_at_tx: TxId,  // ADR-016: TxId statt SystemTime
    pub vram_bytes: usize,     // Für LRU-Eviction-Budget
    /// RoPE-Positions-Offset bei Vorberechnung.
    /// Wird beim inject() via RoPE-Rotation korrigiert.
    pub rope_offset: u32,
}

#[derive(Debug, Clone)]
pub struct EncryptedKvLayer {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

pub trait KvCacheProvider: Send + Sync {
    /// Berechnet und cached KV-Segmente für eine Batch von DocIds.
    /// Idempotent: bereits gecachte, valide Segmente werden übersprungen.
    /// 
    /// KV Packet Adapter-Training (einmalig offline, dann frozen):
    /// 1. Für jedes Dokument: KV(H + doc + T) berechnen (H = Header-Token, T = Trailer-Token)
    /// 2. AES-256-GCM-SIV verschlüsseln mit tenant-abgeleitetem Schlüssel
    /// 3. Auf Disk persistieren (verschlüsselt — P9)
    async fn ensure_cached(&self, tenant_id: TenantId, doc_ids: &[DocId]) -> Result<()>;
    
    /// Entschlüsselt Segmente NUR im aktiven Inferenzprozess (niemals Klartext auf Disk).
    /// Injiziert mit RoPE-Positionsalignment (O(d) via Element-weise Rotation).
    /// 
    /// RoPE-Rotation: k_i^(S+Δ) = R_{Θ,Δ} * k_i^S
    /// Kompatibel mit Llama 3.x, Qwen 3, Mistral (alle RoPE-Modelle).
    /// 
    /// None = stale (Modell/Quantisierungsgrad gewechselt) ODER Tenant-Mismatch
    ///        → transparenter Text-Fallback, KEIN Fehler (P11-konform)
    async fn inject(
        &self,
        tenant_id: TenantId,
        doc_ids: &[DocId],
        ctx: &mut InferenceContext,
    ) -> Result<Option<InjectionResult>>;
    
    /// LRU-Eviction MIT Zeroize-on-Evict (P9-Pflicht).
    /// 
    /// KRITISCH (D4-Korrektur aus v2.1): evict_lru() DARF den Inferenz-Hot-Path
    /// NICHT blockieren. Zeroize-Operation läuft auf dediziertem Eviction-Worker-Thread.
    /// 
    /// Implementierung:
    /// - evict_lru() gibt LRU-Kandidaten zurück (non-blocking, instant)
    /// - Eviction-Worker-Thread führt Zeroize durch (async, kein Block auf Inferenz)
    /// - VRAM-Budget-Check: falls > 80% → Eviction triggern
    async fn evict_lru_nonblocking(&self, target_free_bytes: usize) -> Result<Vec<DocId>>;
    
    /// Sofortige Freigabe aller Segmente eines Tenants.
    /// Pflicht bei Tenant-Kontext-Wechsel — kein Segment überlebt diesen Wechsel.
    async fn purge_tenant(&self, tenant_id: TenantId) -> Result<usize>;
}

// crates/memfuse-kv-bridge/src/security.rs

/// Verschlüsselt KV-Layer-Tensoren für Disk-Persistenz.
/// Wiederverwendet KeyManager aus memfuse-crypto (kein zweiter Krypto-Stack).
pub struct KvSegmentCipher {
    key_manager: Arc<KeyManager>,
}

impl KvSegmentCipher {
    pub fn encrypt_layer(
        &self,
        tenant_id: TenantId,
        doc_id: DocId,
        layer_data: &[f32], // Raw KV-Tensor-Daten
    ) -> Result<EncryptedKvLayer> {
        // Schlüssel-Ableitung: HKDF mit tenant_id || doc_id als Info-Label
        // Wiederverwendet KeyManager::derive_file_key() (bereits in memfuse-crypto)
        let key = self.key_manager.derive_kv_segment_key(tenant_id, doc_id)?;
        let plaintext: Vec<u8> = layer_data.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        self.key_manager.encrypt_auto_nonce(&key, &plaintext)
            .map(|(ciphertext, nonce)| EncryptedKvLayer { ciphertext, nonce })
    }
    
    /// Entschlüsselt NUR im aktiven Inferenzprozess.
    /// Nach Nutzung: Tensor-Speicher mit Zeroize überschreiben (P9).
    pub fn decrypt_layer(
        &self,
        tenant_id: TenantId,
        doc_id: DocId,
        encrypted: &EncryptedKvLayer,
    ) -> Result<ZeroizeOnDropVec<f32>> {
        // ZeroizeOnDropVec: Überschreibt bei Drop automatisch (analog KeyManager::emergency_wipe)
        todo!()
    }
}

/// Zeroize-on-Evict Worker (D4-Fix: nicht im Hot-Path).
/// Läuft auf dediziertem tokio::task mit niedrigerer Priorität.
pub struct EvictionWorker {
    pending_evictions: Arc<Mutex<VecDeque<EvictionTask>>>,
}

pub struct EvictionTask {
    pub vram_address: usize,
    pub byte_count: usize,
    pub tenant_id: TenantId,
    pub doc_id: DocId,
}

impl EvictionWorker {
    pub async fn run(&self) {
        loop {
            let task = self.pending_evictions.lock().pop_front();
            if let Some(task) = task {
                // SICHERHEITS-KRITISCH: Speicher mit Nullen überschreiben
                // analog KeyManager::emergency_wipe() (crypto.rs:322)
                unsafe {
                    // SAFETY: vram_address ist verifiziert gültig und nicht mehr im Inferenzpfad
                    std::ptr::write_bytes(task.vram_address as *mut u8, 0u8, task.byte_count);
                }
                tracing::debug!("Zeroized KV segment: tenant={}, doc={}", task.tenant_id.0, task.doc_id.0);
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}
```

### §7.2 GASP Post-Hoc-Halluzinations-Validator

**Wissenschaftliche Basis:** GASP (arXiv:2607.04223, Juli 2026, Bouke, Multimedia University Melaka). Grounding Sensitivity = Log-Likelihood-Kollaps bei Entfernung des stützenden Chunks. AUC 0.73 (Response-Level), 0.67 (Span-Level). Training-frei.

**Wichtige Präzisierung:** GASP ist KOMPLEMENTÄR zum bestehenden präventiven Halluzinations-Guard in `client.rs` — kein Ersatz. Präventiv (Prompt-Constraint + Zitierpflicht) + Post-Hoc (GASP Log-Likelihood-Analyse) = vollständiges Halluzinations-Management.

**Abhängigkeit:** Log-Likelihood-Zugriff (`LlmTextGenerator::log_likelihood()`). Verfügbar bei `memfuse-candle` (native Logit-Zugriff). NICHT verfügbar bei Ollama-HTTP (keine Logit-API). → GASP ist deshalb strategisch mit `memfuse-candle` verbunden.

```rust
// crates/memfuse-db/src/collection/gasp.rs [NEU]

/// GASP (Grounding-Aware Sensitivity by Perturbation) Post-Hoc-Validator.
/// 
/// ALGORITHMUS:
/// Für jeden Antwortsatz s und jeden Retrieved Chunk C_i:
/// GroundingSensitivity(s, C_i) = log P(s|C_full) − log P(s|C_full \ C_i)
/// 
/// Halluzinierter Satz:  ΔLL ≈ 0  (LLM ignoriert Kontext)
/// Grundierter Satz:     ΔLL >> 0 (LLM hängt vom Chunk ab)
/// 
/// ENTSCHEIDUNG: Training-frei → kein zusätzliches Modell nötig.
/// Nur Log-Likelihood-Zugriff auf das generative LLM.
pub struct GaspValidator {
    /// Nur verfügbar wenn Backend Log-Likelihood unterstützt (memfuse-candle)
    llm: Option<Arc<dyn LlmTextGenerator>>,
}

#[derive(Debug, Clone)]
pub struct GaspResult {
    /// Grounding-Sensitivity pro Antwortsatz × Chunk-Paar
    pub sensitivities: Vec<SentenceGroundingSensitivity>,
    /// Aggregierter Response-Level Score (AUC-Ziel: 0.73)
    pub response_grounding_score: f32,
    /// Span-Level: Welche Antwortsätze sind nicht grundiert?
    pub ungrounded_spans: Vec<SpanRange>,
}

#[derive(Debug, Clone)]
pub struct SentenceGroundingSensitivity {
    pub sentence_range: SpanRange,
    pub supporting_chunk_id: Option<DocId>,
    pub grounding_sensitivity: f32, // ΔLL
    pub is_grounded: bool, // ΔLL > threshold (Default: 0.1)
    pub jsd_score: f32, // Jensen-Shannon-Divergenz als zusätzliches Signal
}

impl GaspValidator {
    /// Prüft eine generierte Antwort auf Halluzinationen.
    /// 
    /// None wenn log_likelihood() nicht verfügbar (Ollama-Backend).
    /// In diesem Fall: nur präventiver Prompt-Guard aktiv.
    pub async fn validate(
        &self,
        response: &str,
        chunks: &[RetrievedChunk],
        model: &str,
        full_prompt: &str,
    ) -> Result<Option<GaspResult>> {
        let llm = match &self.llm {
            Some(l) => l,
            None => return Ok(None), // Graceful degradation bei Ollama-Backend
        };
        
        // Baseline: log P(s|C_full)
        let baseline_ll = llm.log_likelihood(model, full_prompt, response).await?;
        
        let mut sensitivities = Vec::new();
        
        for (i, sentence) in split_sentences(response).iter().enumerate() {
            for chunk in chunks {
                // Perturbierter Kontext: C_full ohne aktuellen Chunk
                let perturbed_prompt = build_perturbed_prompt(full_prompt, chunk);
                let perturbed_ll = llm.log_likelihood(model, &perturbed_prompt, sentence).await?;
                
                let delta_ll = baseline_ll.unwrap_or(0.0) - perturbed_ll.unwrap_or(0.0);
                
                sensitivities.push(SentenceGroundingSensitivity {
                    sentence_range: sentence_range(i, sentence),
                    supporting_chunk_id: Some(chunk.doc_id),
                    grounding_sensitivity: delta_ll as f32,
                    is_grounded: delta_ll > 0.1,
                    jsd_score: compute_jsd(baseline_ll, perturbed_ll),
                });
            }
        }
        
        let ungrounded = sensitivities.iter()
            .filter(|s| !s.is_grounded)
            .map(|s| s.sentence_range.clone())
            .collect();
        
        let response_score = sensitivities.iter()
            .map(|s| s.grounding_sensitivity)
            .sum::<f32>() / sensitivities.len() as f32;
        
        Ok(Some(GaspResult {
            sensitivities,
            response_grounding_score: response_score,
            ungrounded_spans: ungrounded,
        }))
    }
}
```

### §7.3 MCP JSON-RPC 2.0 (produktionsreif, Dokumentation ausstehend)

**Verifiziert:** `memfuse-mcp/src/protocol.rs` — vollständiges JSON-RPC 2.0 stdio, Prompt-Injection-Detection, E2E-Test (#1613).

**Ausstehend:** Vollständige Onboarding-Dokumentation für Claude Desktop, Cursor, VS Code. ADR für MCP-Schema-Versionierung.

```rust
// Interface-Übersicht (produktionsreif)
pub struct McpServer {
    protocol: McpProtocol,
    sandbox: McpSandbox,
    injection_detector: PromptInjectionDetector,
}

// Exponierte MCP-Tools (Zielzustand):
// - memory/search: Hybrid-Suche über Collection
// - memory/ingest: Dokument-Ingestion
// - memory/branch: Session-DAG-Branch-Operationen
// - memory/provenance: Herkunftskette für Suchergebnis
// - memory/stats: Systemzustand (Kalibrierung, Cache-Nutzung)
```

---

## §8 Layer 5 — Evaluation & Benchmarking

### §8.1 LongMemEval & LoCoMo Integration

**SOTA 2026 (LycheeMemory V2, arXiv:2608.12990):**
- LoCoMo: 89.22%
- LongMemEval-S: 92.20%
- LongMemEval-M: ~80%

**MemFuse Zielwerte (aktualisiert aus v2.1):**
- LoCoMo: H3 > 80% (Einstieg), H5 > 89% (SOTA-Parität)
- LongMemEval-S: H3 > 85% (Einstieg), H5 > 92% (SOTA-Parität)

```rust
// crates/memfuse-bench/src/long_mem_eval.rs [NEU]

pub struct LongMemEvalHarness {
    tasks: Vec<LongMemEvalTask>,
    evaluator: LongMemEvalEvaluator,
    db: Arc<MemFuseDb>,
}

/// 5 Task-Typen aus LongMemEval (arXiv:2410.10813, Hu et al., ICLR 2025)
#[derive(Debug, Clone)]
pub enum TaskType {
    InformationExtraction,   // IE: Einzel-Fakten abrufen
    MultiSessionReasoning,   // MR: Über Sessions verketten
    KnowledgeUpdate,         // KU: Veraltete Fakten erkennen
    TemporalReasoning,       // TR: Zeitliche Abfolgen verstehen
    Abstain,                 // ABS: Bei Unsicherheit nicht halluzinieren
}

pub struct LongMemEvalTask {
    pub task_type: TaskType,
    pub session_history: Vec<SessionTurn>,
    pub query: String,
    pub expected_answer: String,
    pub expected_abstain: bool,
}

impl LongMemEvalHarness {
    /// Führt alle 500 Tasks durch und berechnet Gesamtgenauigkeit.
    /// Ergebnis wird in memfuse-bench/results/ gespeichert und in README verlinkt.
    pub async fn run_full_eval(&self) -> LongMemEvalResult {
        todo!()
    }
    
    pub async fn run_task_type(&self, task_type: TaskType) -> f32 {
        todo!()
    }
}

// crates/memfuse-bench/src/locomo.rs [NEU]

pub struct LoCoMoHarness {
    conversations: Vec<LoCoMoConversation>, // 1540 Fragen, ~16k Token/Konv.
    db: Arc<MemFuseDb>,
}
```

---

## §9 Physio-Feature-Katalog (F-01 bis F-11, vollständig)

Alle Features sind Feature-Flag-geschützt (`physio-*`). Defaults sind P1-sicher (Zero-IT-Setup-Nutzer sieht keine Änderung). Alle DREI Kriterien aus PRD §1 erfüllt: (a) geschlossene Formel, (b) bestehende Datenstruktur, (c) analogie-unabhängiges Akzeptanzkriterium.

### F-01: Freie-Energie-Gedächtnis-Thermostat
→ Vollständig spezifiziert in §5.6. Feature-Flag: `physio-thermostat`. Aufwand: 1.5 Wochen.

### F-02: Nukleations-Trigger für HNSW — VETO
→ Verworfen (§0.2 VETO). HNSW ist global verschränkter Graph — partielle Rebuilds zerstören Nachbarschaftsbeziehungen. Keine Alternative empfohlen. ADR-Eintrag in DECISIONS.md als "Permanently Rejected".

### F-03: Synaptisch-Stigmergische Kantenverstärkung (5. Fusionssignal)

**Naturvorbild:** Hebbian Learning ("Neurons that fire together, wire together") + Homöostatische Skalierung (Turrigiano) + Stigmergie (Ameisenkolonien).

**Abgrenzung zu MinnsDB:** MinnsDB löst semantische Kantensuche via zweitem HNSW-Index (hohe RAM-Kosten). F-03 löst Kantenrelevanz über Nutzungsdynamik — nur zwei `f32`-Felder pro Kante.

```rust
// crates/memfuse-graph/src/synaptic.rs [NEU — F-03]

/// MATHEMATISCHES MODELL:
/// Hebbian-Update bei gemeinsamer Aktivierung:
/// w_ij(t+1) = w_ij(t) + η * co_activation(i,j,t) − δ * w_ij(t)  [δ = passiver Zerfall]
/// 
/// Homöostatische Skalierung (verhindert Hub-Explosion):
/// Σ_j w_ij ≤ W_max für jeden Knoten i
/// → bei Überschreitung: alle ausgehenden Kanten von i proportional herunterskalieren
/// 
/// Stigmergisches Update bei bestätigtem Retrieval-Erfolg:
/// τ_ij(t+1) = (1−ρ) * τ_ij(t) + Q / path_length  [ρ = Verdunstungsrate]
/// 
/// SynapticScore = α * w_ij + (1−α) * τ_ij  [fließt als 5. RRF-Signal ein]

pub struct SynapticEdgeWeights {
    /// Hebbian-Gewicht (neuronale Verstärkung)
    pub hebbian: f32,
    /// Pheromon-Gewicht (stigmergische Verstärkung)
    pub pheromone: f32,
}

pub struct SynapticConfig {
    pub eta: f32,    // Lernrate, Default: 0.01
    pub delta: f32,  // Passiver Zerfall, Default: 0.001
    pub rho: f32,    // Verdunstungsrate, Default: 0.05
    pub q: f32,      // Belohnungskonstante, Default: 1.0
    pub alpha: f32,  // Mischgewicht, Default: 0.5
    pub w_max_factor: f32, // Homöostase-Obergrenze als Vielfaches des Median-Gewichts, Default: 5.0
}

/// ARCHITEKTUR: WAL-Batching für Write-Heavy CSR.
/// CSR ist Read-Heavy optimiert — direkte Writes im Hot-Path = Contention.
/// LÖSUNG: Updates in DashMap (lock-frei) sammeln, asynchron via CompactionEngine flushen.
pub struct SynapticUpdateBuffer {
    pending_updates: dashmap::DashMap<(NodeIdx, NodeIdx), SynapticDelta>,
}

pub struct SynapticDelta {
    pub hebbian_delta: f32,
    pub pheromone_delta: f32,
}

impl SynapticUpdateBuffer {
    /// Aufgerufen im Hot-Path bei erfolgreicher Retrieval.
    /// Non-blocking (DashMap, atomare Updates).
    pub fn record_co_activation(&self, node_a: NodeIdx, node_b: NodeIdx, co_activation: f32) {
        // lock-frei via DashMap
        todo!()
    }
    
    /// Aufgerufen von CompactionEngine im Background-Worker.
    /// Batched Flush in CSR-Graph (via WAL, P3-konform).
    pub async fn flush_to_csr(&self, csr: &mut CsrGraph, wal: &mut WalWriter) -> Result<()> {
        // 1. Alle pending_updates in einen WAL-Eintrag batchen
        // 2. Nach WAL-Commit: CSR-Update
        todo!()
    }
    
    /// Homöostatische Normalisierung nach Flush.
    /// Verhindert das in §2.2 (MinnsDB-Analyse) genannte Hub-Problem.
    fn apply_homeostatic_scaling(&self, csr: &mut CsrGraph, config: &SynapticConfig) {
        // Median-Gewicht berechnen
        // Knoten mit Σ_j w_ij > w_max_factor * median → proportional skalieren
        todo!()
    }
}

/// AKZEPTANZKRITERIUM:
/// Nach 30 Tagen simulierter Nutzung (Replay-Testset) muss SynapticScore
/// als 5. Signal die RRF-Recall@10 um ≥ 3pp verbessern,
/// ohne dass ein Knoten mehr als W_max Gesamtgewicht erreicht (Homöostase-Test).
```

### F-04: Immunologisches Kontradiktions-Gedächtnis

**Naturvorbild:** Klonale Selektion (Adaptive Immunsystem) — Antikörper mit wachsender Avidität, Gedächtniszellen mit langsamem Zerfall.

**Abgrenzung zu YantrikDB:** YantrikDB's `think()` scannt reaktiv nach Widersprüchen. F-04 ist präventiv (prüft VOR Commit) und führt Populations-Dynamik (Avidität) statt binäres Flag ein.

**Trigger-Pfad (Gegenprüfung §4):** Schließt die fehlende Verbindung zwischen Supersedes-Event und CSR-Tombstone.

```rust
// crates/memfuse-graph/src/immune.rs [NEU — F-04]

/// MATHEMATISCHES MODELL:
/// Bei bestätigtem Widerspruch (Supersedes-Event mit Contradiction-Flag):
/// Antikörper_k = { centroid: embedding(widerlegte_aussage), avidity: a_k }
/// 
/// Bei neuer Ingestion-Kandidat mit Embedding e:
/// affinity(e) = max_k cos_sim(e, centroid_k)
/// 
/// if affinity > θ_reject:     auto-reject + Audit-Log
/// if θ_flag < affinity ≤ θ_reject: flag-for-review + Warn-Provenienz
/// else:                        normale Ingestion
/// 
/// Klonale Expansion: a_k(t+1) = a_k(t) + β  (deckelt bei a_max)
/// Immunologisches Vergessen: a_k(t) = a_k(0) * exp(−λ_immun * t)
///                            λ_immun ≪ λ_regulär (Faktor 1/10 default)

/// PERSISTENZ: Kompakte, lineare Datenstruktur (KEIN eigener ANN-Index).
/// Antikörper-Population ist erwartet klein (hundert bis niedriger vierstelliger Bereich).
/// Lineare Cosine-Suche reicht, HNSW wäre Over-Engineering.
pub struct ImmunMemory {
    antibodies: Vec<Antibody>,
    config: ImmuneConfig,
}

pub struct Antibody {
    pub centroid: Vec<f32>,          // Embedding der widerlegten Aussage
    pub avidity: f32,                // Bindungsstärke, steigt mit Wiederholung
    pub entity_pattern: (String, String), // (Subjekt, Relation) — Muster des Widerspruchs
    pub created_at_tx: TxId,
    pub last_reinforced_at_tx: TxId,
}

pub struct ImmuneConfig {
    pub theta_reject: f32,    // Default: 0.9 (oberhalb: auto-reject)
    pub theta_flag: f32,      // Default: 0.7 (zwischen: flag-for-review)
    pub beta: f32,            // Avidität-Inkrement, Default: 0.1
    pub a_max: f32,           // Avidität-Maximum, Default: 1.0
    pub lambda_immun: f32,    // Vergessensrate Immun, Default: 0.0001 (sehr langsam)
    pub lambda_regular: f32,  // Vergessensrate normal, Default: 0.001 (10× schneller)
}

#[derive(Debug, Clone)]
pub enum IngestionVerdict {
    Accepted,
    Rejected { antibody_id: usize, affinity: f32 },
    FlaggedForReview { antibody_id: usize, affinity: f32 },
}

impl ImmunMemory {
    /// TRIGGER-PFAD (Gegenprüfung §4 — dieser war die fehlende Verbindung):
    /// 1. Supersedes-Event in QueryBuilder mit Contradiction-Flag
    /// 2. → cascade_tombstone_superseded_edges() (§4.5)
    /// 3. → create_or_reinforce_antibody() (diese Funktion)
    pub fn create_or_reinforce_antibody(
        &mut self,
        embedding: &[f32],
        entity_pattern: (String, String),
        current_tx: TxId,
    ) {
        // Suche nach existierendem Antikörper für dieses Muster
        let existing = self.antibodies.iter_mut()
            .find(|ab| {
                cosine_similarity(embedding, &ab.centroid) > 0.85
                    && ab.entity_pattern == entity_pattern
            });
        
        match existing {
            Some(ab) => {
                // Klonale Expansion: Avidität erhöhen
                ab.avidity = (ab.avidity + self.config.beta).min(self.config.a_max);
                ab.last_reinforced_at_tx = current_tx;
            }
            None => {
                // Neuer Antikörper
                self.antibodies.push(Antibody {
                    centroid: embedding.to_vec(),
                    avidity: self.config.beta,
                    entity_pattern,
                    created_at_tx: current_tx,
                    last_reinforced_at_tx: current_tx,
                });
            }
        }
    }
    
    /// Prüft neuen Ingestion-Kandidaten BEVOR er committed wird.
    /// Integration: Aufruf in ingestion_pipeline.rs vor WAL-Commit.
    pub fn check_ingestion(&self, candidate_embedding: &[f32]) -> IngestionVerdict {
        let max_affinity = self.antibodies.iter()
            .map(|ab| cosine_similarity(candidate_embedding, &ab.centroid) * ab.avidity)
            .fold(0.0_f32, f32::max);
        
        if max_affinity >= self.config.theta_reject {
            IngestionVerdict::Rejected { antibody_id: 0, affinity: max_affinity }
        } else if max_affinity >= self.config.theta_flag {
            IngestionVerdict::FlaggedForReview { antibody_id: 0, affinity: max_affinity }
        } else {
            IngestionVerdict::Accepted
        }
    }
    
    /// AKZEPTANZKRITERIUM:
    /// In Testkorpus mit absichtlich eingestreuten, später widerlegten Fakten:
    /// Re-Ingestion-Rate widerlegter Aussagen sinkt um ≥ 90%.
    /// False-Positive-Rate (fälschlich blockierte korrekte Fakten) < 2%.
}
```

### F-05: Zirkadianer REM-Konsolidierungspass
→ Vollständig spezifiziert in §5.5 (SleepCycle NREM+REM). Feature-Flag: `physio-rem-synthesis`. Aufwand: 4 Wochen.

### F-06: Perkolations-Gesundheitsmonitor

**Naturvorbild:** Perkolationstheorie — Netzwerk kollabiert abrupt bei Unterschreitung des Perkolationsschwellenwerts.

```rust
// crates/memfuse-graph/src/percolation.rs [NEU — F-06]

/// MATHEMATISCHES MODELL:
/// φ(t) = |LCC(t)| / |V(t)|   (Anteil des Graphen in der größten zusammenhängenden Komponente)
/// 
/// Schätzung: Periodisches BFS-Sampling auf MVCC-Snapshot (nicht im Hot-Path).
/// MAX_VISITED_NODES = 10.000 Grenze wird wiederverwendet (kein neuer Traversal-Code).
/// 
/// Trigger: falls φ(t) < θ_perc (Default 0.7):
/// → Re-Bonding-Pass: Entity-Resolution auf Randknoten (Degree < Median)
///   NICHT auf gesamten Graphen (Kostenkontrolle).

pub struct PercolationMonitor {
    theta_perc: f32,         // Default: 0.7
    sampling_fraction: f32,  // Default: 0.1 (10% der Knoten als Stichprobe)
}

pub struct PercolationMetrics {
    pub phi: f32,             // Anteil in größter zusammenhängender Komponente
    pub largest_component_size: usize,
    pub total_nodes: usize,
    pub fragmentation_risk: bool, // true wenn φ < θ_perc
    pub computed_at_tx: TxId,
}

impl PercolationMonitor {
    /// Berechnet φ(t) via BFS-Sampling auf MVCC-Snapshot.
    /// LÄUFT NUR auf Snapshot — blockiert NICHT den Inferenz-Hot-Path.
    pub async fn compute_phi(&self, snapshot: &CsrSnapshot) -> PercolationMetrics {
        // BFS via bestehende BFS-Infrastruktur (MAX_VISITED_NODES=10.000)
        // auf einer Stichprobe der Knoten (sampling_fraction)
        todo!()
    }
    
    /// Re-Bonding-Pass: Entity-Resolution auf Randknoten.
    /// Nur wenn φ(t) < θ_perc.
    pub async fn rebond_peripheral_nodes(&self, csr: &mut CsrGraph, llm: &dyn LlmTextGenerator) -> Result<usize> {
        // 1. Randknoten identifizieren: Degree < Median-Degree
        // 2. Entity-Resolution auf Randknoten: suche neue Verbindungen
        // 3. Neue Kanten mit WAL-Provenienz (§4.5 INV-GRAPH-PROV-1)
        todo!()
    }
    
    /// AKZEPTANZKRITERIUM:
    /// In synthetischem Stresstest (kontrolliertes sukzessives Entfernen von Kanten)
    /// muss Monitor den Fragmentierungs-Übergang mit mindestens einer Re-Bonding-Vorlaufzeit
    /// vor vollständiger Fragmentierung erkennen.
}
```

### F-07: Replikator-Dynamik für adaptive RRF-Gewichte

**Naturvorbild:** Evolutionäre Spieltheorie — Anteil einer Strategie wächst proportional zu relativem Erfolg. Entspricht mathematisch Multiplicative Weights Update (bekannte Regret-Bounds).

```rust
// crates/memfuse-db/src/replicator.rs [NEU — F-07]

/// MATHEMATISCHES MODELL:
/// f_i(t) = Anteil in dem Signal i's Top-Kandidat im akzeptierten Kontext landete
///          (aus ProvenanceRecord ableitbar — bereits vorhanden)
/// f̄(t)   = gewichteter Mittelwert aller f_i(t)
/// 
/// w_i(t+1) = w_i(t) * exp(η * (f_i(t) − f̄(t)))  [multiplikatives Update]
/// w_i(t+1) = w_i(t+1) / Σ_j w_j(t+1)              [Normalisierung]
/// w_i(t+1) = max(w_i(t+1), w_min)                  [Homöostase-Bodensatz]
/// 
/// PERSISTENZ: Wiederverwendet persist_calibration_state/load_calibration_state (P10)

pub struct ReplicatorWeightManager {
    weights: HashMap<SignalKind, f32>,
    eta: f32,    // Lernrate, Default: 0.05
    w_min: f32,  // Bodensatz (kein Signal darf auf 0 fallen), Default: 0.05
    outcome_history: VecDeque<HashMap<SignalKind, f32>>, // f_i(t) Historie
}

impl ReplicatorWeightManager {
    /// Aktualisiert Gewichte basierend auf ProvenanceRecord des letzten Retrievals.
    /// Aufgerufen wenn Nutzer-Feedback vorliegt (Zitat, Bewertung, Kontext-Übernahme).
    pub fn update_from_provenance(&mut self, prov: &ProvenanceRecord) {
        // f_i(t) berechnen: War Signal i's Top-Kandidat im finalen Kontext?
        let fi: HashMap<SignalKind, f32> = compute_signal_success_rates(prov);
        let f_bar = fi.values().sum::<f32>() / fi.len() as f32;
        
        // Multiplikatives Update
        for (signal, weight) in self.weights.iter_mut() {
            let fi_signal = fi.get(signal).copied().unwrap_or(0.0);
            *weight *= (self.eta * (fi_signal - f_bar)).exp();
        }
        
        // Normalisierung + Bodensatz
        let sum: f32 = self.weights.values().sum();
        for weight in self.weights.values_mut() {
            *weight = (*weight / sum).max(self.w_min);
        }
        
        // Renormalisieren nach Bodensatz-Anwendung
        let sum: f32 = self.weights.values().sum();
        for weight in self.weights.values_mut() {
            *weight /= sum;
        }
        
        self.outcome_history.push_back(fi);
        if self.outcome_history.len() > 1000 {
            self.outcome_history.pop_front();
        }
    }
    
    /// AKZEPTANZKRITERIUM:
    /// Nach 14 Tagen simuliertem Nutzungs-Replay muss adaptive Gewichtung
    /// statische Baseline-Konfiguration bei Recall@5 um ≥ 2pp übertreffen,
    /// bei stabiler Konvergenz (keine Oszillation — Lyapunov-Test via F-11-Infrastruktur).
}
```

### F-08: PID-Regler für Retrieval-Homöostase
→ Integration in §5.2 (RerankPidController) vollständig spezifiziert. Feature-Flag: `physio-pid-homeostasis`. Aufwand: 1 Woche. **NACH** Deadline-Fix (P-SOFORT-3), nicht davor.

### F-09: Resonanz-Fusion mit Kohärenz-Bonus
→ Vollständig integriert in §5.1 (`apply_coherence_bonus()`). Feature-Flag: `physio-resonance-fusion`. Aufwand: 1 Woche.

### F-10: Osmotischer Wissensaustausch — VETO
→ Verworfen (§0.2 VETO). Bricht TenantId-Isolationsgarantie und DeletionProof-Beweisbarkeit. Keine Alternative empfohlen.

### F-11: Lyapunov-Drift-Wächter für Kalibrierungsstabilität
→ Vollständig integriert in §6.2 (`LyapunovDriftWatcher`). Feature-Flag: `physio-lyapunov-drift`. Aufwand: 2 Wochen. **Abhängig von** `memfuse-calibration` (§3.3).

---

## §10 PhysioScheduler & PhysioConfig

### §10.1 Unified PhysioScheduler

**Risiko aus PRD §6:** "Zu viele Hintergrund-Worker" — F-01, F-03, F-05, F-06, F-11 brauchen periodische Ausführung. Statt sechs unabhängiger tokio-Tasks: ein gemeinsamer, konfigurierbarer Scheduler.

```rust
// crates/memfuse-db/src/physio_scheduler.rs [NEU]

/// Einziger Hintergrund-Scheduler für alle physio-inspirierten Features.
/// Ersetzt sechs potenzielle unabhängige tokio-Tasks durch EINEN konfigurierbaren.
pub struct PhysioScheduler {
    config: PhysioConfig,
    db: Arc<MemFuseDb>,
    enabled_features: PhysioFeatureFlags,
}

pub struct PhysioFeatureFlags {
    pub thermostat: bool,          // F-01
    pub synaptic_edges: bool,      // F-03
    pub rem_synthesis: bool,       // F-05
    pub percolation_monitor: bool, // F-06
    pub replicator_weights: bool,  // F-07
    pub pid_homeostasis: bool,     // F-08
    pub lyapunov_drift: bool,      // F-11
}

impl Default for PhysioFeatureFlags {
    fn default() -> Self {
        // P12: Alle Features DEFAULT-AUS für P1-Nutzer
        Self {
            thermostat: false,
            synaptic_edges: false,
            rem_synthesis: false,
            percolation_monitor: false,
            replicator_weights: false,
            pid_homeostasis: false,
            lyapunov_drift: false,
        }
    }
}

impl PhysioScheduler {
    pub async fn run_loop(&self) {
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.scheduler_tick_secs)
        );
        
        loop {
            interval.tick().await;
            
            // Thermostat-Update (F-01): Schnell, alle Ticks
            if self.enabled_features.thermostat {
                self.run_thermostat_update().await;
            }
            
            // Synaptische Updates flushen (F-03): Batched, alle N Ticks
            if self.enabled_features.synaptic_edges && self.should_flush_synaptic() {
                self.flush_synaptic_updates().await;
            }
            
            // Perkolationsmonitor (F-06): Seltener, auf Snapshot
            if self.enabled_features.percolation_monitor && self.should_check_percolation() {
                self.run_percolation_check().await;
            }
            
            // Lyapunov-Drift (F-11): Jeder Tick, lightweight
            if self.enabled_features.lyapunov_drift {
                self.check_lyapunov_drift().await;
            }
            
            // Sleep-Cycle (NREM+REM, F-05): Nur ohne aktive Sessions
            // → Eigenständiger Scheduler (§5.5) für bessere Trennung
        }
    }
}
```

### §10.2 PhysioConfig — Zentrale Parametrierung

```rust
// crates/memfuse-core/src/physio.rs [NEU]

/// Alle physio-inspirierten Parameter in EINEM Struct.
/// Verhindert "Parameterexplosion" (PRD §6 Selbstkritik).
/// P1-sichere Defaults: 95% der Nutzer müssen nichts anpassen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysioConfig {
    // Scheduler
    pub scheduler_tick_secs: u64,       // Default: 60
    pub synaptic_flush_ticks: u32,      // Default: 10 (flush alle 10 Ticks)
    pub percolation_check_ticks: u32,   // Default: 60 (check jede Stunde)
    
    // F-01: Freie-Energie-Thermostat
    pub thermostat_kappa: f32,          // Default: 2.0
    pub thermostat_w1: f32,             // Default: 0.6 (Speicherdruck-Gewicht)
    pub thermostat_w2: f32,             // Default: 0.4 (Query-Rate-Gewicht)
    
    // F-03: Synaptische Kanten
    pub synaptic_eta: f32,              // Default: 0.01
    pub synaptic_delta: f32,            // Default: 0.001
    pub synaptic_rho: f32,              // Default: 0.05
    pub synaptic_q: f32,                // Default: 1.0
    pub synaptic_alpha: f32,            // Default: 0.5
    pub synaptic_w_max_factor: f32,     // Default: 5.0
    
    // F-04: Immun-Gedächtnis
    pub immune_theta_reject: f32,       // Default: 0.9
    pub immune_theta_flag: f32,         // Default: 0.7
    pub immune_beta: f32,               // Default: 0.1
    pub immune_a_max: f32,              // Default: 1.0
    pub immune_lambda_ratio: f32,       // Default: 0.1 (immun zerfällt 10× langsamer)
    
    // F-06: Perkolationsmonitor
    pub percolation_theta_perc: f32,    // Default: 0.7
    pub percolation_sampling_fraction: f32, // Default: 0.1
    
    // F-07: Replikator-Gewichte
    pub replicator_eta: f32,            // Default: 0.05
    pub replicator_w_min: f32,          // Default: 0.05
    
    // F-08: PID-Homöostat
    pub pid_kp: f32,                    // Default: 0.5 (via Ziegler-Nichols zu tunen)
    pub pid_ki: f32,                    // Default: 0.1
    pub pid_kd: f32,                    // Default: 0.05
    pub pid_target_p95_latency_ms: f32, // Default: 200.0
    pub pid_k_min: usize,               // Default: 50 (wissenschaftliches Minimum)
    pub pid_k_max: usize,               // Default: 500
    
    // F-09: Resonanz-Fusion
    pub resonance_beta: f32,            // Default: 0.15
    
    // F-11: Lyapunov-Drift
    pub lyapunov_window_size: usize,    // Default: 20
    pub lyapunov_trigger_threshold: f32, // Default: 0.0 (positiver Exponent = Drift)
}

impl Default for PhysioConfig {
    fn default() -> Self {
        // Alle Defaults sind P1-sicher — kein Physio-Feature verändert Verhalten
        // wenn es deaktiviert ist (PhysioFeatureFlags::default() = alle false)
        Self {
            scheduler_tick_secs: 60,
            synaptic_flush_ticks: 10,
            percolation_check_ticks: 60,
            thermostat_kappa: 2.0,
            thermostat_w1: 0.6,
            thermostat_w2: 0.4,
            synaptic_eta: 0.01,
            synaptic_delta: 0.001,
            synaptic_rho: 0.05,
            synaptic_q: 1.0,
            synaptic_alpha: 0.5,
            synaptic_w_max_factor: 5.0,
            immune_theta_reject: 0.9,
            immune_theta_flag: 0.7,
            immune_beta: 0.1,
            immune_a_max: 1.0,
            immune_lambda_ratio: 0.1,
            percolation_theta_perc: 0.7,
            percolation_sampling_fraction: 0.1,
            replicator_eta: 0.05,
            replicator_w_min: 0.05,
            pid_kp: 0.5,
            pid_ki: 0.1,
            pid_kd: 0.05,
            pid_target_p95_latency_ms: 200.0,
            pid_k_min: 50,
            pid_k_max: 500,
            resonance_beta: 0.15,
            lyapunov_window_size: 20,
            lyapunov_trigger_threshold: 0.0,
        }
    }
}
```

---

## §11 Wettbewerbspositionierung

### §11.1 Vollständige Feature-Matrix (technische Tiefe)

| Dimension | MemFuse v4.0 | YantrikDB | MinnsDB | Graphiti | Mem0/Zep |
|---|---|---|---|---|---|
| **Storage** | LSM+WAL v3 (HMAC-Chain) | redb (B-Tree) | Hybrid (Temporal KG) | Neo4j/Postgres | Cloud-DB |
| **Crash-Sicherheit** | WAL+Chaos-Tests | redb-intern | nicht dokumentiert | Backend-abhängig | Cloud |
| **Vektor-Index** | HNSW (SIMD 8× Speedup) + DiskANN | HNSW+Decay | HNSW + Edge-HNSW | Backend-HNSW | Backend |
| **Retrieval-Signale** | 3-RRF + Kohärenz (F-09) + Synaptisch (F-03) + Reranker | 4+Decay+Contradiction | 7-Signal (Edge-Vektoren) | Backend-abhängig | Primär Vektor |
| **Temporal Decay** | F-01 (systemzustandsabhängig) | ✅ (statisch) | ✅ (Validity Windows) | ✅ (Episode-Pinning) | ❌ |
| **Konsolidierung** | NREM+REM (F-05, Segment-Level) | think() (Merge+Scan) | LLM-Cascade | Episode-Retention | ❌ |
| **Widerspruch** | F-04 (Immun, präventiv+Populationsdynamik) | Reaktiv (think()) | LLM-Cascade | Superseded_by | ❌ |
| **Graph-Gesundheit** | F-06 (Perkolations-Monitor) | ❌ | Ontologie-Cascade | ❌ | ❌ |
| **KV-Cache** | KV Packet (verschlüsselt, mandantenisoliert) | ❌ | ❌ | ❌ | ❌ |
| **Kalibrierung** | Unified Primitive + ConfigFingerprint + Lyapunov | ❌ | ❌ | ❌ | ❌ |
| **Lösch-Nachweis** | DeletionProof (kryptographisch) | ❌ | ❌ | ❌ | ❌ |
| **Sovereign Core** | Candle (Pure-Rust GGUF) | Kein eigenes LLM | Kein eigenes LLM | Cloud-LLM | Cloud-LLM |
| **Deutsche Morphologie** | ✅ (Kompositum-Dekomposition) | ❌ | ❌ | ❌ | ❌ |

---

## §12 Roadmap mit Sprint-Struktur & Abhängigkeitsgraph

### §12.1 Kritischer Pfad (vollständiger Abhängigkeitsgraph)

```
[P-SOFORT-1] P8/P9/P10/P11/P12 ADRs in CONSTITUTION.md
     ↓
[P-SOFORT-4] memfuse-calibration Grundgerüst (§3.3)
     ├──→ [P-SOFORT-2] Router-ConfigFingerprint (§6.2)
     │         ↓
     │    [F-11] Lyapunov-Drift-Wächter (§9.11, §6.2)
     │
     ├──→ ImportanceClassifier (§6.1)
     │         ↓
     │    Reranker-Kalibrierung schließt AGT-EMBED-62093e61
     │
     └──→ [P-SOFORT-3] Reranking-Fenster-Fix (§5.2)
               ↓
          [F-08] PID-Homöostat (§9.8)

[P-SOFORT-5] DiskANN persist_delta() (§4.3) — parallel zu obigem

TenantId-Typ (memfuse-core, Layer 0)
     ↓
TenantKeyCodec (memfuse-store) + DeletionProof (memfuse-crypto)
     ↓
KV-Bridge Tenant-Isolation ←── memfuse-candle ←── AFIT-Migration

CSR-Provenienz-Pflicht (§4.5)
     ↓
F-04 Immun-Gedächtnis (§9.4) ←── Supersedes-Tombstone-Trigger
     ↓
F-05 REM-Konsolidierung (§5.5)
     ↓
F-10 VETO — nicht implementierbar

F-03 Synaptische Kanten (§9.3) ←── WAL-Batching-Infrastruktur
F-06 Perkolationsmonitor (§9.6) ←── MVCC-Snapshot-Infrastruktur
F-07 Replikator-Gewichte (§9.7) ←── memfuse-calibration
F-09 Resonanz-Fusion (§9.9) — SOFORT parallelisierbar
F-01 Thermostat (§5.6) ←── ImportanceRecord PRIO 2
```

### §12.2 Horizont G0 — Sofortmaßnahmen (Woche 1–2)

| Initiative | Aufwand | Akzeptanzkriterium |
|---|---|---|
| ADR P8–P12 in CONSTITUTION.md | 0.5 Tage | ADR-Nummern vergeben, in DECISIONS.md |
| Router-ConfigFingerprint (§6.2) | 3–4 Tage | Test: Prompt-Änderung → calibrated: false; Quantisierungswechsel → neuer Fingerabdruck |
| Reranking-Fenster-Fix (§5.2) | 0.5–1 Tag | Benchmark: Recall@5 vor/nach Fix dokumentiert |
| memfuse-calibration Grundgerüst (§3.3) | 2–3 Tage | IsotonicCalibrator: None bei < warmup_required; invalidate_on_config_change() reset |
| Reranker-Kalibrierung (AGT-EMBED-62093e61) | 1–2 Tage | Sigmoid → Platt-kalibriert via memfuse-calibration |
| DiskANN persist_delta() (§4.3) | 3–4 Tage | Atomic Write + Merge + Test; kein Vollrebuild bei kleinen Deltas |
| F-09 Resonanz-Fusion (§9.9) | 1 Woche | Recall@5 ≥ +1pp vs. RRF-Baseline; INV-PROV-2 eingehalten |

### §12.3 Horizont 1 — Primärkanal-Fertigstellung (Woche 2–6)

| Initiative | Kernergebnis | Abhängigkeit |
|---|---|---|
| Session-DAG-UI vollständig | Konversationsverzweigung sichtbar in Desktop-UI | Backend bereits produktionsreif |
| ImportanceClassifier Variante B (k-NN) | LLM-Call im Hot-Path eliminiert, P50 < 100ms | memfuse-calibration |
| LongMemEval-S/LoCoMo Integration | Erste externe, reproduzierbare Ergebnisse | Keine |
| Installer/Auto-Update | Endnutzer ohne Cargo installierbar | Tauri-Release-Workflow |
| ADR-AFIT-Migration (Planung) | Strategie-ADR: trait_variant vs. BoxFuture | Vor SOLO-Lauf |
| F-01 Thermostat (nach PRIO 2) | Adaptive Half-Life unter Speicherdruck | ImportanceRecord PRIO 2 |

**Exit-Kriterium H1:** Nicht-technischer Testnutzer installiert MemFuse Brain, importiert Dokumente, verzweigt im Session-DAG, sieht LongMemEval-Ergebnis (gegen LycheeMemory V2 Referenz gemessen) im README.

### §12.4 Horizont 2 — Sovereign Core (Woche 3–16)

```
Woche 3–6:  AFIT-Migration (SOLO-Lauf, isoliert)
             → memfuse-core Traits zuerst (diskann.rs:988 betroffen)
             → Post-Merge: alle drei Feature-Checks (reranking, candle, physio)

Woche 5–8:  memfuse-candle Strategie A (parallel nach AFIT)
             → Blaupause: candelabra (GGUF-Loader) + candle-core (Attention)
             → Akzeptanz: cosine_similarity > 0.85 (Candle vs. Ollama, gleicher Prompt)

Woche 6–7:  ImportanceClassifier Variante A (ONNX, nach Variante B)
             → Trainings-Daten: Distillation aus bisherigen LLM-Scoring-Calls

Woche 8–14: KV-Cache-Bridge MIT Sicherheitsschicht
             → Voraussetzung: memfuse-candle fertig (RoPE-Shift-Zugriff)
             → Voraussetzung: TenantId-Typ in memfuse-core
             → KV Packet Adapter-Training (256–512 Samples, einmalig offline)
             → Akzeptanz: ≥40% TTFT-Reduktion + Zeroize-Test + Tenant-Isolation-Test
```

### §12.5 Horizont 3 — Gedächtnis-Intelligenz (Woche 6–14, teilweise parallel)

| Initiative | Wissenschaftliche Basis | Aufwand (Rust-realistisch) |
|---|---|---|
| SleepCycle NREM+REM (F-05) | SleepGate, LycheeMemory V2 | 4 Wochen |
| PathRAG + Query-Klassifikator + Sufficiency-Gate (§5.4) | PathRAG AAAI, ICLR Graph-Survey | 2–3 Wochen |
| CSR-Graph-Provenienz-Pflicht + Supersedes-Trigger (§4.5) | arXiv:2603.14828 | 1 Woche |
| F-04 Immun-Gedächtnis (§9.4) | Klonale Selektion | 3 Wochen |
| F-03 Synaptische Kanten (§9.3) | Hebbian + Stigmergie | 3 Wochen |
| F-06 Perkolationsmonitor (§9.6) | Perkolationstheorie | 2 Wochen |
| F-07 Replikator-Gewichte (§9.7) | Multiplicative Weights | 1.5 Wochen |

**Exit-Kriterium H3:** LoCoMo > 80%, LongMemEval-S > 85%. SleepCycle 24h ohne Absturz auf realem Datensatz. PathRAG aktiviert sich nachweislich nur bei Multi-Hop-Queries.

### §12.6 Horizont 4 — Enterprise (Woche 14–24)

```
TenantId (memfuse-core)         → 1–2 Tage [VORGEZOGEN, da KV-Bridge-Abhängigkeit]
     ↓
DeletionProof + DeletionLayer   → 4–5 Tage (§3.5)
     ↓
TenantKeyCodec in LSM           → 4–5 Tage (§3.4)
     ↓
KV-Cache Tenant-Isolation       → 2–3 Tage (§7.1, Abgleich mit §3.4)
     ↓
RBAC + Audit-Trail              → 3–4 Wochen
     ↓
OAuth/SSO                       → 2–3 Wochen
```

### §12.7 Horizont 5 — Ökosystem-Reife (parallel zu H3/H4)

| Initiative | Kernergebnis |
|---|---|
| PyPI Publish-Pipeline | Automatisierte Veröffentlichung |
| MCP Onboarding-Dokumentation | Claude Desktop / Cursor vollständiger Onboarding-Pfad |
| AQR-HNSW Quantisierung (arXiv:2602.21600) | 4× Kompression, 2.5–3.3× Throughput |
| LongMemEval-M + BEAM-1M Integration | Enterprise-Skalierungs-Nachweis |
| BrainPalace-inspiriertes LSP-aware Chunking | AST-Grenzen für Code-Repositories |

---

## §13 Definition of Done

Ein Task ist abgeschlossen wenn **alle** Bedingungen erfüllt sind:

### Code-Dimension (CI-erzwungen)
- `cargo test --workspace` grün
- `just dag-check` grün (keine Layer-Violations)
- `cargo check --features reranking` grün
- `cargo check --features candle` grün (ab H2)
- `cargo check --features physio-thermostat` grün (jedes physio-Feature einzeln)
- Keine neuen `let _ =` auf I/O-Operationen (P3-Schutz)
- Keine neuen `#[allow(deprecated)]` ohne explizite Begründung und Fälligkeit
- `cargo clippy --workspace -- -D warnings` grün

### Kalibrierungs-Dimension (NEU v3.0, P8)
- Jede Änderung an SlmProfile, Prompt-Templates oder Modell-Konfiguration löst nachweislich einen Kalibrierungs-Reset aus (automatischer Test)
- Jede neue Konfidenz-/Wahrscheinlichkeitsaussage nutzt `memfuse-calibration`, keine Ad-hoc-Sigmoid-Logik

### Provenienz-Dimension (NEU v3.0)
- Jede neue Graph-Kante hat einen EdgeProvenance-Eintrag (INV-GRAPH-PROV-1)
- Jede neue RRF-Fusion mit Kohärenz-Bonus: INV-PROV-2 gewährleistet (coherence_bonus als separates Feld)
- `sum(contributions) ≈ rrf_score` (|Δ| < 1e-6) — INV-PROV-1 weiterhin gültig

### Produkt-Dimension (manuell)
- Kann ein Mitglied der Zielgruppe die Änderung nutzen?
- Gibt es einen End-to-End-Test der den Nutzer-Flow abbildet?

### Prozess-Dimension
- Vor jedem neuen Agent-Task: `git branch -r | grep -i "<THEMA>"` — kein redundanter Branch
- ADR in `DECISIONS.md` eingetragen wenn architektonisch
- `WORKING_STATE.md` via `cargo xtask sync-docs` aktualisiert

### Chaos-Dimension (Storage-Änderungen)
- Power-Cut-Simulation für WAL/Compaction/Recovery
- SSTable-Bit-Flip-Fuzzing für Index-Änderungen
- **[NEU]** Zeroize-Nachweis nach KV-Cache-Eviction (Speicher-Scan-Test)

### Physik-Dimension (Physio-Features)
- Jedes gemergete physio-Feature dokumentiert im PR: (a) mathematische Formel erfüllt, (b) bestehende Datenstruktur genutzt, (c) analogie-unabhängiges Akzeptanzkriterium grün
- Feature ist im Zero-IT-Setup-Default NICHT sichtbar (Feature-Flag-Test)

---

## §14 Vollständiger ADR-Backlog

| ADR | Titel | Priorität | Status |
|---|---|---|---|
| **Sofort** | | | |
| ADR-0xx | P8/P9/P10/P11/P12 Kodifizierung in CONSTITUTION.md | SOFORT | Offen |
| ADR-0xx | Router-ConfigFingerprint: Zwingende Re-Kalibrierung bei Konfigurationsänderung | SOFORT | Offen |
| ADR-0xx | memfuse-calibration: Unified Calibration Primitive vor ImportanceClassifier | SOFORT | Offen |
| ADR-0xx | ImportanceClassifier: Variante A (ONNX) vs. B (k-NN), Phasenplan | Hoch | Offen |
| ADR-0xx | TenantId: Typ-Definition in memfuse-core Layer 0 | Hoch | Offen |
| ADR-0xx | KV-Segment-Verschlüsselung + Zeroize-on-Evict Architektur | Hoch | Offen |
| ADR-0xx | PathRAG: Query-Klassifikator + Sufficiency-Gate + MultiHop-only-Aktivierung | Mittel | Offen |
| ADR-0xx | CSR-Graph-Provenienz-Pflicht + Supersedes→Tombstone-Trigger | Mittel | Offen |
| ADR-0xx | Chunk-Injektionsreihenfolge-Policy (Reranker-Score-absteigend + Lost-in-Middle) | Mittel | Offen |
| ADR-0xx | AFIT-Migration: dyn-Kompatibilitätsstrategie (trait_variant vs. BoxFuture) | Mittel | Offen |
| ADR-0xx | memfuse-candle: Native GGUF, Strategie A (candle-core) vs. B (mistral.rs) | Mittel | Offen |
| ADR-0xx | DeletionProof: Scope-Dokumentationspflicht + ExcludedScope-Enum | Mittel | Offen |
| ADR-0xx | DiskANN: persist_delta() Lifecycle + Delta-Merge-Algorithmus | Mittel | Offen |
| **Physio-Features** | | | |
| ADR-070 | F-01: Systemtemperatur-abgeleitete Half-Life für Importance-Decay | Mittel | Offen |
| ADR-071 | F-02: PERMANENT REJECTED — Partieller HNSW-Rebuild verletzt globale Integrität | Abgeschlossen | Veto |
| ADR-072 | F-03: Hebbian/Stigmergisches Kantengewicht als 5. RRF-Signal + WAL-Batching | Mittel | Offen |
| ADR-073 | F-04: Immunologisches Antikörper-Register + Trigger-Pfad Supersedes→CSR-Tombstone | Mittel | Offen |
| ADR-074 | F-05: REM-Synthese-Phase im Sleep-Cycle + abstracts_from-Kantentyp | Mittel | Offen |
| ADR-075 | F-06: Perkolations-Gesundheitsmetrik φ(t) + Re-Bonding-Pass | Mittel | Offen |
| ADR-076 | F-07: Multiplicative-Weights-RRF + ProvenanceRecord-Feedback-Loop | Mittel | Offen |
| ADR-077 | F-08: PID-Regler für Kandidatenpoolgröße (NACH Deadline-Fix) | Niedrig | Offen |
| ADR-078 | F-09: Kohärenz-Bonus-Term in RRF + INV-PROV-2 | SOFORT | Offen |
| ADR-079 | F-10: PERMANENT REJECTED — Bricht TenantId-Isolation + DeletionProof | Abgeschlossen | Veto |
| ADR-080 | F-11: Lyapunov-Drift-Erkennung als proaktive Ergänzung zu ConfigFingerprint | Mittel | Offen |
| ADR-081 | PhysioScheduler: Unified Background-Worker statt N unabhängiger Tasks | Mittel | Offen |
| ADR-082 | PhysioConfig: Zentrales Parameter-Struct + P12-Default-Garantien | Mittel | Offen |

---

## §15 Governance & Prozessmodell

### §15.1 Branch-Hygiene (nicht verhandelbar)

```bash
# Pflicht VOR jedem neuen Agent-Task:
git branch -r | grep -i "<THEMA-STICHWORT>" | head -5
# Treffer → bestehenden Branch reviewen statt neu anlegen
```

### §15.2 Große Refaktorierungen als SOLO-Lauf

Router-ConfigFingerprint-Migration, AFIT-Migration, Checkpoint-Konsolidierung, Tenant-Isolation-Integration: Alle als isolierter SOLO-Lauf ohne parallele Branches.

Post-Merge-Pflicht-Checks:
```bash
cargo test --workspace 2>&1 | grep -E "FAILED|error"
cargo check --features reranking
cargo check --features candle      # ab H2
cargo check --features physio-thermostat  # jedes aktive physio-Feature
grep -rn "#[allow(deprecated)]" crates/ --include="*.rs" | grep -v "test"
```

**Lernpunkt Commit 46a20b22:** Eine Refaktorierung setzte bereits behobene `#[allow(deprecated)]`-Fixes zurück. Gegenmittel: Post-Merge-Diff-Review gegen alle zuvor behobenen Punkte. Für `memfuse-calibration` (§3.3): Änderungen müssen gegen ALLE drei Verwender (Router, Reranker, ImportanceClassifier) re-getestet werden.

### §15.3 Marketing-Aussagen sind an Code-Nachweise gebunden (P7)

Gilt explizit auch für interne Architekturdokumente. Jede quantitative Aussage benötigt:
- Eigene reproduzierbare Messung in `memfuse-bench`, ODER
- Explizite "fremdreferenziert, an MemFuse nicht validiert"-Kennzeichnung mit ArXiv-ID

### §15.4 Chronologie-Vorrangregel

Bei widersprüchlichen Quellen: Jüngere Quelle hat Vorrang (Konferenz-akzeptiert > gleichdatiertes Preprint). Widerlegung wird explizit vermerkt. Beispiel: LycheeMemory V2 (Aug 2026) setzt frühere SOTA-Zahlen (30–70%) außer Kraft → Zielwerte in §8.1 entsprechend aktualisiert.

---

## Anhang A: Technische Schulden (priorisiert nach Impact)

| Schuld | Impact | Horizont |
|---|---|---|
| 125 `#[async_trait]`-Annotationen (betrifft Kernpfade, diskann.rs:988) | Box-Allokation pro async-Aufruf — Sub-ms-Latenz-Versprechen inkonsistent | H2 (SOLO) |
| Reranking-Pool k*3=30 Kandidaten | Recall@5 ≈ 0.458 (fast wertlos) | G0 (SOFORT) |
| SlmProfile ohne ConfigFingerprint | Coverage-Kollaps-Risiko | G0 (SOFORT) |
| ImportanceScore als LLM-Call im Hot-Path | 970ms P50 statt erreichbarer 58ms | H1 |
| 3 parallele Kalibrierungspfade ohne gemeinsame Primitive | Kollisionsrisiko, Inkonsistenz | G0 |
| DiskANN: kein inkrementeller Persist-Pfad | Nur Vollrebuild möglich | G0 |
| Kein TenantId-Typ in memfuse-core | Blockiert KV-Bridge und LSM-Isolation | H2/H4 |
| Checkpoint-Konsolidierung: 3 Abstraktionen → 1 Fassade | Kognitive Redundanz, Sync-Risiko | H3 |
| Benchmark: 8-Dokument-Synthetik-Korpus | Statistisch bedeutungslos, Recall@1=100% wertlos | H1 |
| `unsafe` in Tests ohne SAFETY-Kommentare | P2-Verletzung | H3 |
| Keine Cascade-Invalidation Supersedes→Graph-Kanten | Veraltete Kanten bleiben aktiv | H3 (F-04 liefert Trigger) |

---

## Anhang B: Verworfene Features (mit Begründung)

| Feature | Grund für Verwerfung |
|---|---|
| F-02 Nukleations-Trigger (partieller HNSW-Rebuild) | HNSW global verschränkt — partielle Rebuilds zerstören Nachbarschaftsinvarianten. Lock-Contention unter RwLock in Rust unlösbar. |
| F-10 Osmotischer Cross-Tenant-Austausch | Bricht TenantId-Isolationsgarantie + DeletionProof-Beweisbarkeit + KV-Cache-Sicherheitsmodell. DSGVO-Compliance-Risiko. |
| Quanten-Superpositions-Bewertung (PRD-Entwurf) | Kein analogie-unabhängiges Akzeptanzkriterium erfüllbar — nur verzögerte Score-Aggregation reimplementiert. |
| Lotka-Volterra wörtlich statt Replikatordynamik | Replikatordynamik (F-07) liefert identische Homöostase-Eigenschaften mit bekannten, bewiesenen Konvergenzgarantien. |
| Genetische Algorithmen für Meta-Hyperparameter | F-07+F-08 lösen dasselbe Problem mit stärkeren theoretischen Garantien. Redundante Komplexität. |

---

## Anhang C: ArXiv-Paper-Verzeichnis (Tier 1–3)

### Tier 1 — Unmittelbar architektur-relevant (ADR erforderlich)

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| 2608.01460 | Conformalized LLMs under Configuration Shift | Router ConfigFingerprint (§6.2) — KRITISCHSTE QUELLE |
| 2604.13226 | KV Packet: Recomputation-Free Context-Independent KV Caching | KV-Cache-Bridge (§7.1) |
| 2510.17098 | Can Transformer Memory Be Corrupted? | KV-Bridge-Sicherheit (§7.1) |
| 2508.09442 | Privacy Risks of KV-cache in LLM Inference | KV-Bridge-Sicherheit (§7.1) |
| 2605.00356 | MemRouter: Memory-as-Embedding Routing | ImportanceClassifier (§6.1) |
| 2608.12990 | LycheeMemory V2 | SleepCycle-Zielwerte (§5.5, §8.1) — AKTUELLSTER SOTA |
| 2506.00610 | MemGraphRAG (Recall vs. Precision) | PathRAG Sufficiency-Gate (§5.4) |
| 2506.05690 | When to use Graphs in RAG (ICLR 2026) | PathRAG-Trigger-Logik (§5.4) |
| 2505.16831 | Unlearning Isn't Deletion | DeletionProof-Grenze (§3.5) |
| 2502.14902 | PathRAG (AAAI 2026) | PathRAG-Basisreferenz (§5.4) |

### Tier 2 — Wissenschaftliche Validierung (stützend)

| ArXiv-ID | Titel | MemFuse-Komponente |
|---|---|---|
| 2603.14517 | SleepGate | SleepCycle-Interferenzhorizont (§5.5) |
| 2605.17625 | Episodic-Semantic Memory Architecture | SleepCycle Dual-Process (§5.5) |
| 2604.01733 | From BM25 to Corrective RAG (T2-RAGBench) | RRF-Konfiguration, Rerank-Fenster (§5.2) |
| 2605.18796 | UCCI | Conformal Router (§6.2, §3.3) |
| 2603.06616 | RACER | Abstention-Pfad (§6.2) |
| 2607.04223 | GASP | Post-Hoc-Validator (§7.2) |
| 2601.02993 | Stable-RAG | Chunk-Injektionsreihenfolge (§5.2) |
| 2603.14828 | Robust Multi-Hop GraphRAG | Graph-Provenienz-Pflicht (§4.5) |
| 2603.15033 | MUNKEY | DeletionProof-Stützung (§3.5) |
| 2602.21600 | AQR-HNSW | HNSW-Quantisierungs-Roadmap (H5) |
| 2602.21514 | I/O Optimizations for Graph-Based ANN | DiskANN-Lifecycle (§4.3) |
| 2410.10813 | LongMemEval (ICLR 2025) | Benchmark-Integration (§8.1) |
| 2604.19771 | Cognis | Deutsche Morphologie (§4.4) |
| 2512.07515 | TPA: Token Probability Attribution | Halluzinations-Mechanismus (§7.2) |
| 2503.19878 | CausalRAG | PathRAG-Kausal-Validierung (§5.4) |
| 2604.23577 | RouteNLP | Router-SOTA-Vergleich (§6.2) |

### Tier 3 — Kritische Gegenposition (Risikomanagement)

| ArXiv-ID | Titel | MemFuse-Risiko |
|---|---|---|
| 2604.09666 | Do We Still Need GraphRAG? | PathRAG-Scope-Beschränkung (§5.4) |
| 2603.19664 | The Residual Stream Is All You Need? | KV-Cache-Fallback-Design (§7.1) |

---

## Anhang D: PhysioConfig-Referenz

→ Vollständig in §10.2. Alle Parameter mit Defaults. Änderung erfordert Re-Validierung via `memfuse-bench`. Versionierung analog zu Prompt-Templates (P8-analoge Regel).

---

## Anhang E: Wettbewerber-Featureδ-Analyse

### Was Wettbewerber haben, MemFuse nicht (und warum)

| Wettbewerber-Feature | Wettbewerber | MemFuse-Status | MemFuse-Antwort |
|---|---|---|---|
| Edge-Vektoren (Separate HNSW für Kanten) | MinnsDB | ❌ | F-03 löst Kantenrelevanz über Nutzungsdynamik — kein zweiter Index nötig |
| Statischer Temporal Decay | YantrikDB | ❌ | F-01 löst es systemzustandsabhängig — dynamischer als YantrikDB |
| Validity Windows auf Kanten | Graphiti | ✅ (ADR-033) | Bereits implementiert, nur Trigger zu CSR-Tombstone fehlte (§4.5) |
| Autonomer `think()`-Consolidation-Pass | YantrikDB | ⏳ | SleepCycle (§5.5) + F-05 REM übertrifft think() durch generative Synthese |
| Ontologie-getriebene Cascade-Invalidation | MinnsDB | ❌ | §6.28 (Cascade-Invalidation) schließt diese Lücke |
| 7-Signal-Fusion | MinnsDB | Teils | 3-RRF + F-09 + F-03 + Reranker = effektiv 5+ Signale ohne MinnsDB's Overhead |
| AST-aware Code-Chunking | BrainPalace | ❌ | H5 Roadmap (niedrige Priorität) |

### Was MemFuse hat, kein Wettbewerber hat

1. WAL v3 mit HMAC-Chain (kryptographische WAL-Integrität)
2. KV-Cache-Bridge mit Positions-Unabhängigkeit (KV Packet), verschlüsselt + mandantenisoliert
3. Conformal Router mit ConfigFingerprint-Zwang + Lyapunov-Drift (F-11)
4. Kryptographischer DeletionProof mit layer-expliziter Deckungsgrenze
5. Physikalisch-biologisches Selbstmanagement als kohärentes System (F-01, F-03, F-04, F-06, F-07, F-09, F-11)
6. Deutsche Morphologie (Kompositum-Dekomposition) in BM25
7. Pure-Rust-Embedding + ONNX-Reranker ohne externe Abhängigkeit
8. Session-DAG mit Typ-erzwungener Lock-Reihenfolge (NodesGuard — keine Deadlocks)

---

*Dieses Dokument ist die einzige normative Wahrheitsquelle für MemFuse-Architektur, -Features und -Roadmap (Version 4.0). Es ersetzt vollständig alle Vorgängerdokumente. Nächste Überarbeitung: Nach Abschluss Horizont G0 (alle P-SOFORT-Maßnahmen), wenn empirische Validierung der Sofortfixes vorliegt.*
