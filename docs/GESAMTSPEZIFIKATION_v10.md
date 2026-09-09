# MemFuse — Gesamtspezifikation v10.0
## Einzige normative Wahrheitsquelle · Synthetisiert aus 12 Strategiedokumenten
> **Änderungsvermerk (Konsolidierung v10 / v10.1):** 2026-09-09 — Konsolidierung aller v10/v10.1 Spezifikationsinhalte, Behebung verwaister Pfad-Referenzen (u.a. `OFFEN-11`), Entfernung von Root-Duplikaten gemäß ADR-078.
> **Ersetzt:** alle Vorgängerdokumente (v4.0–v9.0, alle docs/NEW_STRATEGY/*.md)
> **Stand:** 2026-09-09
> **HEAD zum Zeitpunkt der Synthese:** `HEAD 92b22c9535eb0a9be2faeaefbc2cc86ea9a7ffc2, 2026-09-09 12:47:04 +0000`
> **Syntheseprinzip:** Jede Aussage ist entweder (a) per grep/read am Live-Code
> verifiziert, oder (b) als verbindliche Entscheidung aus dem Entscheidungsdokument
> (v9.0 §1–§3, Entscheidungen v1/v2) übernommen, oder (c) als offener Punkt mit
> explizitem "OFFEN:" gekennzeichnet. Keine dritte Kategorie.

---

## §0 — Warum dieses Dokument existiert und was es ablöst

### §0.1 Das Kernproblem: Kapazität kompensiert fehlende Entscheidung
In der bisherigen Entwicklung von MemFuse führte eine hohe Entwicklungsgeschwindigkeit (~195 Tasks/Tag in Spitzenzeiten) dazu, dass unentschiedene Architekturfragen von parallelen Agenten-Sessions mehrfach und auf widersprüchliche Weise gelöst wurden.
Beispiele aus der Git-Historien-Analyse belegen dieses Muster eindrucksvoll:
- `ConfigFingerprint` wurde am 07.09.2026 innerhalb von 66 Minuten dreimal unabhängig voneinander in verschiedenen Architektur-Varianten implementiert (#1627, #1634, #1645), was zu einem unbemerkten Kompilierfehler auf `main` führte.
- `ADR-070` existierte vierfach und `ADR-072` zweifach in `docs/decisions/`.
- Drei Produktvisionen (PyPI-Library, Desktop-Enterprise-App, Voice-Assistant) galten in älteren ADRs (z.B. ADR-018) zeitgleich als "final".

### §0.2 Veraltung von Vorgängerspezifikationen (v4.0–v9.0)
Alle vorherigen Spezifikationsdokumente (v4.0, v7.0, v8.0, v9.0 sowie die Einzelanalysen unter `docs/NEW_STRATEGY/`) hatten bei einer Kadenz von über 70 Commits pro Tag eine Halbwertszeit von wenigen Tagen oder Stunden. Sie enthielten ungelöste Drei-Wege-Optionslisten, biologische Metaphern als Primärtypen und veraltete Annahmen über den Stand der Code-Implementierung.

### §0.3 Das Funktionsprinzip der Gesamtspezifikation v10.0
Die Gesamtspezifikation v10.0 beendet diesen Zustand dauerhaft:
1. **Live-Code-Verifikation schlägt Spezifikation:** Der tatsächliche Rust-Code auf `HEAD` bildet das Fundament.
2. **Entscheidungen statt Optionslisten:** Jede Architekturfrage ist entschieden (z. B. PyPI-Library, Position A per ADR-077).
3. **Maschinenlesbare Invarianten:** Alle offenen Punkte sind eindeutig klassifiziert und mit Prioritäten versehen (`OFFEN-01` bis `OFFEN-12`).

---

## §1 — Produktvision (verbindlich, keine Optionsliste)

**Entscheidung:** PyPI-Library, Position A ("Schlanker als MinnsDB, Krypto-Härtung als Alleinstellungsmerkmal", ADR-077, ADR-007-Richtung).

### §1.1 Was MemFuse IST
MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten und wissensintensive Einzelanwender. Es wird primär als Python-Library via PyPI (`pip install memfuse`) und als Rust-Crate via `crates.io` verteilt.

### §1.2 Alleinstellungsmerkmale
1. **Kryptographische Integrität & Löschung:** MFW3 HMAC-WAL-Kette und verifizierbarer `DeletionProof` für DSGVO Art. 17 auf Storage-Ebene.
2. **Outcome-kalibriertes Retrieval:** Conformal-Router mit `ConfigFingerprint`-Invalidierungsschutz (P8) und proaktiver Lyapunov-Drift-Überwachung (F-11).
3. **Erweiterte Retrieval-Signale:** Dual-Process Memory, BM25 mit deutscher Kompositum-Dekomposition, 3-Signal-RRF mit Resonanz-Kohärenz-Bonus (F-09) und Synaptisch-Hebbianischem Signal (F-03).
4. **Typ-sichere Lock-Infrastruktur:** Session-DAG mit `NodesGuard` zur strikten Verhinderung von Deadlocks auf Architekturebene.

### §1.3 Was MemFuse NICHT ist
- Kein Cloud-SaaS und keine externe API-Abhängigkeit im Kernbetrieb.
- Kein Enterprise-Multi-Tenant-Produkt (Multi-Tenant-Typen dienen der Prozess- und Test-Isolation).
- Keine Desktop-App (`memfuse-tauri` ist offiziell `deprecated` gemäß ADR-077 §3 und wird physisch entfernt).

### §1.4 Revisionsklausel
Die Entscheidung für Position A wird durch kontinuierliche LongMemEval- und LoCoMo-Benchmarks gegen MinnsDB empirisch verankert. Eine Re-Evaluierung erfolgt ausschließlich per neuer, formaler ADR, falls empirische Messungen eine Kurskorrektur zwingend erfordern.

---

## §2 — Architekturprinzipien P1–P19

- **P1 — DAG-Integrität:** `cargo xtask check-dag` ist ein zwingendes CI-Gate. Kein Code in Layer $N$ darf Abhängigkeiten auf Layer $>N$ besitzen.
- **P2 — Zero-Panic-Doctrine:** Production-Code ist panic-frei. `unsafe` ist strikt beschränkt auf `distance.rs` (SIMD), `diskann.rs` & `persistence.rs` (Mmap) mit verpflichtendem `// SAFETY:`-Proof. `memfuse-py` verwendet ein eigenes Workspace-Profil (`panic = "unwind"`) mit `catch_unwind`-Isolation.
- **P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. `fsync` wird auf Datei- und Directory-Ebene durchgeführt.
- **P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` in `memfuse-core` sichern die Abstraktion ab.
- **P5 — Kein Cloud-Zwang:** Inferenz und Retrieval laufen vollständig lokal auf Nutzer-Hardware.
- **P6 — Eine Quelle für Architekturentscheidungen:** `DECISIONS.md` ist die einzige Single Source of Truth für ADRs (ADR-060).
- **P7 — Code-Nachweis-Pflicht für Marketing-Aussagen:** Quantitative Leistungsversprechen benötigen reproduzierbare Benchmark-Nachweise in `memfuse-bench`.
- **P8 — Kalibrierungs-Integrität:** Jede Änderung an `prompt_template_hash`, `temperature_bits` oder `quantization` invalidiert automatisch alle Kalibrierungsstatistiken (`IsotonicCalibrator::invalidate_on_config_change()`).
- **P9 — Kein Klartext-Sensitivspeicher:** Sensitiver Tensor-Speicher wird im VRAM/RAM nach der Nutzung via `ZeroizeOnDrop` überschrieben.
- **P10 — Reuse vor Neubau:** Vor Neuanlage von Typen oder Modulen erfolgt die Prüfung gegen `TYPE_REGISTRY.md` und der CI-Gate-Check `check-duplicate-symbols` (ADR-065).
- **P11 — Latenzbudget-Pflicht für Hot-Path:** Deadlines und PID-Regler (`RerankPidController`) begrenzen P95-Retrieval-Latenzen.
- **P12 — Physio-Feature-Default-Unsichtbarkeit:** Alle `physio-*`-Features sind per Feature-Flag deaktivierbar. Defaults verhalten sich transparent.
- **P13 — Modulgrenzen nach Verantwortung:** Klare Trennung zwischen Layer 0 (Fundament) bis Layer 5/6 (Agenten/Interfaces).
- **P14 — Ein Scheduler pro Subsystem:** Konsolidierung von Hintergrund-Tasks im `PhysioScheduler` und `MaintenanceScheduler`.
- **P15 — Eine Vision pro Release:** Einzige Zielpositionierung ist die PyPI-Library (ADR-077).
- **P16 — Dokumente als Zieldefinitionen:** Dokumente beschreiben Soll-Zustände und maschinenlesbare Invarianten, keine ephemeral Code-Zeilen.
- **P17 — Ambient-Kontext ist keine Garantie:** `AGENTS.md` wird nicht automatisch vom LLM geladen. Jeder Trigger-Prompt erzwingt das Einlesen von `AGENTS.md` und `.jules/SESSION_BOOTSTRAP.md` als ersten Schritt.
- **P18 — Parallele Sessions brauchen Claims:** Parallele Bearbeitung desselben Crates erfordert die Registrierung eines Claims (`cargo xtask claim`).
- **P19 — Beschlossener, nicht umgesetzter Governance-Beschluss ist gefährlicher als keiner:** Beschlüsse werden entweder per CI-Gate durchgesetzt oder formal widerrufen.

---

## §3 — Crate-Topologie: Ist-Zustand und Ziel

### §3.1 Ist-Zustand (Live-verifiziert am HEAD)

Das Repository umfasst aktuell 18 Workspace-Mitglieder im Haupt-Workspace sowie ein isoliertes FFI-Workspace (`memfuse-py`):

| Crate | Layer | Status | Beschreibung |
|:---|:---:|:---:|:---|
| `memfuse-core` | 0 | ✅ KERN | Core-Typen, Traits, Domain-Primitiven |
| `memfuse-crypto` | 1 | ✅ KERN | AES-256-GCM-SIV, DeletionProof, HMAC-Chain |
| `memfuse-calibration` | 1 | ✅ KERN | IsotonicCalibrator, PlattScaler, ReplicatorState |
| `memfuse-checkpoint` | 1 | ⏳ Konsolidieren | Checkpoint- und Snapshot-Management |
| `memfuse-graph` | 1 | ✅ KERN | CSR-Graph, PPR, PathRAGEngine, ImmunMemory |
| `memfuse-text` | 1 | ✅ KERN | BM25+, DACH-Kompositum-Dekomposition |
| `memfuse-candle` | 1 | ⏳ Konsolidieren | Pure-Rust GGUF Inferenz & Embedding |
| `memfuse-store` | 2 | ✅ KERN | LSM-Tree, WAL v3, SSTable, Mmap |
| `memfuse-index` | 2 | ✅ KERN | HNSW, DiskANN (persist_delta), SIMD |
| `memfuse-kv-bridge` | 2 | ⏳ Konsolidieren | KV-Cache-Bridge Sicherheitsschicht |
| `memfuse-ollama` | 2 | ⏳ Konsolidieren | Ollama HTTP-Client & Prefix Engine |
| `memfuse-embed` | 2 | ⏳ Konsolidieren | Optionales ONNX-Embedding/Reranking |
| `memfuse-db` | 3 | ✅ KERN | Embedded Hybrid-Search & Collection Engine |
| `memfuse-router` | 4 | ⏳ Konsolidieren | Conformal Router & SLM-Profile |
| `memfuse-tauri` | 4 | 🗑️ DEPRECATED | Desktop App Shell (ADR-077, Frist läuft) |
| `memfuse-bench` | 4 | ✅ KERN | LongMemEval / LoCoMo Benchmark Harness |
| `memfuse-agent` | 5 | ✅ KERN | Persistent Agent Workflow Loop & DLQ |
| `memfuse-mcp` | 6 | ✅ KERN | MCP JSON-RPC 2.0 stdio Server (ADR-010) |
| `memfuse-py` | Grenzschicht | ✅ KERN | PyO3-Bindings (isoliertes Workspace, ADR-064) |

### §3.2 Ziel-Topologie (9–10 Crates)

Ziel ist die Konsolidierung der 18 Workspace-Mitglieder in 9–10 fokussierte Module:

```
KERN (6 Module):
  memfuse-core        [Layer 0 — Typen, Traits, Domain-Primitiven, ConfigFingerprint]
  memfuse-security    [Layer 0/1 — Fusion aus memfuse-crypto + memfuse-kv-bridge]
  memfuse-persistence [Layer 1 — Fusion aus memfuse-store + memfuse-checkpoint]
  memfuse-retrieval   [Layer 2 — Fusion aus memfuse-index + memfuse-graph + memfuse-text]
  memfuse-orchestrator[Layer 3 — memfuse-db, Scheduler-Konsolidierung]
  memfuse-inference   [Layer 3 — Fusion aus calibration + ollama + candle + router + embed]

GRENZSCHICHT (2–3 Module):
  memfuse-mcp         [MCP-Protokoll, stdio only, kein axum — ADR-010]
  memfuse-py          [PyPI-Primärkanal, PyO3-Bindings, panic=unwind]
  memfuse-agentic     [memfuse-agent, High-Level Workflow API]

WERKZEUG:
  xtask              [Build-Automatisierung, Governance-Gates]
  memfuse-bench      [Evaluation, LongMemEval / LoCoMo Harness]
```

### §3.3 Migrationsreihenfolge (5 Phasen)

- **Phase 1a (Security-Schicht):** Zusammenführung von `memfuse-crypto` und `memfuse-kv-bridge` in `memfuse-security`. Akzeptanz: Zeroize-on-Drop und AES-GCM-SIV voll integriert, 0 DAG-Verletzungen.
- **Phase 1b (Persistence-Schicht):** Zusammenführung von `memfuse-store` und `memfuse-checkpoint` in `memfuse-persistence`. Akzeptanz: LSM-Tree und CheckpointGuard unter einheitlicher Fassade.
- **Phase 2 (Inference-Schicht):** Konsolidierung von `calibration`, `ollama`, `candle`, `router` und `embed` in `memfuse-inference`. Akzeptanz: Bündelung aller LLM-/Embedding-Backends.
- **Phase 3 (Scheduler & Orchestration):** Zusammenführung von `reaper.rs`, `sleep_cycle.rs` und `physio_scheduler.rs` in `memfuse-orchestrator` (`memfuse-db`).
- **Phase 4 (Retrieval-Schicht):** Bündelung von `memfuse-index`, `memfuse-graph` und `memfuse-text` unter `memfuse-retrieval`. Akzeptanz: Unified 3-Signal Hybrid Search Engine.
- **Phase 5 (Abschluss & Bereinigung):** Entfernung verwaister Crates, finale DAG-Integritätsprüfung via `cargo xtask check-dag`.

*Constraint:* Kein Migrationsschritt startet vor der vollständigen Verankerung des Claim-Mechanismus (Phase B).

---

## §4 — Feature-Klassifikation (KERN / ENTFERNEN)

Gemäß v9.0 existieren strikt zwei Feature-Kategorien (KERN vs. ENTFERNEN/VETO):

| Feature | Code-Name (ADR-069) | Feature-Flag | Status | Begründung |
|:---|:---|:---|:---:|:---|
| F-01 | DecayController | — (kern) | ✅ KERN | Adaptiver Zerfall verbessert Retrieval-Relevanz |
| F-02 | PartialIndexRebuild | `partial-index-rebuild` | 🚫 VETO | HNSW Delaunay-Kollaps, VETOES.md |
| F-03 | EdgeReinforcement | `physio-synaptic-edges` | ⏳ FRIST | Integration in Fusion-Hot-Path ausstehend |
| F-04 | ConsistencyEnforcement | — (kern) | ✅ KERN | Immunologische Widerspruchsabwehr |
| F-05 | MemoryConsolidation | — (kern) | ✅ KERN | NREM/REM Sleep-Cycle Konsolidierung |
| F-06 | GraphConnectivityHealth | `graph-connectivity-health` | ✅ KERN | Perkolations-Gesundheitsmetrik für CSR |
| F-07 | ReplicatorDynamics | `physio-replicator-weights` | 🗑️ ENTFERNEN | Enterprise-Multi-Tenant / Inkompatibel mit Vision |
| F-08 | PidHomeostasis | — (kern) | ✅ KERN | PID-Latenzbudget-Einhaltung (P11) |
| F-09 | CoherenceBonus | `coherence-bonus-fusion` | ✅ KERN | Resonanz-Fusion, Signal-Kohärenz |
| F-10 | CrossTenantExchange | — | 🚫 VETO | Permanent rejected (bricht TenantId + DeletionProof) |
| F-11 | LyapunovDriftWatcher | — (kern) | ✅ KERN | Proaktive Kalibrierungs-Drift-Erkennung |

---

## §5 — Technische Kernarchitektur (Layer 0–5)

### Layer 0 — Fundament (`memfuse-core`, `memfuse-security` / `crypto`)
- `TenantId`: Mit `try_new()`-Guard zur Durchsetzung von `INV-TENANT-1` (`TenantId(0)` ist `SYSTEM`-reserviert).
- `ConfigFingerprint`: Invalidation-Fingerprint über `model_id`, `quantization`, `prompt_template_hash` und `temperature_bits`.
- `DeletionProof`: Kryptographischer Löschnachweis mit expliziter Deckungsgrenze (`ExcludedScope` deklariert Ausnahmen für Fine-Tuning und LLM-Parametergedächtnis).
- HMAC-WAL-Kette (WAL v3): Kryptographische Integritätskette zur Absicherung gegen Biting- und Tampering-Angriffe.

### Layer 1 — Storage-Primitiven (`memfuse-persistence` / `store`, `index`)
- LSM-Tree Engine: MemTable SkipList, SSTable mit Bloom-Filtern und CRC32-Verifikation. WAL-First Pflicht (P3).
- DiskANN Vector Index: Inkrementeller `persist_delta()`-Pfad mit atomarem Rename und Pending-WAL-Puffer.
- HNSW Index: SIMD-beschleunigte Distanzberechnung (AVX2/NEON), 2-Phasen CoW-Rebuild (ADR-061).

### Layer 2 — Orchestrierung & Fusion (`memfuse-orchestrator` / `db`, `graph`, `text`)
- Hybrid Search Engine: 3-Signal RRF (Vektor + BM25 + Graph) kombiniert mit F-09 Resonanz-Kohärenz-Bonus und F-08 PID-Regler.
- PathRAGEngine: Causal-Path Extraction auf CSR-Graph mit Sufficiency-Gate und Query-Klassifikation.
- Cascading-Invalidation: Invalidation von Supersedes-Chunks führt via `DocEdgeIndex` zum automatischen Tombstoning verknüpfter Graph-Kanten.
- MaintenanceScheduler: Konsolidierte Ausführung von Reclaim-, Cleanup- und MemoryConsolidation-Tasks.

### Layer 3 — Inferenz & Routing (`memfuse-inference` / `router`, `ollama`, `candle`)
- ConformalRouter: Outcome-kalibriertes Routing mit UCCI-Konfidenzintervallen und Abstention-Pfad.
- LyapunovDriftWatcher (F-11): Proaktive Überwachung der Non-Conformity-Score-Verteilung zur Erkennung von Distributional Shift.
- `memfuse-candle`: Native Pure-Rust GGUF Inferenz und Embedding Engine. (OFFEN-06: Ausstehende Anbindung an Serving-Pipeline).

### Layer 4 — Grenzschicht (`memfuse-mcp`, `memfuse-py`)
- `memfuse-mcp`: Model Context Protocol stdio Server, JSON-RPC 2.0, Zero-Trust Sandbox, kein axum/HTTP (ADR-010).
- `memfuse-py`: Primärer PyPI-Kanal, PyO3-Bindings, `panic = "unwind"` mit `catch_unwind`-Boundary (ADR-064).
- `memfuse-tauri`: DEPRECATED (ADR-077), verbleibt bis zum Ablauf der 60-Tage-Frist.

### Layer 5 — Evaluation (`memfuse-bench`)
- LongMemEval Harness: Systematische Evaluierung über 5 Task-Typen (InformationExtraction, MultiSessionReasoning, KnowledgeUpdate, TemporalReasoning, Abstain).
- LoCoMo Harness: Multi-Session Conversation Evaluierung.

---

## §6 — Governance-Infrastruktur: Soll-Zustand

### §6.1 Fünf Säulen des KI-Entwicklungssystems

1. **Säule 1 — Preflight Gate (`jules-preflight`):** `cargo xtask jules-preflight` als zentraler Aggregator aller lokalen und CI-Gates. Prüft Unwrap-Baseline, Vetoes, DAG-Topologie und Claims.
2. **Säule 2 — Anti-Collision Claim System:** `cargo xtask claim --crate X --issue Y` sperrt Ziel-Crates. Speicherung via GitHub-Issues/Labels für atomare Locks.
3. **Säule 3 — Single Source of Truth:** `DECISIONS.md` als einzige ADR-Quelle (ADR-060). Autogenerierung von `WORKING_STATE.md` via `cargo xtask sync-docs`. `check-agents-integrity` stellt Doku-Frische sicher.
4. **Säule 4 — Gehärtete CI Guardrails:** Workflow `context-gates.yml` sichert Pull Requests ab. `rust-ci.yml` nutzt `cargo nextest --retries 2` für deterministische Testläufe.
5. **Säule 5 — Prompter & Bootstrap Protocol:** `Memfuse-Prompter-v25` injiziert unüberspringbaren Mandatory Bootstrap Präfix in jede Session.

### §6.2 Zwei-Stufen-Entwicklungsprozess (verbindlich)

- **Stufe 1 — Analyse (Claude Orchestrator):** Claude liest den Repository-Zustand, prüft Architektur und ADRs, trifft Entscheidungen und verfasst präzise Task-Spezifikationen. Claude schreibt keinen Produktionscode.
- **Stufe 2 — Implementierung (Jules Agent):** Jules führt zwingend das Mandatory Bootstrap aus, setzt den Crate-Claim, führt `jules-preflight` aus und setzt exakt die spezifizierten Code-Änderungen um.
- **ADR-Governance:** Jules verfasst keine eigenständigen ADRs für Architekturfragen. Bei Bedarf fügt Jules `ADR-VORSCHLAG:` im PR-Body ein und wartet auf menschliche Freigabe.

---

## §7 — Offene Punkte (maschinenlesbar, priorisiert)

| ID | Bereich | Problem | Prio | Blockiert |
|:---|:---|:---|:---:|:---|
| OFFEN-01 | `memfuse-bench` | `bench.yml` läuft gegen Fixtures (`total_cases: 2`), Dataset-Download mit `actions/cache` fehlt | P0 | Echte Baseline |
| OFFEN-02 | `xtask` | `jules_preflight.rs` prüft aktive Claims noch nicht gegen GitHub-API | P0 | P18-Garantie |
| OFFEN-03 | CI / Workflows | `context-gates.yml` fehlen Gate 12 (`MEMFUSE_PR_BODY`) und Gate 14 | P1 | CI-Vollständigkeit |
| OFFEN-04 | Workspaces | `memfuse-py` nicht in Root-`Cargo.toml` `workspace.members` gelistet | P1 | panic=abort CI-Check |
| OFFEN-05 | `memfuse-db` | Cascading-Invalidation: Trigger von Supersedes zu Graph-Kanten-Tombstone unvollständig | P1 | PathRAG-Korrektheit |
| OFFEN-06 | `memfuse-candle` | Native Inferenz/Embedding nicht in `memfuse-db`/`router` Serving-Pipeline verdrahtet | P2 | Sovereign Core Claim |
| OFFEN-07 | `xtask` | `gen_prompter_data.rs` injiziert `WORKING_STATE.md` nicht vollständig in Manifest | P2 | Session-Frische |
| OFFEN-08 | Governance | Zwei-Stufen-Prozess (Claude/Jules) in `AGENTS.md` noch nicht dokumentiert | P2 | Governance-Standard |
| OFFEN-09 | Prompter | Prompter v25 Bootstrap-Präfix und Claim-Step nicht hart im HTML-Baukasten verankert | P2 | P17-Garantie |
| OFFEN-10 | `memfuse-tauri` | Deprecated per ADR-077, physische Entfernung aus Repo steht aus (Frist: 2026-11-07) | P3 | Vision-Clean-Up |
| OFFEN-11 | `memfuse-db` | F-03 EdgeReinforcement Flush-Hook in `crates/memfuse-db/src/maintenance_scheduler.rs` implementiert | P3 | Scheduler-Ausbau |
| OFFEN-12 | `memfuse-embed` | `ImportanceClassifier` zurückgestellt bis Benchmark-Baseline steht | P3 | LongMemEval |

---

## §8 — Umsetzungsreihenfolge (Gesamt-Roadmap)

### Phase 0 — Benchmark-Baseline (parallel active)
- Reparatur von `bench.yml` (OFFEN-01): Einrichtung von `actions/cache` für LongMemEval-S und LoCoMo Datensätze. Erstellung der ersten echten Baseline-Metriken.

### Phase A — Governance-Fundament
- Verdrahtung aller xtask-Module, Ergänzung von `check-agents-integrity`, Reparatur von `context-gates.yml` und `environment_script.sh`.

### Phase B — Claim-Mechanismus
- Vollständige Anbindung von `cargo xtask claim` an GitHub-Issues/Labels. Integration der Claim-Prüfung in `jules-preflight.rs` und Prompter v25.

### Phase C — Crate-Konsolidierung (18 → 9–10 Crates)
- **Schritt C1:** `memfuse-crypto` + `memfuse-kv-bridge` → `memfuse-security`
- **Schritt C2:** `memfuse-store` + `memfuse-checkpoint` → `memfuse-persistence`
- **Schritt C3:** `calibration` + `ollama` + `candle` + `router` + `embed` → `memfuse-inference`
- **Schritt C4:** `reaper` + `sleep_cycle` + `physio_scheduler` → `memfuse-orchestrator`
- **Schritt C5:** `memfuse-index` + `memfuse-graph` + `memfuse-text` → `memfuse-retrieval`

### Phase D — Vision-Bereinigung & Release
- Physische Entfernung von `memfuse-tauri` nach Fristablauf (OFFEN-10).
- Ausbau von `memfuse-py` zum primären Grenzschicht-Crate.
- Erstes offizielles PyPI-Release `memfuse` v0.1.0.

---

## §9 — Definition of Done

Ein Milestone oder Release gilt als "Done", wenn:
1. `cargo test --workspace` besteht fehlerfrei mit 0 Regressions.
2. LongMemEval Recall@10 zeigt keine Regression gegenüber der etablierten Baseline.
3. `cargo xtask check-dag` bestätigt die strikte Einhaltung der 9–10 Crate Ziel-Topologie ohne Layer-Verletzungen.
4. `DeletionProof` erbringt den negativen Rekonstruktionstest (gelöschte Daten auf Storage-Ebene unrufbart).
5. `memfuse-py` lässt sich fehlerfrei via `pip install memfuse` installieren und besteht alle Integrationstests.
6. Sämtliche P0- und P1-Punkte aus §7 (`OFFEN-01` bis `OFFEN-05`) sind vollständig gelöst.
7. Das Frische-Datum von `AGENTS.md` weicht maximal 3 Tage vom letzten Code-Commit ab.

---

## Anhang A — Wettbewerber-Differenzierung

| Merkmal / Komponente | MemFuse v10.0 | MinnsDB | YantrikDB | Graphiti | Mem0 / Zep |
|:---|:---:|:---:|:---:|:---:|:---:|
| **Krypto-WAL & Löschbeweis** | ✅ (HMAC-Chain + DeletionProof) | ❌ | ❌ | ❌ | ❌ |
| **Conformal Router & Drift** | ✅ (UCCI + Lyapunov F-11) | ❌ | ❌ | ❌ | ❌ |
| **Dual-Process & Sleep-Cycle** | ✅ (NREM/REM F-05) | ❌ | ⏳ (think) | ❌ | ❌ |
| **Cascading Invalidation** | ✅ (DocEdgeIndex + Tombstone) | ❌ | ❌ | ❌ | ❌ |
| **DE-Kompositum-Dekomposition**| ✅ (BM25+ Morphologie) | ❌ | ❌ | ❌ | ❌ |
| **Native Pure-Rust Inferenz** | ⏳ (`memfuse-candle`, OFFEN-06) | ❌ | ❌ | ❌ | ❌ |
| **MCP stdio Integration** | ✅ (Zero-Trust Sandbox) | ❌ | ❌ | ❌ | ❌ |

---

## Anhang B — Verbindliche ADR-Referenz

- **ADR-001:** LSM-Tree Storage Engine Architektur
- **ADR-002:** HNSW Vector Index & SIMD Beschleunigung
- **ADR-007:** PyPI als primärer Vertriebskanal
- **ADR-010:** Entkopplung MCP Server — Ausschließliche Nutzung von stdio JSON-RPC 2.0 (kein axum/HTTP)
- **ADR-033:** Bi-Temporale Validity Windows
- **ADR-060:** Governance-Konsolidierung auf `DECISIONS.md`
- **ADR-061:** 2-Phasen Copy-on-Write HNSW Rebuild
- **ADR-063:** ConfigFingerprint-Verpflichtung für Kalibrierungsschutz (P8)
- **ADR-064:** Isolierung von `memfuse-py` im eigenen Cargo-Workspace (`panic = "unwind"`)
- **ADR-065:** CI-Gate `check-duplicate-symbols` zur Durchsetzung von P10
- **ADR-069:** Standard-Terminologie-Normierung (Entfernung biologischer Metaphern)
- **ADR-077:** Produktvision PyPI-Library Fokus und Deprecation von `memfuse-tauri`
- **ADR-078:** Konsolidierung aller Strategy-Dokumente in `GESAMTSPEZIFIKATION_v10.md`

---

## Anhang C — Verworfene Optionen

1. **Desktop-Enterprise-App (`memfuse-tauri`):** Verworfene Vision Option 2. Aufwand für Support, SLAs und Enterprise Sales steht im Widerspruch zum Entwicklungsmodell. Deprecated per ADR-077.
2. **Voice-Assistant / Jarvis Interface:** Verworfene Vision Option 3. Veto aktiv (`VETOES.md`), Review in 6 Monaten.
3. **F-02 Partieller HNSW-Rebuild (Nucleation):** Verworfene Option (VETO-F02). Partielle Rebuilds verletzen Delaunay-Nachbarschaften und führen zu Recall-Kollaps. Ersatz: 2-Phasen CoW-Rebuild (ADR-061).
4. **F-10 Cross-Tenant-Wissensaustausch:** Permanentes VETO (VETO-F10). Bricht TenantId-Isolationsgarantien und macht DeletionProof mathematisch unmöglich.
5. **`CLAIMS.md` Dateisystem-Locking:** Verworfene Option. Datei-Locks bei 25 parallelen Sessions erzeugen Git-Merge-Konflikte. Ersatz: Atomare GitHub-Issues/Labels API.
6. **Verteilte ADR-Dateien (`docs/decisions/*.md`):** Verworfene Option per ADR-060. Führte zu ADR-Nummern-Kollisionen (4× ADR-070). Ersatz: Zentrale `DECISIONS.md`.
