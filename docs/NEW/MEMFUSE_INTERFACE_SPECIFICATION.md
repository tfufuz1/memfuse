# MemFuse — Mikrofeingranulare Schnittstellenspezifikation
## Konkrete Trait-, Typ- und Funktionssignaturen für die nächste Ausbaustufe, verifiziert gegen Repository-Code und ArXiv-Primärquellen (Stand August 2026)

> **Verhältnis zu anderen Dokumenten**: Dieses Dokument ergänzt `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md` (strategische Ebene: *warum*) um die Implementierungsebene (*wie genau*, in Rust-Signaturen). Es ersetzt keines der beiden Dokumente, sondern übersetzt die dort priorisierte Roadmap in konkrete `pub trait`/`pub struct`/`pub fn`-Vorschläge, jeweils direkt neben dem tatsächlich vorgefundenen Code zitiert.
> **Methodik**: Für jede Spezifikation wurde (1) der reale Code im geklonten Repository `github.com/tfufuz1/memfuse` (HEAD zum Zeitpunkt der Analyse, 50 Commits im flachen Klon, 58.853 Rust-Zeilen, 39 abgeschlossene ADRs) gelesen, (2) die zugrunde liegende ArXiv-Quelle verifiziert (nicht aus Trainingswissen übernommen, sondern per Websuche am 29.08.2026 bestätigt), und (3) eine Schnittstelle entworfen, die sich additiv in das bestehende Trait-System einfügt, ohne ADR-035 (Governance gegen Trait-Default- und Typ-Dopplungsfehler) zu verletzen.
> **Ehrlichkeitsprinzip**: Wo der reale Code von der bisherigen Strategiedarstellung abweicht (z. B. fehlendes `ProvenanceRecord`-Objekt), wird das hier explizit korrigiert, nicht stillschweigend übernommen.

---

## Inhaltsverzeichnis

0. Korrekturen gegenüber dem bisherigen Strategiedokument (Code-Realitätsabgleich)
1. `memfuse-core`: `ProvenanceRecord`, `CausalEdge`, erweiterte `GraphIndex`-Methoden
2. `memfuse-graph`: Multi-Graph-Namespace-Erweiterung (MAGMA-Muster)
3. `memfuse-db`: `ContextCompactor`-Erweiterung, `ProvenanceRecord`-Integration, Cache-bewusste Segmentierung
4. `memfuse-router`: Kalibriertes Kaskaden-Routing (UCCI-Muster)
5. `memfuse-agent`: Sleep-Cycle-Konsolidierungs-Tool (Auto-Dreamer-Muster) & proaktive Foresight-Events (CogniFold/EverMemOS-Muster)
6. `memfuse-crypto`: Kryptographischer Löschbeweis (Verified Forgetting)
7. `memfuse-mcp`: Schreibautorisierungs-Gate (VMG Write Authorization)
8. Neue Retrieval-Strategie: Pfad-Extraktion (PathRAG-Stil) in `GraphTraversalStrategy`
9. Quellenregister mit Verifikationsstatus
10. Was bewusst **nicht** spezifiziert wird und warum

---

## 0. Korrekturen gegenüber dem bisherigen Strategiedokument

Die Code-Analyse deckt drei Punkte auf, die in `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md` optimistischer dargestellt waren, als der Code es hergibt. Das wird hier zuerst korrigiert, weil die Feinspezifikation sonst auf einer falschen Ausgangslage aufbaut:

| Behauptung im Strategiedokument | Code-Realität | Konsequenz für diese Spezifikation |
|---|---|---|
| "Provenienz-Feld" bei `consolidate_via_llm` (ADR-032) bereits vorhanden | `consolidate_via_llm()` in `memfuse-db/src/context_compaction.rs` trägt Herkunft nur als `source_doc_ids: Vec<DocId>` und ein Metadata-Flag `"llm_summarized": true` — es gibt **kein** dediziertes, abfragbares `ProvenanceRecord`-Objekt, keine Kette (wer hat wann konsolidiert, aus welchem WAL-Segment, mit welchem Prompt-Hash) | Abschnitt 1 und 3 spezifizieren `ProvenanceRecord` **als neuen Typ von Grund auf**, nicht als Erweiterung eines bestehenden Feldes |
| `memfuse-router` "basiert auf einfacher Score-Aggregation ohne Kalibrierung" | Bestätigt exakt: `RouterEngine::route()` in `router.rs` aggregiert `chunk.relevance` roh, mit einem statischen `1.2×`-Community-Boost, kein Kalibrierungsschritt, kein Konfidenzintervall | Abschnitt 4 kann direkt an der bestehenden Funktion ansetzen — Diagnose war korrekt |
| ADR-Anzahl "40 dokumentierte ADRs" | `DECISIONS.md` enthält 39 tatsächlich ausgefüllte ADR-Einträge plus 1 leeres `## ADR-NNN: <Titel>`-Template | Kosmetisch, aber für Präzision hier korrigiert: **39 abgeschlossene ADRs** |
| `GraphIndex`-Trait "existiert bereits für additive Erweiterung" | Bestätigt: `memfuse-core/src/traits.rs` definiert `GraphIndex` mit Default-Methoden, die bei fehlender Implementierung `MemFuseError::CapabilityUnsupported` zurückgeben (z. B. `traverse_at`, `personalized_page_rank`) — ein sauberes, bereits etabliertes Erweiterungsmuster | Alle neuen Trait-Methoden in dieser Spezifikation folgen exakt diesem Muster |

---

## 1. `memfuse-core`: Neue Typen und Trait-Erweiterungen

### 1.1 `ProvenanceRecord` — abfragbares Herkunfts-Objekt

**Forschungsbezug**: VMG-Primitiv "Provenance Visibility" (arXiv:2604.16548, verifiziert — *A Survey on Long-Term Memory Security in LLM Agents*, Lin/Li/Chen, MemTensor Shanghai, cs.CR, 17. April 2026). Ergänzend: MemLineage (arXiv:2605.14421, "lineage-guided enforcement for LLM agent memory") und Auto-Dreamer (arXiv:2605.20616), das explizit **"provenance-linked source trajectories"** als Voraussetzung für sicheres Konsolidieren nennt — ein Konsolidator darf nur auf Einträge zugreifen, die ihre Quelle nachweisen können.

**Ort**: neue Datei `memfuse-core/src/types/provenance.rs`, re-exportiert über `lib.rs`.

```rust
/// Herkunfts-Nachweis für einen einzelnen Memory-Eintrag (Chunk, Entity, Edge).
///
/// Wird bei jeder schreibenden Operation (Ingestion, LLM-Konsolidierung,
/// Agent-Tool-Ausgabe) miterzeugt und unveränderlich (append-only) im
/// LSM-Store unter dem Präfix `__provenance:` persistiert — analog zum
/// bestehenden `__graph:entity:`/`__graph:edge:`-Muster (ADR-033).
///
/// Referenz: VMG-Primitiv "Provenance Visibility" (arXiv:2604.16548, Table 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// Eindeutige ID dieses Provenance-Eintrags (monoton, analog zu TxId).
    pub provenance_id: u64,
    /// Ziel-Objekt, auf das sich dieser Nachweis bezieht.
    pub target: ProvenanceTarget,
    /// Herkunftsart: woher stammt der Inhalt ursächlich.
    pub origin: ProvenanceOrigin,
    /// TxId, unter der dieser Eintrag geschrieben wurde (Kausalordnung, AGT-GRAPH-001-kompatibel).
    pub written_at_tx: TxId,
    /// Bei LLM-generierten Einträgen: SHA-256-Hash des tatsächlich verwendeten Prompts.
    /// None bei direkter Nutzereingabe / Ingestion ohne LLM-Schritt.
    pub prompt_hash: Option<[u8; 32]>,
    /// Kette der Quell-Einträge, aus denen dieser Eintrag hervorgegangen ist
    /// (z. B. bei consolidate_via_llm: alle ursprünglichen DocIds).
    pub derived_from: Vec<ProvenanceTarget>,
    /// HMAC-SHA256 über (target, origin, written_at_tx, prompt_hash, derived_from),
    /// verkettet mit dem WAL-HMAC des Vorgänger-Eintrags (WalHmac-Wiederverwendung, ADR-029).
    pub integrity_hmac: [u8; 32],
}

/// Adressierbares Ziel eines Provenance-Nachweises.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProvenanceTarget {
    Chunk(DocId),
    Entity(EntityId),
    Edge { from: EntityId, to: EntityId },
}

/// Herkunftsart eines Eintrags.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProvenanceOrigin {
    /// Direkte Nutzer-/API-Ingestion ohne LLM-Zwischenschritt.
    DirectIngestion,
    /// Erzeugt durch ContextCompactor::consolidate_via_llm().
    LlmConsolidation { model: String },
    /// Erzeugt durch einen Sleep-Cycle-Konsolidierungs-Workflow (siehe Abschnitt 5.1).
    SleepCycleConsolidation { workflow_run_id: u64 },
    /// Geschrieben durch ein AgentTool innerhalb eines memfuse-agent-Workflows.
    AgentTool { tool_name: String, node_id: String },
    /// Über memfuse-mcp durch einen externen MCP-Client geschrieben (Sandbox-Kontext).
    McpWrite { client_id: String },
}
```

**Neue Trait-Erweiterung in `memfuse-core/src/traits.rs`** (additiv, folgt dem `CapabilityUnsupported`-Default-Muster, das `GraphIndex` bereits etabliert):

```rust
#[async_trait]
pub trait ProvenanceStore: Send + Sync + 'static {
    /// Schreibt einen neuen, unveränderlichen Provenance-Eintrag.
    async fn record_provenance(&self, record: ProvenanceRecord) -> Result<()>;

    /// Liest die vollständige Herkunftskette für ein Ziel-Objekt zurück
    /// (rekursiv über `derived_from`, mit Zyklenerkennung).
    ///
    /// # Errors
    /// `MemFuseError::CapabilityUnsupported("provenance_chain")` falls
    /// nicht implementiert.
    async fn provenance_chain(
        &self,
        target: &ProvenanceTarget,
    ) -> Result<Vec<ProvenanceRecord>> {
        let _ = target;
        Err(MemFuseError::capability_unsupported(
            "provenance_chain",
            "Provenance-chain lookup is not supported by default — tracked as VMG primitive 'Provenance Visibility'",
        ))
    }

    /// Verifiziert die HMAC-Integritätskette eines Provenance-Eintrags
    /// gegen den WAL-Hash-Chain-Zustand (WalHmac-Wiederverwendung).
    async fn verify_provenance_integrity(
        &self,
        provenance_id: u64,
    ) -> Result<bool> {
        let _ = provenance_id;
        Err(MemFuseError::capability_unsupported(
            "verify_provenance_integrity",
            "Cryptographic provenance verification is not supported by default",
        ))
    }
}
```

### 1.2 `CausalEdge` — vierte Graph-Dimension (MAGMA-Vorstufe)

**Forschungsbezug**: MAGMA (arXiv:2601.03236, verifiziert — Jiang/Li/Li/Li, ACL 2026 Main Conference, `aclanthology.org/2026.acl-long.1709`, Seiten 36848–36865). Zentrale Aussage aus dem verifizierten Abstract: *"we propose MAGMA, a multi-graph agentic memory architecture that represents each memory item across orthogonal semantic, temporal, causal, and entity graphs"* — vier **orthogonale** Graphen, nicht vier Kantentypen in einem Graphen. MemFuse hat mit `Edge.valid_from`/`valid_to` (ADR-033) bereits die temporale Dimension; `CausalEdge` ist der erste Schritt zur kausalen Dimension, wie in der Roadmap (Strategiedokument Abschnitt 6, Punkt 5) vorgesehen.

Ergänzend gestützt durch CausalRAG2 (arXiv:2602.05143, verifiziert — Wang et al., v1 als "HugRAG" im Feb. 2026 eingereicht, in v2 vom 24. Juni 2026 zu "CausalRAG2" umbenannt, ICML 2026 angenommen), das explizit vor dem Fehler warnt, den reine PPR/BFS-Traversierung macht: Rückgabe von *"topically similar yet causally irrelevant evidence"* statt kausal relevanter Belege.

```rust
/// Gerichtete kausale Kante — orthogonal zur bestehenden semantischen/
/// entitätsbasierten Edge, NICHT deren Ersatz.
///
/// Persistiert unter neuem LSM-Präfix `__graph:causal:` (analog zu
/// `__graph:entity:`/`__graph:edge:`), damit bestehende CSR-Kompaktierung
/// (`CsrGraph::compact()`) unverändert bleibt und Traversierungen explizit
/// zwischen semantischem und kausalem Graphen wählen müssen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub cause: EntityId,
    pub effect: EntityId,
    /// Kausale Stärke/Konfidenz in [0.0, 1.0], nicht zu verwechseln mit
    /// der semantischen `weight` in `Edge` — unterschiedliche Skalen,
    /// unterschiedliche Bedeutung, daher bewusst kein gemeinsamer Typ.
    pub causal_strength: f32,
    /// Herkunft der Kausal-Annotation: LLM-inferiert oder aus explizitem
    /// Nutzer-/Dokumenten-Signal (z. B. "weil", "dadurch", "als Folge").
    pub inference_method: CausalInferenceMethod,
    pub valid_from: Option<TxId>,
    pub valid_to: Option<TxId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CausalInferenceMethod {
    LlmInferred { model: String, confidence: OrderedFloatBits },
    ExplicitMarker { marker_text: String },
}
```

### 1.3 Erweiterung `GraphIndex`-Trait um kausale Traversierung

```rust
pub trait GraphIndex: Send + Sync + 'static {
    // ... bestehende Methoden unverändert ...

    /// Traversiert ausschließlich den kausalen Graphen (nicht den semantischen/
    /// Entity-Graphen) beginnend bei `start_node`, gewichtet nach `causal_strength`
    /// statt nach `SCORE_DECAY * weight` wie in der bestehenden BFS-Traversierung.
    ///
    /// # Errors
    /// `MemFuseError::CapabilityUnsupported("graph_causal_traverse")` falls
    /// keine kausale Graph-Dimension implementiert ist.
    async fn causal_traverse(
        &self,
        _start_node: EntityId,
        _max_hops: usize,
        _direction: CausalDirection,
    ) -> Result<Vec<(EntityId, f32)>> {
        Err(MemFuseError::capability_unsupported(
            "graph_causal_traverse",
            "Causal-dimension traversal is not supported by default — tracked as MAGMA-inspired extension (arXiv:2601.03236)",
        ))
    }
}

/// Traversierungsrichtung im kausalen Graphen — Vorwärts (Ursache→Wirkung)
/// oder Rückwärts (Wirkung→Ursache, für "Warum ist X passiert?"-Queries,
/// direkt inspiriert vom CausalRAG2-Anwendungsbeispiel "Warum kam es zum
/// citywide gridlock nach dem Stromausfall?", arXiv:2602.05143).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalDirection { Forward, Backward }
```

---

## 2. `memfuse-graph`: Multi-Graph-Namespace-Erweiterung

**Ort**: neue Datei `memfuse-graph/src/causal.rs`, strukturell parallel zu `csr.rs`, aber als eigenständige `CsrGraph`-Instanz mit eigenem `GraphInner`, um Kompaktierung, Kanten-Iteration und Snapshot-Isolation (ADR-024) nicht mit dem semantischen Graphen zu vermischen.

```rust
/// Zweite CSR-Graph-Instanz für die kausale Dimension.
///
/// Bewusst KEINE Erweiterung von `CsrGraph::insert_edge_direct`, weil
/// `CausalEdge.causal_strength` semantisch nicht mit `Edge.weight`
/// austauschbar ist (MAGMA-Prinzip der Orthogonalität — dieselbe Information
/// darf nicht in zwei Bedeutungen in derselben Datenstruktur landen).
pub struct CausalCsrGraph {
    inner: parking_lot::RwLock<GraphInner>, // Wiederverwendung des bestehenden GraphInner-Typs
    storage: Option<Arc<dyn StorageEngine>>,
}

impl CausalCsrGraph {
    pub fn new() -> Self { /* ... */ }

    /// Persistiert unter `__graph:causal:` statt `__graph:edge:`.
    pub fn insert_causal_edge_direct(&self, edge: CausalEdge) -> Result<()> { /* ... */ }

    /// Führt eine gerichtete kausale BFS aus, analog zu
    /// `CsrGraph::traverse_at_time`, aber mit `causal_strength` statt
    /// `SCORE_DECAY * weight` als Zerfallsfunktion.
    pub fn causal_traverse(
        &self,
        start: EntityId,
        max_hops: usize,
        direction: CausalDirection,
    ) -> Vec<(EntityId, f32)> { /* ... */ }
}
```

**`CsrGraph::pagerank()`-Wiederverwendung**: Die bestehende `pagerank()`-Methode (Zeile 632 in `csr.rs`) und `ppr::compute_ppr()` sind graphstrukturagnostisch (sie operieren auf `offsets`/`targets`/`weights`-Arrays in `GraphInner`) — `CausalCsrGraph` kann dieselbe PPR-Implementierung direkt wiederverwenden, ohne Code-Duplikation. Das ist der konkrete technische Grund, warum diese Erweiterung als "kein architektonischer Bruch" gilt.

---

## 3. `memfuse-db`: `ContextCompactor`-Integration von `ProvenanceRecord`

**Korrigierter Ansatz** (siehe Abschnitt 0): Da `consolidate_via_llm()` aktuell keine Provenienz-Kette erzeugt, sondern nur `source_doc_ids`, ist die Änderung eine **echte Erweiterung**, keine Vertiefung eines bestehenden Feldes.

```rust
impl ContextCompactor {
    /// Wie `consolidate_via_llm()`, aber erzeugt zusätzlich einen
    /// `ProvenanceRecord` mit `ProvenanceOrigin::LlmConsolidation` und
    /// `prompt_hash` über den tatsächlich gesendeten Prompt-String.
    ///
    /// Direkt umsetzbar durch Erweiterung des bestehenden Rückgabetyps.
    /// Bewusst keine Breaking Change am bestehenden `consolidate_via_llm()` —
    /// neue Methode, um bestehende Aufrufer nicht zu brechen (ADR-035-konform:
    /// additive statt destruktive Änderung).
    pub async fn consolidate_via_llm_with_provenance(
        &self,
        chunks: &[ContextChunk],
        ollama: &OllamaClient,
        provenance_store: &dyn ProvenanceStore,
        tx: TxId,
    ) -> Result<(CompactedContext, ProvenanceRecord)> {
        let compacted = self.consolidate_via_llm(chunks, ollama).await?;

        let prompt_hash = /* SHA-256 über den intern gebauten `prompt`-String */;
        let derived_from: Vec<ProvenanceTarget> = chunks
            .iter()
            .map(|c| ProvenanceTarget::Chunk(c.doc_id))
            .collect();

        let record = ProvenanceRecord {
            provenance_id: /* nächste ID aus provenance_store */,
            target: ProvenanceTarget::Chunk(
                compacted.retained_chunks.first().map(|c| c.doc_id)
                    .unwrap_or_default(),
            ),
            origin: ProvenanceOrigin::LlmConsolidation {
                model: ollama.config().model.clone(),
            },
            written_at_tx: tx,
            prompt_hash: Some(prompt_hash),
            derived_from,
            integrity_hmac: /* HMAC über obige Felder, WalHmac-Kette fortsetzend */,
        };

        provenance_store.record_provenance(record.clone()).await?;
        Ok((compacted, record))
    }
}
```

### 3.1 Cache-bewusste Segmenttrennung (Prompt-Caching-Ökonomie)

**Forschungsbezug**: *"Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks"* (arXiv:2601.06007) und *"The Missing Memory Hierarchy: Demand Paging for LLM Context Windows"* (arXiv:2603.09023) — beide im ursprünglichen Strategiedokument zitiert; diese Feinspezifikation übersetzt das Prinzip in eine konkrete Signatur.

```rust
/// Erweiterung von `CompactedContext` um eine explizite Trennung in
/// cache-stabile (statische) und cache-volatile (dynamische) Segmente,
/// damit `memfuse-ollama` beim Prompt-Aufbau den statischen Anteil an
/// den Anfang stellen kann (Voraussetzung für Prefix-Caching in den
/// meisten Ollama-/llama.cpp-Backends).
#[derive(Debug, Clone)]
pub struct CacheAwareContext {
    /// Unverändert über mehrere Turns hinweg (System-Prompt-Anteil,
    /// stabile Nutzerprofil-Fakten). Reihenfolge deterministisch,
    /// damit der Cache-Präfix stabil bleibt.
    pub static_segment: Vec<ContextChunk>,
    /// Ändert sich pro Turn (Tool-Outputs, aktuelle Suchergebnisse).
    pub dynamic_segment: Vec<ContextChunk>,
}

impl ContextCompactor {
    /// Klassifiziert Chunks nach Cache-Stabilität, bevor `compact()`
    /// aufgerufen wird. Heuristik: Chunks mit Metadata-Key "session_scoped"
    /// oder Alter > `stability_threshold_turns` gelten als statisch.
    pub fn partition_by_cache_stability(
        &self,
        chunks: Vec<ContextChunk>,
        stability_threshold_turns: u32,
    ) -> CacheAwareContext {
        /* ... */
    }
}
```

---

## 4. `memfuse-router`: Kalibriertes Kaskaden-Routing

**Forschungsbezug**: UCCI (arXiv:2605.18796, verifiziert — Varun Kotte, 11. Mai 2026). Konkrete, verifizierte Zahlen aus dem Paper-Abstract: isotonische Regression zur Kalibrierung von Token-Margin-Unsicherheit auf eine Fehlerwahrscheinlichkeit pro Anfrage; auf einem Produktions-NER-Workload (75.000 Anfragen, 4B/12B-Modell-Kaskade auf H100) senkt UCCI die Inferenzkosten um **31 % (95%-CI: [27 %, 35 %])** bei stabiler Micro-F1 und reduziert den Expected Calibration Error (ECE) von **0,12 auf 0,03**. Isotonische Kalibrierung erreicht nachweislich `O(n^(-1/3))`-Stichprobenkomplexität für ECE. Ergänzend: *"Cluster, Route, Escalate"* (arXiv:2606.27457) für das zweistufige Cluster→Eskalations-Verfahren.

**Diagnose am realen Code** (siehe Abschnitt 0, bestätigt): `RouterEngine::route()` verwendet aktuell `chunk.relevance` roh mit einem statischen `1.2×`-Faktor bei Community-Treffer — kein Kalibrierungsschritt.

```rust
/// Isotonische Kalibrierungsfunktion: bildet einen rohen Margin-/Konfidenz-
/// score auf eine kalibrierte Fehlerwahrscheinlichkeit ab.
///
/// Isotonic Regression statt Temperature Scaling, weil UCCI (arXiv:2605.18796,
/// Table 3) zeigt: ECE 0,03 (isotonisch) vs. 0,08 (Temperature Scaling)
/// bei gleichem Kalibrierungsbudget.
pub struct IsotonicCalibrator {
    /// Stückweise monotone Kalibrierungskurve, gefittet auf einem
    /// Hold-out-Kalibrierungsset historischer Routing-Entscheidungen
    /// mit bekanntem Korrektheits-Label.
    breakpoints: Vec<(f32 /* raw_score */, f32 /* calibrated_error_prob */)>,
}

impl IsotonicCalibrator {
    /// Fittet die Kalibrierungskurve via Pool-Adjacent-Violators-Algorithmus (PAVA)
    /// auf `(raw_score, was_correct)`-Paaren aus historischen Routing-Logs.
    pub fn fit(samples: &[(f32, bool)]) -> Self { /* ... */ }

    /// Bildet einen rohen Score auf eine kalibrierte Fehlerwahrscheinlichkeit
    /// in [0.0, 1.0] ab (stückweise linear zwischen Breakpoints).
    pub fn calibrate(&self, raw_score: f32) -> f32 { /* ... */ }
}

/// Konfiguration für kostenoptimale Eskalationsentscheidung
/// (UCCI: "threshold policies on the calibrated score are cost-optimal
/// under three explicit assumptions").
pub struct CascadeCostConfig {
    /// Kosten einer Anfrage am kleinen/lokalen Profil (z. B. Token-Zeit in ms).
    pub cost_small_profile: f32,
    /// Kosten einer Anfrage am eskalierten/größeren Profil.
    pub cost_escalated_profile: f32,
    /// Kosten eines Fehlers (task-spezifisch, z. B. Kosten einer
    /// fehlerhaften Antwort in einem Support-Kontext).
    pub cost_of_error: f32,
}

impl RouterEngine {
    /// Wie `route()`, aber mit vorgeschaltetem Kalibrierungsschritt:
    /// der aggregierte Rohscore wird durch `calibrator.calibrate()`
    /// geführt, bevor die Eskalationsentscheidung (Wahl des SLM-Profils)
    /// getroffen wird. Eskaliert genau dann, wenn die kalibrierte
    /// Fehlerwahrscheinlichkeit mal `cost_of_error` die Kostendifferenz
    /// zum stärkeren Profil übersteigt (UCCI-Kostenminimierung).
    pub async fn route_calibrated(
        &self,
        query_embedding: &[f32],
        query_text: &str,
        calibrator: &IsotonicCalibrator,
        cost_config: &CascadeCostConfig,
    ) -> Result<RoutingDecision> {
        /* Wiederverwendung der bestehenden hybrid_search_with_strategy()-
           und Community-Score-Logik aus route(), aber Ersetzung von
           Schritt 3 ("Select profile with highest aggregated relevance
           score") durch:
           1. calibrated_error_prob = calibrator.calibrate(aggregated_score)
           2. expected_cost_small = cost_small_profile
                + calibrated_error_prob * cost_config.cost_of_error
           3. expected_cost_escalated = cost_escalated_profile
           4. Eskaliere, wenn expected_cost_small > expected_cost_escalated */
        todo!()
    }
}
```

**Wichtige Einschränkung** (Ehrlichkeitsprinzip, wie im UCCI-Paper selbst benannt): Die Kalibrierungskurve muss auf einem Hold-out-Set mit **bekannten Korrektheits-Labels** gefittet werden — MemFuse müsste dafür ein Feedback-Signal einführen (z. B. explizite Nutzer-Korrektur eines SLM-Outputs), das aktuell nicht existiert. Ohne dieses Signal bleibt `IsotonicCalibrator::fit()` unbenutzbar; das ist eine Voraussetzung, keine automatische Verbesserung.

---

## 5. `memfuse-agent`: Sleep-Cycle-Konsolidierung & proaktive Foresight-Events

### 5.1 Sleep-Cycle als `AgentTool`

**Forschungsbezug**: Auto-Dreamer (arXiv:2605.20616, verifiziert — Chongrui Ye et al., 20. Mai 2026). Zentrales, verifiziertes Zitat aus dem Volltext: der Konsolidator *"treats a selected working region as read-only evidence"* und führt *"region rewriting"* durch — ein gelernter, mehrschrittiger, werkzeugnutzender Konsolidator ersetzt einen Speicherbereich vollständig durch eine neu synthetisierte, kompaktere Version, statt einzelne Einträge per CRUD zu ändern (im Gegensatz zu "LightMem", das Auto-Dreamer explizit als Vergleichsbaseline nennt). Ergänzend SCM (arXiv:2604.20943, verifiziert — Saish Sachin Shinde): NREM-artige Konsolidierung (Verstärkung wichtiger Assoziationen), REM-artiges "Dreaming" (Erzeugung neuer Assoziationen zwischen hochwichtigen Konzepten) und ein "ForgettingModule" mit mehrdimensionalem Wichtigkeits-Score — im Paper mit **90,9 % Rausch-Reduktion** bei perfektem Recall über zehn Turns demonstriert (Benchmark-Zahl, nicht production-verifiziert).

```rust
/// Sleep-Cycle-Konsolidierungs-Tool: nimmt einen "Working Region"-Ausschnitt
/// des Gedächtnisses (nach Zeitfenster oder Community-ID begrenzt),
/// behandelt ihn als schreibgeschützte Evidenz, und ersetzt ihn durch
/// eine kompaktere, LLM-synthetisierte Version — analog zu Auto-Dreamers
/// "region rewriting"-Prinzip.
///
/// Implementiert `AgentTool`, läuft also im bestehenden
/// Checkpoint→Execute→Commit→Audit-Loop (OrchestratorEngine), nicht
/// als separater Prozess — das ist der konkrete Grund, warum dies
/// "kein architektonischer Bruch" ist.
pub struct SleepCycleConsolidator {
    ollama: Arc<OllamaClient>,
    provenance_store: Arc<dyn ProvenanceStore>,
}

#[async_trait::async_trait]
impl AgentTool for SleepCycleConsolidator {
    fn name(&self) -> &str { "sleep_cycle_consolidator" }

    /// Erwartet als `input`:
    /// { "working_region": { "community_id": u64, "max_age_tx": TxId } }
    ///
    /// 1. Liest alle ContextChunks der Working Region (read-only).
    /// 2. Ruft `provenance_store.provenance_chain()` für jeden Chunk ab,
    ///    um NUR Chunks mit vollständiger, verifizierbarer Herkunft zu
    ///    konsolidieren (Sicherheitsgrenze: unverifizierte Chunks werden
    ///    übersprungen und im StepResult als "skipped_unverified" gemeldet —
    ///    direkte Umsetzung des Auto-Dreamer-Prinzips "provenance-linked
    ///    source trajectories" als Zugriffsvoraussetzung).
    /// 3. Synthetisiert einen Ersatzsatz via
    ///    `ContextCompactor::consolidate_via_llm_with_provenance()`.
    /// 4. Markiert die Original-Chunks als tombstoned (nicht sofort gelöscht —
    ///    Löschung erst nach ADR-036-konformer Nachweisführung, siehe
    ///    Abschnitt 6 "Verified Forgetting").
    async fn execute(
        &self,
        ctx: &AgentContext,
        input: serde_json::Value,
    ) -> Result<StepResult> {
        /* ... */
    }
}
```

**Single-Writer-Invariante** (übernommen aus dem community-dokumentierten Anthropic-"Auto Dream"-Muster, siehe Abschnitt 9 zur Quellenlage dieses spezifischen Punkts): `SleepCycleConsolidator` darf pro Working Region nur von genau einer `OrchestratorEngine`-Instanz gleichzeitig ausgeführt werden. Umsetzung: ein Lock-Eintrag `__agent:sleep_lock:{community_id}` im LSM-Store mit TTL, geprüft vor Schritt 1.

### 5.2 Proaktive Foresight-Events (CogniFold/EverMemOS-Muster)

**Forschungsbezug**: Zwei neu identifizierte, hochrelevante Quellen, die über die bisherige Strategiedokument-Recherche hinausgehen:

- **EverMemOS** (arXiv:2601.02163, verifiziert): definiert `MemCell = (E, F, P, M)` mit `P` als *"foresight/prospection with validity intervals (temporary plans/states)"* — ein Gedächtniseintrag trägt nicht nur Vergangenheit, sondern eine zeitlich begrenzte Erwartung an zukünftige Relevanz.
- **CogniFold** (arXiv:2605.13438, verifiziert — Wang et al., 13. Mai 2026, "Always-On Proactive Memory via Cognitive Folding"): geht noch einen Schritt weiter und beschreibt Agenten-Gedächtnis als *"predominantly reactive and retrieval-based"* im Status quo, während CogniFold Ereignisströme **kontinuierlich, unaufgefordert** in kognitive Strukturen faltet — inklusive automatischer Reaktivierung verwandter, ruhender Konzepte, wenn ein neues Ereignis eintrifft (Beispiel aus dem Paper: die Erwähnung eines Wien-Konzerts reaktiviert automatisch die zuvor gespeicherte "Wien-Hotel"-Information). Dies ist die direkteste, bisher nicht in die MemFuse-Strategie aufgenommene Quelle für "proaktive Chat-Optimierung".

```rust
/// Ein zeitlich begrenztes Antizipations-Signal, das MemFuse aus dem
/// Gesprächsverlauf ableitet (EverMemOS-"Foresight"-Konzept).
///
/// Beispiel: Nutzer erwähnt "nächste Woche fahre ich nach Wien" →
/// ForesightSignal mit validity_window = [heute, +14 Tage], das bei
/// jeder folgenden Anfrage mit thematischem Wien-Bezug automatisch
/// mit erhöhter Priorität in den Retrieval-Kontext einfließt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForesightSignal {
    pub entity_id: EntityId,
    pub anticipated_topic: String,
    pub validity_from: TxId,
    pub validity_to: TxId,
    /// LLM-geschätzte Wahrscheinlichkeit, dass dieses Signal noch
    /// innerhalb des Gültigkeitsfensters relevant wird.
    pub confidence: f32,
}

/// Reaktivierungs-Event: wenn ein neu eintreffender Chunk semantisch
/// nahe an einem "ruhenden" (lange nicht abgerufenen, aber nicht
/// gelöschten) Entity liegt, wird dieses Event ausgelöst, BEVOR der
/// Nutzer danach fragt — CogniFold-Prinzip der unaufgeforderten
/// Reaktivierung.
///
/// Konkrete Umsetzbarkeit: `EventSource`-Trait (bereits in
/// `memfuse-agent/src/event_source.rs` vorhanden, siehe
/// `PollingDocumentEventSource`) ist die naheliegende Erweiterungsstelle —
/// ein neuer `DormantEntityReactivationSource: EventSource`, der bei
/// jedem neu committeten Chunk eine PPR-Nachbarschaftssuche über lange
/// nicht besuchte Entities durchführt (Schwellenwert: `last_accessed_tx`
/// älter als N Transaktionen) und bei Treffer ein `BackgroundEvent`
/// emittiert.
pub struct DormantEntityReactivationSource {
    graph: Arc<dyn GraphIndex>,
    dormancy_threshold_tx_delta: u64,
}

#[async_trait::async_trait]
impl EventSource for DormantEntityReactivationSource {
    async fn poll(&self) -> Result<Vec<BackgroundEvent>> {
        /* Nutzt CsrGraph::personalized_page_rank() auf neu committeten
           Entities als Seed, filtert Ergebnisse auf last_accessed_tx
           älter als dormancy_threshold_tx_delta, emittiert pro Treffer
           ein BackgroundEvent mit reaktiviertem EntityId als Payload. */
    }
}
```

**Ehrliche Einordnung**: CogniFold ist ein Forschungspreprint (Stand v4, 5. August 2026), keine production-battle-tested Referenzimplementierung. Die MemFuse-Umsetzung sollte, analog zur bereits im Strategiedokument getroffenen Einordnung zu Sleep-Time-Compute, als **eigenständige, konservative Adaption** verstanden werden — insbesondere die "automatische Reaktivierung" birgt ein Risiko für unerwünschtes proaktives Verhalten (Privacy: Nutzer könnte nicht wollen, dass ruhende Themen unaufgefordert wieder auftauchen). Eine `DormantEntityReactivationSource` sollte daher **standardmäßig deaktiviert** und nur bei explizitem Opt-in aktiv sein.

---

## 6. `memfuse-crypto`: Kryptographischer Löschbeweis (Verified Forgetting)

**Forschungsbezug**: Letztes, technisch anspruchsvollstes VMG-Primitiv aus arXiv:2604.16548 (verifiziert). SCM (arXiv:2604.20943) nennt algorithmisches Vergessen zudem explizit als Compliance-relevant: *"intentional forgetting reduces memory bloat, limits data retention, and may help address privacy concerns by enabling users to have specific information pruned"*.

**Reale Grundlage im Code** (verifiziert): `memfuse-crypto/src/wal_crypto.rs` enthält bereits `WalHmac` (HMAC-SHA256-Kette) und einen `WalEntrySnapshot`-Typ mit `prev_hmac`-Feld für Hash-Chain-Kontinuität. Das ist exakt die Infrastruktur, auf der ein Löschbeweis aufbauen kann.

```rust
/// Kryptographischer Nachweis, dass ein bestimmter Schlüssel (DocId/
/// EntityId) in KEINEM aktiven WAL-Segment und KEINER SSTable mehr
/// referenziert wird — nicht nur "als gelöscht markiert" (Tombstone),
/// sondern durch eine Merkle-Struktur über alle noch aktiven Segmente
/// beweisbar abwesend.
pub struct DeletionProof {
    pub target: ProvenanceTarget,
    /// Merkle-Root über alle SSTable-Segment-Hashes, die zum Zeitpunkt
    /// der Löschprüfung aktiv waren.
    pub merkle_root_at_deletion: [u8; 32],
    /// Merkle-Pfad, der beweist: target ist in keinem Blatt enthalten,
    /// das zu merkle_root_at_deletion beiträgt.
    pub absence_proof_path: Vec<[u8; 32]>,
    /// HMAC über obige Felder, WalHmac-Kette fortsetzend (Wiederverwendung
    /// von `WalHmac::new()`/`update()`/`finalize()`).
    pub proof_hmac: [u8; 32],
    pub verified_at_tx: TxId,
}

pub trait VerifiedForgetting: Send + Sync + 'static {
    /// Erzeugt einen `DeletionProof` NACHDEM eine LSM-Kompaktierung
    /// (`CsrGraph::compact()`-Analogon auf Storage-Ebene) das Ziel
    /// tatsächlich aus allen aktiven Segmenten entfernt hat.
    ///
    /// # Errors
    /// `MemFuseError::CapabilityUnsupported("verified_forgetting")` falls
    /// nicht implementiert — dies ist der technisch anspruchsvollste
    /// Punkt der gesamten Spezifikation und bewusst als letzter
    /// Roadmap-Schritt vorgesehen (siehe Strategiedokument Abschnitt 6).
    fn prove_deletion(&self, target: &ProvenanceTarget) -> Result<DeletionProof> {
        let _ = target;
        Err(MemFuseError::capability_unsupported(
            "verified_forgetting",
            "Cryptographic deletion proof is not supported by default — final VMG primitive, tracked long-term",
        ))
    }

    /// Verifiziert einen zuvor erzeugten `DeletionProof` unabhängig
    /// (z. B. durch einen externen Auditor ohne Schreibzugriff).
    fn verify_deletion_proof(&self, proof: &DeletionProof) -> bool {
        let _ = proof;
        false
    }
}
```

---

## 7. `memfuse-mcp`: Schreibautorisierungs-Gate

**Forschungsbezug**: VMG-Primitiv "Write Authorization" (arXiv:2604.16548) plus die konkrete Angriffsklasse aus MemoryGraft (arXiv:2512.16962, im Referenzverzeichnis des MemAudit-Papers zitiert, verifiziert über arXiv:2605.23723-Referenzliste) und "Non-Malleable, Origin-Bound Authority" (arXiv:2606.24322, verifiziert): dort wird explizit demonstriert, dass ein Angreifer *"untrusted content in one session"* speichern kann, das später *"a consequential action"* in einer **zukünftigen** Sitzung steuert — der Grund, warum reine Schreibvalidierung zum Zeitpunkt des Schreibens nicht ausreicht, sondern die Autorität an die Herkunft gebunden sein muss ("origin-bound").

**Reale Grundlage im Code**: `McpSandbox` existiert bereits (`docs/SOURCE_OF_TRUTH.md`: *"DB-Reads erlaubt, DB-Writes und Code-Execution opt-in"*). Die Erweiterung fügt eine inhaltliche Prüfung VOR dem bereits vorhandenen Opt-in-Gate hinzu.

```rust
/// Prüft vor jedem opt-in-genehmigten DB-Write eines MCP-Clients, ob der
/// zu schreibende Inhalt eine Autoritätskette besitzt, die auf eine
/// vertrauenswürdige Quelle zurückgeführt werden kann — nicht nur, OB
/// geschrieben werden darf (bestehendes SandboxPolicy-Opt-in), sondern
/// WAS in welcher Rolle geschrieben werden darf.
pub trait WriteAuthorizationGate: Send + Sync + 'static {
    /// Gibt `Ok(())` nur zurück, wenn `origin` eine Schreibautorität für
    /// `target_capability` besitzt (z. B. ein MCP-Client darf Chunks
    /// schreiben, aber keine Provenance-Records fälschen, die eine
    /// LlmConsolidation vortäuschen, die nie stattfand).
    fn authorize_write(
        &self,
        origin: &ProvenanceOrigin,
        target_capability: WriteCapability,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCapability {
    ContentChunk,
    EntityGraph,
    /// Bewusst eigene, restriktivere Capability: kein MCP-Client darf
    /// per Default Provenance-Records mit ProvenanceOrigin::LlmConsolidation
    /// oder SleepCycleConsolidation vortäuschen — nur die entsprechenden
    /// internen Subsysteme selbst dürfen das.
    ProvenanceAssertion,
}
```

---

## 8. Explizite Pfad-Extraktion (PathRAG-Stil) als dritte `GraphTraversalStrategy`

**Forschungsbezug**: PathRAG (2025, in mehreren 2026er-Surveys referenziert, u. a. der von Shibui Yusuke zitierten Survey arXiv:2602.05665 sowie im Kontext von CausalRAG2/HugRAG arXiv:2602.05143 als Vergleichsbaseline erwähnt) und HG-RAG (arXiv:2607.14095). Kernidee: statt eines vollständigen Subgraphen oder reiner Knoten-Score-Liste werden explizite **relationale Pfade** extrahiert und in Text-Prompts umgewandelt — kohärenter für nachgelagerte LLM-Verarbeitung als eine unstrukturierte Knotenliste.

```rust
/// Dritte Retrieval-Strategie neben Hops (BFS) und PersonalizedPageRank.
/// Extrahiert die K besten zusammenhängenden Pfade zwischen Seed-Nodes
/// statt einer flachen, nach Score sortierten Knotenliste.
pub enum GraphTraversalStrategy {
    Hops { max_hops: usize },
    PersonalizedPageRank(PprConfig),
    /// Neu: gibt explizite Pfade zurück, die direkt als
    /// "A → (Relation) → B → (Relation) → C"-Text formatiert werden
    /// können — bessere LLM-Prompt-Kohärenz laut PathRAG-Motivation.
    PathExtraction {
        max_path_length: usize,
        top_k_paths: usize,
    },
}

/// Ergebnis der PathExtraction-Strategie: eine geordnete Kantensequenz
/// statt einer Knoten-Score-Liste.
pub struct ExtractedPath {
    pub nodes: Vec<EntityId>,
    pub edge_labels: Vec<String>,
    pub cumulative_score: f32,
}

impl CsrGraph {
    /// Nutzt die bestehende CSR-Traversal-Infrastruktur
    /// (`offsets`/`targets`/`weights`, siehe `traverse_at_time`), aber
    /// hält bei jedem BFS-Schritt den vollständigen Pfad statt nur den
    /// aktuellen Score fest, und dedupliziert am Ende auf die
    /// `top_k_paths` mit höchstem `cumulative_score`.
    pub async fn extract_paths(
        &self,
        seed_nodes: &[EntityId],
        max_path_length: usize,
        top_k_paths: usize,
    ) -> Result<Vec<ExtractedPath>> {
        /* ... */
    }
}
```

---

## 9. Quellenregister mit Verifikationsstatus

Alle Quellen wurden am 29. August 2026 per Websuche direkt gegen arXiv/ACL Anthology/Autoren-Repositorien geprüft (nicht aus Trainingswissen unverifiziert übernommen).

| Kürzel | Titel | arXiv-ID | Status | Relevanz für diese Spezifikation |
|---|---|---|---|---|
| MAGMA | A Multi-Graph based Agentic Memory Architecture for AI Agents | 2601.03236 | ✅ Verifiziert, ACL 2026 Main Conference (aclanthology.org/2026.acl-long.1709) | Abschnitte 1.2, 1.3, 2 |
| EverMemOS | A Self-Organizing Memory Operating System for Structured Long-Horizon Reasoning | 2601.02163 | ✅ Verifiziert (v2, 9. Jan. 2026) | Abschnitt 5.2 |
| CogniFold | Always-On Proactive Memory via Cognitive Folding | 2605.13438 | ✅ Verifiziert (v4, 5. Aug. 2026) — **neu identifiziert, nicht im vorherigen Strategiedokument** | Abschnitt 5.2 |
| VMG-Survey | A Survey on Long-Term Memory Security in LLM Agents (Toward Mnemonic Sovereignty) | 2604.16548 | ✅ Verifiziert (MemTensor Shanghai, 17. Apr. 2026) | Abschnitte 1.1, 6, 7 |
| MemAudit | Post-hoc Auditing of Poisoned Agent Memory via Causal Attribution and Structural Anomaly Detection | 2605.23723 | ✅ Verifiziert (Referenzliste gegengeprüft) | Strategiedokument 4.2 (unverändert) |
| MemLineage | Lineage-guided enforcement for LLM agent memory | 2605.14421 | ✅ Verifiziert — **neu identifiziert** | Abschnitt 1.1 |
| Origin-Bound Authority | Securing LLM-Agent Long-Term Memory Against Poisoning: Non-Malleable, Origin-Bound Authority | 2606.24322 | ✅ Verifiziert — **neu identifiziert** | Abschnitt 7 |
| Auto-Dreamer | Learning Offline Memory Consolidation for Language Agents | 2605.20616 | ✅ Verifiziert (Chongrui Ye et al., 20. Mai 2026) | Abschnitt 5.1 |
| SCM | Sleep-Consolidated Memory with Algorithmic Forgetting for LLMs | 2604.20943 | ✅ Verifiziert (Shinde, Forschungsvorschau, kein Peer-Review) | Abschnitt 5.1, 6 |
| CausalRAG2 / HugRAG | Hierarchical Causal Knowledge Graph Design for RAG | 2602.05143 | ✅ Verifiziert — v1 (Feb. 2026) als "HugRAG", v2 (Juni 2026) als "CausalRAG2", ICML 2026 angenommen | Abschnitte 1.2, 1.3 |
| UCCI | Calibrated Uncertainty for Cost-Optimal LLM Cascade Routing | 2605.18796 | ✅ Verifiziert (Kotte, 11. Mai 2026) — konkrete Zahlen bestätigt (31 % Kostenreduktion, ECE 0,12→0,03) | Abschnitt 4 |
| PathRAG | (referenziert in Surveys, kein eigener dedizierter arXiv-Check in dieser Runde durchgeführt) | — | 🟡 Nur indirekt verifiziert (über Zitationen in arXiv:2602.05665, arXiv:2602.05143) | Abschnitt 8 |

**Hinweis zu PathRAG**: Im Gegensatz zu allen anderen Quellen wurde PathRAG selbst nicht direkt per Volltext-Websuche isoliert verifiziert, sondern nur über Zitationskontext in anderen, direkt verifizierten Papieren bestätigt. Abschnitt 8 sollte vor Implementierung durch eine dedizierte Suche nach der PathRAG-Primärquelle ergänzt werden.

---

## 10. Was bewusst nicht spezifiziert wird

Konsistent mit dem Strategiedokument (Abschnitt 4.6/8) werden folgende Themen hier **nicht** in Schnittstellen übersetzt, weil sie außerhalb des sauber implementierbaren Scopes von MemFuse liegen:

- **KV-Cache-Kompression/-Quantisierung, Speculative Decoding**: Diese Techniken operieren auf der Ebene der Inferenz-Engine (llama.cpp/Ollama-Runtime), nicht auf der einer externen Rust-Datenbank. Eine Schnittstellenspezifikation dafür würde MemFuse zu einer eigenen Inferenz-Engine machen — außerhalb des Kern-Scopes.
- **RAGCache/RetrievalAttention als eigene KV-Cache-Kooperationsschicht**: Was MemFuse stattdessen sauber anbieten kann, ist bereits in Abschnitt 3.1 (`CacheAwareContext`) spezifiziert — die vorgelagerte Rolle als strukturierter Wissens-Cache für Ollama, nicht die KV-Cache-Manipulation selbst.
- **UCCI-Kalibrierung ohne Feedback-Signal** (siehe Einschränkung in Abschnitt 4): Die Schnittstelle ist spezifiziert, aber ohne ein noch zu bauendes Korrektheits-Feedback-Signal nicht sinnvoll nutzbar — das ist eine Voraussetzung, kein separates Feature dieser Spezifikation.
- **Vollständige Verifizierung der Anthropic-"Auto Dream"-Referenz**: Der Bezug zu Anthropics eigenem, internem "Auto Dream"-Konzept bleibt community-dokumentiert (siehe die im Zuge dieser Recherche gefundene Quelle "Before Anthropic Dreams: a short lineage of memory consolidation", die eine Genealogie von einem Karpathy-Gist über SCM zu Anthropics Dreams-Feature nachzeichnet) — nicht offiziell von Anthropic publiziert. Diese Spezifikation lehnt das Single-Writer-Invariante-Prinzip (Abschnitt 5.1) an dieses community-Muster an, ohne einen Anspruch auf Übereinstimmung mit der tatsächlichen internen Anthropic-Implementierung zu erheben.
