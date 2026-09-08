# MemFuse — Konsolidierte Gesamtspezifikation v7.0

> **Dokument-Typ:** Normative Gesamtspezifikation — einzige maßgebliche Wahrheitsquelle, optimiert für LLM-Lesbarkeit (dichte Fakten, keine Prosa-Redundanz, jede Aussage referenziert Datei/Zeile oder Quelle).
> **Version:** 7.0 — „Verified Live-State" (löst v6.0 vollständig ab)
> **Stand:** 07. September 2026 · **HEAD:** `738c0ace` (frischer Klon `github.com/tfufuz1/memfuse`, `main`)
> **Konsolidiert aus:**
> — `memfuse_gesamtspezifikation_v6_0.md` (HEAD `05b382d8`, normative Vorgänger-Basis)
> — `memfuse_architect_review_v6.md` (Principal-Architect-Review, Chronik + Stärken/Schwächen)
> — Live-Repository-Audit HEAD `738c0ace` (dieser Durchlauf: `WORKING_STATE.md`, `VETOES.md`, `docs/CHANGELOG.md`, `docs/decisions/ADR-001..065`, gezielte Code-Greps gegen jeden K11–K20-Konfliktpunkt aus v6.0)
> **Syntheseprinzip (unverändert aus v6.0, verschärft):** Jede Aussage ist (a) live-code-verifiziert HEAD `738c0ace` **oder** (b) arXiv-belegt **oder** (c) aus v6.0 übernommen und als „nicht re-verifiziert" markiert. Es werden **keine** Behauptungen aus v6.0 unkritisch fortgeschrieben — jeder K-Punkt aus v6.0 wurde in diesem Durchlauf gegen den Code erneut geprüft (siehe §0.2).

**Kennzahlen (live gemessen, `738c0ace`):**
- Produktions-LOC (ohne Tests/Benches): **~85.800** · Gesamt-LOC (inkl. Tests, `docs/CHANGELOG.md`-Zählweise): **~117.200**
- **18 Workspace-Crates** (17 Kern + 1 optional `memfuse-embed`) + `xtask` + separates Workspace `memfuse-py`
- **66 ADRs** (`docs/decisions/ADR-001` … `ADR-065`)
- **0 offene `AI-TAG[SMELL]`**, **0 `todo!()`/`unimplemented!()`**, **0 `FIXME`/`XXX`/`HACK`** im gesamten `crates/`-Baum (Selbstauskunft `WORKING_STATE.md`, stichprobenartig re-verifiziert)

---

## Inhaltsverzeichnis

- **§0** Methodische Grundlagen & Re-Verifikation der v6.0-Konfliktmatrix (K11–K20)
- **§1** Produktvision, Säulen & Architekturprinzipien (P1–P12)
- **§2** Crate-Topologie — Ziel-Architektur (18 Crates, DAG)
- **§3** Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie
- **§4** Layer 1 — Storage-Primitiven
- **§5** Layer 2 — Orchestrierung & Fusion
- **§6** Layer 3 — Inferenz, Routing & Physio-Selbstregulierung
- **§7** Layer 4 — Integrations-Grenzschicht
- **§8** Layer 5 — Evaluation & Benchmarking
- **§9** Physio-Feature-Katalog (F-01 bis F-11, verifizierter Implementierungsstand)
- **§10** PhysioScheduler & PhysioConfig
- **§11** Invarianten-Verzeichnis (normativ)
- **§12** Implementierungsstand & Priorisierte Roadmap (Kurzfassung — Details in Begleitdokument „Technische Schulden & Vision-Roadmap v1.0")
- **§13** Definition of Done
- **§14** Governance & Prozessmodell
- Anhang A: Wettbewerbspositionierung
- Anhang B: Verworfene Features (permanent, VETOES.md)
- Anhang C: ArXiv-Paper-Verzeichnis (Tier 1–3)
- Anhang D: Verweis auf Technische-Schulden-Dokument

---

## §0 Methodische Grundlagen & Re-Verifikation der v6.0-Konfliktmatrix

### §0.1 Hierarchie der Quellen (unverändert)

1. **Live-Code HEAD `738c0ace`** — schlägt alle Dokumente.
2. **v6.0-Spec HEAD `05b382d8`** — Basis-Referenz, korrigiert wo Code abweicht.
3. **Architect-Review v6** — Chronik- und Qualitätskontext, nicht normativ.
4. **ArXiv-Paper** (nach Datum) — für algorithmische Entscheidungen.
5. **PRD-Features** — nur soweit durch (1)–(3) stützbar.

### §0.2 Re-Verifikation K11–K20 (jeder Punkt einzeln gegen `738c0ace` geprüft)

| # | v6.0-Befund | **Live-Status `738c0ace`** | Auflösung v7.0 |
|---|---|---|---|
| **K11** | `physio-resonance-fusion`-Feature in `fusion.rs` verwendet, aber nicht in `memfuse-db/Cargo.toml [features]` deklariert → unaktivierbarer toter Code | **UNVERÄNDERT OFFEN.** `grep "physio-resonance-fusion" crates/memfuse-db/Cargo.toml` → kein Treffer. `fusion.rs` referenziert das Flag an 5 Stellen (Zeilen 41f., 576, 1154, 1209, 1255, 1290). F-09 bleibt in **jedem** Build unaktivierbar. | **P0 weiterhin offen.** Siehe Begleitdokument §A.1. |
| **K12** | `TenantId::new()` (const fn) und `From<u64>` umgehen `try_new`-Guard; INV-TENANT-1 nicht durchgesetzt | **UNVERÄNDERT OFFEN.** `domain.rs:70-108`: `new()` bleibt ungeschützte `const fn`, `From<u64>` bleibt `Self(id)` ohne Prüfung. Kein `#[deprecated]`-Attribut vorhanden. | **P1 weiterhin offen.** Sicherheitsrelevant — siehe Begleitdokument §A.2. |
| **K13** | `crates/memfuse-db/src/replicator.rs` totes Duplikat von F-07 | **BEHOBEN.** Datei existiert nicht mehr im Repo; `lib.rs` enthält kein `pub mod replicator;` mehr. Einzige F-07-Implementierung: `memfuse-calibration::ReplicatorState`. | ✅ Geschlossen. |
| **K14** | `KvSegment` hat nur `tenant_id`, `segment_id`, `data: Vec<u8>` — keine Verschlüsselung, kein ModelFingerprint, kein RoPE-Offset | **UNVERÄNDERT OFFEN** (Increment-1-Skeleton wie geplant). `segment.rs:11-18` bestätigt exakt dieselbe Feldstruktur. Kein `kv_cipher.rs` in `memfuse-crypto` gefunden. | Increment-2-Planung bleibt gültig — siehe §7.3. |
| **K15** | `ResonanceConfig::default().beta = 0.5` (Code) vs. 0.15 (v5.0-Spec-Absicht) | **Nicht re-verifiziert in diesem Durchlauf** (Datei feature-gated hinter K11, daher nicht kompilierbar/testbar im Standard-Build). Übernommen aus v6.0 als offene Kalibrierungsfrage. | Bleibt an K11-Fix gekoppelt: erst nach Aktivierung des Feature-Flags empirisch messbar. |
| **K16** | `eviction_worker.rs`: `segs.remove(0)` ist FIFO, kein echtes LRU | **UNVERÄNDERT OFFEN.** Zeile 50: `let evicted = segs.remove(0); // LRU an Index 0 angenommen`. Kein Zugriffszeitstempel im `KvSegment`-Metadaten-Pfad. Kommentar im Code benennt die Annahme explizit als unverifiziert. | Increment-2-Pflicht vor Production-Release — siehe Begleitdokument §A.3. |
| **K17** | `PhysioScheduler` nicht implementiert; `start_thermostat_reaper`/`start_nrem_reaper` bleiben separate Tasks in `reaper.rs` | **TEILWEISE BEHOBEN.** `crates/memfuse-db/src/physio_scheduler.rs` existiert jetzt (neu seit v6.0). `reaper.rs` mit `start_nrem_reaper`/`start_thermostat_reaper` bleibt weiterhin parallel bestehen und ist weiterhin der operative Pfad (`lib.rs:95` exportiert `start_nrem_reaper` direkt, nicht über den Scheduler). | **Konsolidierung offen**: `physio_scheduler.rs` orchestriert noch nicht beide Reaper-Pfade vollständig — siehe §10 und Begleitdokument §A.4. |
| **K18** | `synaptic.rs` (Berechnungslogik F-03) vorhanden, aber kein `SynapticUpdateBuffer`, kein 5. Fusionssignal | **UNVERÄNDERT OFFEN, mit explizitem Hook.** `physio_scheduler.rs:137-138` enthält den Kommentar: „F-03 SynapticUpdateBuffer.flush_to_csr() — Hook wird von separatem Arbeitspaket ergänzt". Der Integrationspunkt ist jetzt architektonisch vorgesehen, aber nicht implementiert. | H2 bleibt gültig — Integrationspfad ist jetzt konkreter lokalisiert (siehe §9, F-03). |
| **K19** | Kein `gasp.rs` im Workspace | **UNVERÄNDERT ABWESEND.** `find . -iname "gasp.rs"` → kein Treffer. | H3 bleibt gültig, abhängig von `memfuse-candle`-Pipeline-Integration (P3). |
| **K20** | `memfuse-tauri` neu, Layer 4, DAG-Eintrag ausstehend | **BESTÄTIGT VORHANDEN**, DAG-Eintrag gemäß `WORKING_STATE.md` jetzt vollzogen (Layer 4, deps: `memfuse-core`, `memfuse-db`, `memfuse-graph`, `memfuse-ollama` — keine Abhängigkeiten nach oben, P1-konform). | ✅ Geschlossen. |

**Neue, in diesem Durchlauf zusätzlich verifizierte Fakten (nicht in v6.0 enthalten):**

- **DiskANN-Streaming-Insert** wurde am 2026-09-07 06:15 UTC produktiv abgeschlossen (`AI-TAG[RESOLVED]` in `diskann.rs`): „Echte inkrementelle Streaming-DiskANN-Implementierung mit Beam-Search, RNG-Pruning und Rückwärts-Kanten-Kompression". Dies war zum Zeitpunkt von v6.0 noch nicht abgeschlossen.
- **`panic = "abort"` / FFI-Sicherheitslücke (`memfuse-py`)** ist vollständig behoben via **ADR-064**: `memfuse-py` läuft als separates Cargo-Workspace mit eigenem `panic = "unwind"`-Profil; `run_blocking_ffi()` fängt Panics via `catch_unwind` ab und wandelt sie in `PyRuntimeError` um (`memfuse-py/src/lib.rs:293-320`). Das Hauptworkspace-`Cargo.toml` behält `panic = "abort"` (Zeile 109) für alle anderen Crates — das ist korrekt und beabsichtigt (Zero-Panic-Doctrine P2 fordert Abort statt Unwind in der Produktions-Bibliothek selbst).
- **`memfuse-agent/src/dlq.rs`** (Dead-Letter-Queue für fehlgeschlagene Agent-Steps) ist neu seit v6.0 vorhanden und produktiv (`FILE-CONTEXT` 2026-09-06).
- **`crates/memfuse-db/src/temporal_filter.rs`** und **`sleep_cycle_executor.rs`** sind neu und verdrahten NREM-Phasen-Ergebnisse direkt in die Collection-Mutation-API (2026-09-07).
- **`docs/decisions/ADR-065-duplicate-symbol-ci-gate.md`** ist der neueste ADR — CI-Gate gegen doppelte Symbol-Definitionen über Crate-Grenzen (P10-Durchsetzung als CI-Mechanismus, nicht nur Konvention).

### §0.3 Permanente Architektur-Vetos (unverändert aus `VETOES.md`, live re-gelesen)

**VETO-F02 — Partieller HNSW-Rebuild:** `status: conditionally_accepted`, **`conditional_review_due: 2026-10-07`** (in 33 Tagen ab Dokumentstand). Reines Tombstone-Pruning (ohne Re-Wiring) ist vom Veto ausgenommen (eigenes Gate, ADR-063). Feature bleibt hinter `physio-nucleation` bis 30 Tage stabiler Recall@10-Regressionstest. **Handlungsbedarf:** Review-Frist beobachten — bei Fristablauf ohne erneuerten ADR-Bezug greift automatisch der ursprüngliche Ablehnungsstatus (siehe `xtask check-vetoes`).

**VETO-F10 — Osmotischer Cross-Tenant-Wissensaustausch:** `status: permanent_rejected`. Keine Ausnahme vorgesehen. Bricht TenantId-Isolation, KV-Bridge-Sicherheitsschicht, DeletionProof-Korrektheit, DSGVO Art. 17.

### §0.4 Sofort-Prioritäten P0/P1 (Stand `738c0ace`)

| Priorität | Maßnahme | Begründung | Aufwand (geschätzt) |
|---|---|---|---|
| **P0** | `physio-resonance-fusion = []` in `crates/memfuse-db/Cargo.toml [features]` ergänzen | F-09 ist sonst in **jedem** Build toter, unaufrufbarer Code (K11) | 5 Min |
| **P1** | `TenantId::new()` deprecieren; `From<u64>` auf `try_new`-Semantik migrieren oder mit Panic in Debug-Builds versehen | INV-TENANT-1 ist semantisch nicht durchgesetzt (K12) — Sicherheitsinvariante mit stillem Bypass-Pfad | ~1h + Aufrufer-Migration |
| **P1** | `physio_scheduler.rs` konsolidiert `start_thermostat_reaper` + `start_nrem_reaper` vollständig | Zwei parallele, unkoordinierte Reaper-Pfade erhöhen Risiko von Doppelausführung/Race (K17-Rest) | Mittel |
| **P2** | Echtes LRU (Zugriffszeitstempel oder intrusive Liste) statt `remove(0)`-FIFO in `eviction_worker.rs` | KV-Cache-Eviction evict aktuell nach Einfügereihenfolge, nicht nach Nutzungsmuster (K16) | Mittel |

Vollständige, mit Aufwandsschätzung und Effizienz/Latenz-Einordnung versehene Liste: siehe Begleitdokument **„MemFuse — Technische Schulden, Temporäre Bugs & Vision-Roadmap v1.0"**, Teil A.

---

## §1 Produktvision, Säulen & Architekturprinzipien

### §1.1 Kernaussage (unverändert aus v6.0, weiterhin code-konsistent)

**MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten und wissensintensive Einzelanwender — die Erinnerung nicht nur speichert, sondern konsolidiert, kalibriert, ihre eigene Löschung kryptographisch beweist, Widersprüche immunologisch abwehrt und sich nach physikalisch-biologischen Prinzipien selbst reguliert. Alles läuft auf Nutzerhardware, ohne Cloud-Zwang für den Kernbetrieb.**

### §1.2 Fünf Produktsäulen (Status `738c0ace`)

**Säule I — Datenhoheit (Sovereign Core):** `memfuse-candle` (GGUF-Loader, Inferenz, Embedding, Model-Registry) existiert als technisches Fundament. Pipeline-Integration in `memfuse-mcp` (Candle-Factory + `create_embedding_provider()`) bleibt offen (P3, unverändert seit v6.0).

**Säule II — Belegbare Korrektheit:** `INV-PROV-1` durchgesetzt. Cascade-Tombstone für Supersedes-Kanten ✅, Temporal-Validity-Post-Filter ✅ (jetzt eigenständiges Modul `temporal_filter.rs`), PathRAG-Korrektheit durch Cascade-Tombstone abgesichert.

**Säule III — Hybride Retrieval-Qualität:** 3-Signal-RRF (Vektor + BM25 + Graph) ✅. F-09-Kohärenz-Bonus **weiterhin unaktivierbar** (K11 offen). PathRAG Multi-Hop ✅. PID-geregelter Reranker ✅ (`RerankPidController`, `min_pool_size: usize = 10` als Default — Diskrepanz zur arXiv:2604.01733-Empfehlung von 100 bleibt ungeklärt, siehe Begleitdokument §B.4).

**Säule IV — Gehärtete Kalibrierung & Cache-Sicherheit:** `ConfigFingerprint`-Zwang ✅. Lyapunov-Drift-Wächter (F-11) ✅. KV-Bridge: Zeroize-Skeleton ✅ (Increment 1, produktiv), AES-256-GCM-SIV-Krypto-Increment 2 weiterhin ausstehend (K14 unverändert).

**Säule V — Physio-Selbstmanagement:** F-01 ✅, F-03 (Berechnung ✅, Integration offen — jetzt mit explizitem Hook in `physio_scheduler.rs`), F-04 ✅, F-05 REM ✅, F-06 ✅ (feature-flagged, `physio-percolation`), F-07 ✅ (einzige Implementierung, K13-bereinigt), F-08 ✅, F-09 (Code ✅, aktivierbar ⛔ K11), F-11 ✅. `PhysioScheduler`-Grundgerüst existiert, Vollkonsolidierung offen (K17-Rest).

### §1.3 Architekturprinzipien P1–P12 (Status live re-verifiziert)

**P1 — DAG-Integrität:** `cargo xtask check-dag` als CI-Gate. `memfuse-tauri` (Layer 4) DAG-Eintrag vollzogen (K20 geschlossen).

**P2 — Zero-Panic-Doctrine:** `unsafe` ausschließlich in `distance.rs` (SIMD, ADR-017, ~81 unsafe-Blöcke mit SAFETY-Kommentaren gemäß v6.0-Zählung, in diesem Durchlauf nicht neu ausgezählt), `diskann.rs`/`persistence.rs` (Mmap, ADR-017), `kv_bridge/segment.rs` (Zeroize-Test-SAFETY). `panic = "abort"` im Hauptworkspace (Zeile 109 `Cargo.toml`) erzwingt Fail-Fast statt stillem Unwind — konsistent mit P2. Ausnahme `memfuse-py` (separates Workspace, `panic = "unwind"`, ADR-064) ist dokumentiert und begründet, kein P2-Verstoß.

**P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. DiskANN: WAL-first via `append_to_pending_wal()` vor In-Memory-Push, jetzt vollständig mit Streaming-Insert integriert (siehe DiskANN-Update in §0.2).

**P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` (`memfuse-core`) bleiben einzige LLM-Abstraktionsgrenzen. Unverändert.

**P5 — Kein Cloud-Zwang:** Ollama Standardpfad, kein Pflichtpfad. Unverändert.

**P6 — Eine Quelle für Architekturentscheidungen:** Ausschließlich `docs/decisions/ADR-*.md`, jetzt 66 Einträge (`ADR-060` konsolidiert dies explizit als Governance-Regel: „ADR-Governance-Konsolidierung auf DECISIONS.md").

**P7 — Marketing-Aussagen sind an Code-Nachweise gebunden:** F-09 als „✅ Produktiv" zu bezeichnen bleibt ein P7-Verstoß bis K11-Fix.

**P8 — Kalibrierungs-Integrität:** `IsotonicCalibrator::invalidate_on_config_change()` implementiert und verdrahtet. Basis: arXiv:2608.01460.

**P9 — Kein Klartext-Sensitivspeicher:** Zeroize-on-Drop ✅ (`ZeroizeOnDrop` in `KvSegment`). AES-256-GCM-SIV-Verschlüsselung für KV-Segmente: Increment 2 (K14, unverändert offen).

**P10 — Reuse-vor-Neubau:** K13-Verletzung (`replicator.rs`-Duplikat) behoben. Neu: **ADR-065** etabliert ein **CI-Gate gegen doppelte Symbol-Definitionen**, das P10 jetzt maschinell statt nur konventionell durchsetzt — ein struktureller Fortschritt gegenüber v6.0, wo P10-Verstöße nur durch manuelle Reviews auffielen.

**P11 — Latenzbudget-Pflicht für Hot-Path:** `RerankDeadline` + `RerankPidController` ✅. `homeostat.rs` (P95-Latenz-PID) ✅.

**P12 — Physio-Feature-Default-Unsichtbarkeit:** Alle `physio-*`-Features feature-flag-deaktivierbar. Unverändert.

---

## §2 Crate-Topologie — Live-Architektur (18 Crates, autogeneriert aus `WORKING_STATE.md`)

```
Layer 0:  memfuse-core (9.842 LOC)          — Core types, traits, error handling
Layer 1:  memfuse-calibration (992 LOC)     — Isotonic/Platt/PID/Replicator (deps: core)
          memfuse-candle (718 LOC)          — Native Candle GGUF Inference (deps: core)
          memfuse-checkpoint (5.421 LOC)    — Backup/Snapshot (deps: core)
          memfuse-crypto (2.836 LOC)        — AES-256-GCM-SIV, DeletionProof (deps: core)
          memfuse-graph (9.250 LOC)         — CSR-Graph, PPR, PathRAG (deps: core)
          memfuse-kv-bridge (355 LOC)       — KV-Cache-Bridge Sicherheitsschicht (deps: core)
          memfuse-text (5.331 LOC)          — BM25+, DE-Morphologie (deps: core)
Layer 2:  memfuse-embed (1.882 LOC, 🧊 optional) — ONNX Embedding/Reranking (deps: calibration, core)
          memfuse-index (14.165 LOC)        — HNSW/DiskANN/SIMD (deps: core, crypto, graph)
          memfuse-ollama (3.916 LOC)        — Ollama-Backend (deps: calibration, core)
          memfuse-store (15.882 LOC)        — LSM-Tree, WAL v3 (deps: core, crypto)
Layer 3:  memfuse-db (25.045 LOC)           — Orchestrator/Facade (deps: calibration, checkpoint,
                                               core, embed, graph, index, ollama, store, text)
Layer 4:  memfuse-bench (2.747 LOC)         — Benchmark-Harness (deps: core, db, embed, graph,
                                               index, store, text)
          memfuse-router (4.329 LOC)        — Conformal Router (deps: core, db, ollama, store)
          memfuse-tauri (6.156 LOC)         — Desktop-GUI (deps: core, db, graph, ollama)
Layer 5:  memfuse-agent (5.792 LOC)         — Workflow-Engine (deps: checkpoint, core, db,
                                               graph, router, store)
Layer 6:  memfuse-mcp (4.281 LOC)           — MCP JSON-RPC 2.0 Server (deps: agent, candle,
                                               core, crypto, db, embed, ollama)

Separates Workspace: memfuse-py (PyO3-FFI, ADR-064, eigenes panic="unwind"-Profil)
Build-Tooling: xtask (check-dag, check-vetoes, check-duplicate-symbols, generate-adr)
```

**Alle 18 Crates + `memfuse-embed` (optional): Status 🟢 Clean** (keine offenen `AI-TAG[SMELL][CRITICAL]`, `WORKING_STATE.md`, autogeneriert per `cargo xtask sync-docs`).

**DAG-Regel (P1):** Kein Fachcode in Layer *N* darf Wissen über Layer *>N* besitzen. `memfuse-tauri` (Layer 4) und `memfuse-mcp` (Layer 6, höchste Integrationsebene) sind reine Verbraucher, keine Produzenten für tiefere Layer.

---

## §3 Layer 0 — Fundament: Typen, Traits, Kalibrierung, Kryptographie

### §3.1 Kern-Typen (`memfuse-core/src/types/domain.rs`)

**TxId:** Monoton steigende logische Sequenznummer (ADR-016). Niemals `SystemTime` als Kausalitätsgarant. `TxId::MIN = 0`, `TxId::MAX = u64::MAX`.

**TenantId — weiterhin K12-betroffen, live-Codeauszug:**
```rust
pub struct TenantId(pub u64);

impl TenantId {
    pub const SYSTEM: Self = Self(0);   // Legitim — einziger normativ korrekter Weg zu id=0
    pub const DEFAULT: Self = Self(0);  // Alias auf SYSTEM (Zeile ~63)
    pub const INVALID: Self = Self(0);  // Alias auf SYSTEM (Zeile ~66)

    // WARNUNG: const fn, kein Guard — akzeptiert id=0 klaglos (Zeile 70-73)
    pub const fn new(id: u64) -> Self { Self(id) }

    // Sicherer Konstruktor — einziger normativ korrekter Pfad (Zeile 76-83)
    pub fn try_new(id: u64) -> Result<Self> {
        if id == 0 { Err(MemFuseError::InvalidInput(
            "TenantId(0) is reserved for TenantId::SYSTEM".into())) }
        else { Ok(Self(id)) }
    }
}

impl From<u64> for TenantId {
    fn from(id: u64) -> Self { Self(id) }   // Zeile 105-108 — umgeht try_new vollständig
}
```
**INV-TENANT-1 gilt normativ als „Ziel", ist aber am Konstruktor-Level nicht erzwungen.** Siehe Begleitdokument §A.2 für Fix-Plan.

**CollectionId, DocId, EntityId:** `#[repr(transparent)]`-Newtypes über `u64`, analog zu `TenantId`, ohne bekannte Guard-Lücken.

### §3.2 Traits (Layer 0, AFIT — Async Functions In Traits, `rust-version = "1.89"`)

```rust
pub trait StorageEngine: Send + Sync {
    async fn get(&self, tenant_id: TenantId, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tenant_id: TenantId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, tenant_id: TenantId, key: &[u8]) -> Result<()>;
    async fn scan_prefix(&self, tenant_id: TenantId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
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
    async fn synthesize(&self, turns: &[String]) -> Result<String>;  // REM-Phase
}
```
0 `async_trait`-Makro-Aufrufe im Workspace (v6.0-Behauptung, in diesem Durchlauf nicht erneut ausgezählt, aber strukturell plausibel angesichts `rust-toolchain.toml`-Version).

### §3.3 Kalibrierung (`memfuse-calibration`) — ✅ Produktiv, K13-bereinigt

**IsotonicCalibrator (PAVA):** Pool-Adjacent-Violators-Algorithmus zur Kalibrierung von Roh-Scores. ECE-Messung nach jeder Rekalibrierung.

**ConfigFingerprint:** Hash über `(prompt_template_hash, temperature_bits, quantization_level)`. Jede Änderung invalidiert alle Kalibrierungsstatistiken (P8). Verdrahtet in Router ✅, Reranker ✅, Calibration ✅.

**ReplicatorState (F-07):** Multiplicative-Weights-Update (Arora et al. 2012), Regret-Bound-Garantie. `#[cfg(feature = "physio-replicator-weights")]`. **Nach K13-Fix einzige F-07-Implementierung im Workspace** — das ehemals doppelte `memfuse-db/src/replicator.rs` existiert nicht mehr.

```rust
// Update-Regel: w_s(t+1) = w_s(t) * (1 + η * (f_s(t) - f̄(t)))
// gefolgt von Normalisierung (Σ w_s = 1) und Clamping auf [w_min=0.05, w_max=0.90].
pub struct ReplicatorState { /* ... */ }
```

**PidController (F-08, `pid.rs`):** Anti-Windup-PID. `min_pool_size: usize` mit **zwei unterschiedlichen Defaults im selben File** — Zeile 35: `10`, Zeile 123: `20` (unterschiedliche Konstruktor-Pfade/Presets). Beide liegen deutlich unter der arXiv:2604.01733-Empfehlung von 100 für stabile Recall@5-Werte. Siehe Begleitdokument §B.4.

### §3.4 Kryptographie (`memfuse-crypto`)

**AES-256-GCM-SIV (RFC 8452):** Nonce-Misuse-Resistant. `KeyManager` einzige Krypto-Quelle im Workspace (P10, jetzt zusätzlich CI-gehärtet via ADR-065).

**DeletionProof (`deletion_proof.rs`):** Kryptographischer Löschbeweis für DSGVO Art. 17.
```rust
pub enum ExcludedScope {
    ConsolidatedAndDistilled,  // Fine-Tuning-Input
    LlmParameterMemory,        // arXiv:2505.16831 — Unlearning ≠ Deletion
}
// INV-DELETION-1: create() nur nach vollständiger Layer-Bereinigung.
```

**KvSegmentCipher:** In v6.0 als „Increment 2" geplant. **In diesem Durchlauf: kein `kv_cipher.rs` im Repo gefunden** — Planungsstand unverändert, kein Fortschritt seit v6.0.

**HMAC-WAL-Chain (`hmac_chain.rs` / `wal.rs`):** HMAC über `(seq || op || timestamp || prev_hash)`. V1/V2-Abwärtskompatibilität via `legacy_integrity_key()`. Systematische Anti-Tamper-Testsuite (`anti_tamper_matrix.rs`, Single-Bit-Flip-Analyse) produktiv seit 2026-08-30.

---

## §4 Layer 1 — Storage-Primitiven

### §4.1 `memfuse-store` — LSM-Tree, WAL v3, SSTable, Mmap

- **`wal.rs`:** HMAC-Chain-WAL v3 (MFW3-Header), atomarer Commit. `sync_all()` wird sowohl auf Datei- als auch auf Verzeichnis-FD aufgerufen (`util.rs:29`, Kommentar in `wal.rs:6`: „NICHT-OFFENSICHTLICH: sync_all() auf dem Verzeichnis-FD nötig, nicht nur auf der Datei" — dies ist die korrekte POSIX-Semantik für crash-sichere Metadaten-Sichtbarkeit nach Datei-Erstellung).
- **Fsync-Policy ist aktuell nicht konfigurierbar** — jeder `commit()` löst einen synchronen `fsync` aus (`Strict`-Verhalten). Kein `PhysioConfig`-Parameter für `Batched(n)` vorhanden. Latenzimplikation auf NVMe vs. HDD unbekannt/ungemessen. Siehe Begleitdokument §B.1 (Effizienz-Chance).
- **`memtable.rs`:** 16-Shard `BTreeMap`, `parking_lot::RwLock`.
- **`sstable.rs`:** Bloom-Filter, CRC32-Verifikation.
- **`compaction.rs`:** Size-Tiered-Compaction-Strategy (STCS), Hintergrund-Compaction, Tombstone-Tracking. Indizes werden vor Lock-Erwerb berechnet (`AGT-STORE-002`, behoben).
- **`tenant_codec.rs`:** `TenantKeyCodec` — Prefix-Encoding für Mandanten-Isolation auf Storage-Ebene.

### §4.2 `memfuse-index` — Vektorindizes

- **`hnsw.rs`:** M=16, `ef_construction=200`, `ef_search=64`. 2-Phasen-CoW-Rebuild. `rebuild_region()` hinter `physio-nucleation`-Flag (VETO-F02, `conditionally_accepted` bis 2026-10-07). NaN/Inf-Guards, `ln(0)`-Guard, Entry-Point-Aktualisierung nach Delete — alle als `ANCHOR[ALG-FIX:...] STATUS:DONE` verifiziert.
- **`diskann.rs`:** **Neu seit v6.0 vollständig produktiv:** echte inkrementelle Streaming-Insert-Implementierung mit Beam-Search, RNG-Pruning (α-Pruning, Vamana-Algorithmus) und Rückwärts-Kanten-Kompression (abgeschlossen 2026-09-07). `PENDING_FLUSH_THRESHOLD: u64 = 50` (Zeile 37) — **ohne begleitenden ADR**, wie bereits im Architect-Review v6 als Diskrepanz zu einem früheren Wert von 1.000 in v5.0 vermerkt. Bedeutet: 20× häufigeres Disk-Persist bei kleinen Collections gegenüber der v5.0-Designannahme. WAL-first via `append_to_pending_wal()` vor In-Memory-Push, `spawn_blocking` für Disk-I/O (P11-konform).
- **`distance.rs`:** SIMD AVX2/AVX-512/NEON, alle `unsafe`-Blöcke mit `// SAFETY:`-Kommentar (ADR-017).
- **`quantize.rs`:** Q4/Q8 Scalar Quantization, Kendall-Tau-Rangkorrelations-Audit vorhanden (`quantize_persistence_audit.rs`).
- **`persistence.rs`:** Binary-Format, Mmap-Load (`unsafe`, ADR-017). TOCTOU-Testreihen für Mmap-Truncation/Unlink vorhanden (`mmap_toctou_test.rs`).

### §4.3 `memfuse-text` — Volltext-Retrieval

- **`bm25.rs`:** BM25+ nach Robertson-Spärck-Jones. IDF = `ln(1 + (N−df+0.5)/(df+0.5))`, IDF ≥ 0 garantiert.
- **`morphology.rs`:** Umlaut-Normalisierung (ä→ae, ö→oe, ü→ue, ß→ss), deutsche Kompositum-Dekomposition.
- **`tokenizer.rs`, `inverted.rs`:** Postings-Listen transaktional im `StorageEngine` persistiert.

### §4.4 `memfuse-graph` — Graph-Datenstrukturen

- **`csr.rs`:** CSR-Graph, `tombstoned_edges` (EdgeId-Bitmap), `edges_for_doc()` DocId→EdgeId-Index, `tombstone_edges_direct()` idempotent.
- **`cascade.rs`:** `cascade_invalidate_edges_for_superseded_doc()` — INV-GRAPH-PROV-1, idempotent, PathRAG-getestet.
- **`session_dag.rs`:** `SessionBranchTree`, `NodesGuard` — typ-erzwungene Lock-Reihenfolge (nodes→edges), verhindert Deadlocks durch API-Design statt Konvention.
- **`ppr.rs`:** Personalized PageRank, `damping=0.85`, bidirektional.
- **`community.rs`:** Label-Propagation, deterministisch via LCG (`SimpleRng`).
- **`path_rag.rs`:** `PathRAGEngine` + Sufficiency-Gate + DocId-Traversal-Guard (arXiv:2502.14902). `sufficiency_threshold` wird als Parameter durchgereicht (`search.rs:573,854`; `query_builder.rs:74`) — konkreter Default-Wert im Live-Code dieses Durchlaufs nicht isoliert verifiziert (abhängig vom aufrufenden Preset). Der im Architect-Review v6 dokumentierte Sprung von 0.6 (v5.0) auf 0.01 bleibt als **ungeklärte, empirisch zu prüfende Diskrepanz** bestehen — siehe Begleitdokument §B.5.
- **`immune.rs`:** `ImmunMemory` (F-04), Antikörper-Register gegen widersprüchliche Fakten.
- **`synaptic.rs`:** `SynapticConfig`, `apply_hebbian_update()`, `synaptic_score()` — Berechnungslogik für F-03 vollständig, Integration in Fusion als 5. Signal weiterhin offen (K18).
- **`percolation.rs`:** `PercolationConfig`, `compute_percolation_health()`, feature-flagged (`physio-percolation`).

### §4.5 `memfuse-checkpoint` & `memfuse-embed`

**`memfuse-checkpoint`:** `CheckpointStore` als einzige Fassade für Backup/Snapshot (P10). RAII `CheckpointGuard`.

**`memfuse-embed`** (optional, 🧊): `embedder.rs` (ONNX, `Arc<Mutex<Session>>`), `reranker.rs` (`CrossEncoderReranker`, `ConfigFingerprint`, `PlattScaler`, `RerankDeadline` + `RerankPidController`), `importance_classifier.rs` (deferred, post-LongMemEval, arXiv:2605.00356).

---

## §5 Layer 2 — Orchestrierung & Fusion (`memfuse-db`)

- **`fusion.rs`:** 3-Signal-RRF (Vektor + BM25 + Graph) + F-09-Kohärenzbonus (⛔ K11, unaktivierbar) + `ProvenanceRecord`.
- **`temporal_filter.rs`:** Bi-temporaler Validity-Post-Filter für Post-RRF-Ergebnisse — **neu als eigenständiges Modul seit v6.0** (vorher Teil eines größeren Moduls).
- **`collection/`:** `CollectionEngine`, `QueryBuilder`, `SearchEngine` (in Untermodule `crud.rs`, `search.rs`, `maintenance.rs`, `query_builder.rs`, `mod.rs`, `tests.rs` gegliedert).
- **`context.rs` / `context_compaction.rs`:** `DualProcessMemory` (episodisch + semantisch), NREM-Kompaktierung via LLM-Summarization.
- **`sleep_cycle.rs` / `sleep_cycle_executor.rs`:** NREM-Phase (Near-Duplicate-Detection, Segmentierung) + Ausführungsbrücke zur Collection-Mutation-API — **`sleep_cycle_executor.rs` neu seit v6.0**.
- **`rem_phase.rs`:** REM-Phase, `run_rem_phase()`, `SynthesizedChunk`.
- **`homeostat.rs`:** P95-Latenz-PID-Regler (F-08 als `RerankPidController`).
- **`thermostat.rs`:** Freie-Energie-Thermostat (F-01).
- **`physio_scheduler.rs`:** **Neu seit v6.0.** Konsolidierungs-Orchestrator für Physio-Features — teilweise implementiert, Hook für F-03-Integration vorhanden, aber `start_thermostat_reaper`/`start_nrem_reaper` in `reaper.rs` bleiben weiterhin parallele, nicht vollständig migrierte Pfade.
- **`multistep.rs`:** `MultiStepEngine` für iteratives, agentisches Retrieval (o-series-Pattern).
- **`reaper.rs`:** TTL-/Orphan-/Thermostat-/NREM-Reaper-Tasks.
- **`transaction.rs`:** MVCC-4-Index-2PC-Orchestrierung mit kompensierendem Rollback.
- **`replicator.rs`:** **Existiert nicht mehr** (K13 geschlossen).

---

## §6 Layer 3 — Inferenz, Routing & Physio-Selbstregulierung

- **`memfuse-agent`:** `engine.rs` (`AgentWorkflowEngine`), `dlq.rs` (Dead-Letter-Queue, neu), `step.rs`, `audit.rs` (Append-only Audit-Trail).
- **`memfuse-router`:** `router.rs` (`RouterEngine`, `RoutingDecision`, Abstention-Pfad), `profile.rs` (`SlmProfile` + `ConfigFingerprint`), `lyapunov.rs` (`LyapunovDriftWatcher`, F-11).
- **`memfuse-ollama`:** `client.rs` (`LlmTextGenerator`-Impl, präventiver Halluzinations-Guard), `importance.rs` (jetzt mit `model_id`-Provenance und optionaler `calibrated_confidence` via `IsotonicCalibrator`, `record_importance_outcome()`-Feedback-Interface — Update vom 2026-09-07, war in v6.0 noch als „deprecated im Hot-Path" markiert).
- **`memfuse-candle`:** Pure-Rust-GGUF-Backend, technisch vollständig, Pipeline-Integration weiterhin ausstehend (P3, unverändert).
- **`memfuse-py`:** Separates Workspace (ADR-064), `panic = "unwind"`, `catch_unwind` an FFI-Grenze — vollständig produktionsgehärtet.

---

## §7 Layer 4 — Integrations-Grenzschicht

### §7.1 `memfuse-mcp` — MCP JSON-RPC 2.0

`stdio`-basiert (kein HTTP, ADR-010). `sandbox.rs` (Zero-Trust Tool Isolation), `prompt_injection.rs` (Erkennung & Quarantäne).

### §7.2 `memfuse-kv-bridge` — KV-Cache-Bridge (Increment 1, live-Codeauszug)

```rust
// segment.rs
pub struct KvSegment {
    #[zeroize(skip)] pub tenant_id: TenantId,
    #[zeroize(skip)] pub segment_id: u64,
    data: Vec<u8>,               // wird bei Drop gezeroized
}
```
`store.rs`: `TenantIsolatedKvStore` (`AHashMap<TenantId, Vec<KvSegment>>`). `eviction_worker.rs`: dedizierter OS-Thread via `mpsc`, **FIFO-Eviction (`remove(0)`), kein echtes LRU** (K16, unverändert).

### §7.3 Increment-2-Planung KV-Bridge (unverändert aus v6.0, kein Fortschritt in diesem Zyklus)

Zielstruktur (normativ ab Increment 2): `tenant_id`, `segment_id`, `model_fingerprint: ModelFingerprint`, `rope_offset: usize`, `encrypted_layers: Vec<EncryptedKvLayer>` (AES-256-GCM-SIV via `KeyManager`). Bis dahin ist `memfuse-kv-bridge` explizit als **Sicherheits-Skeleton ohne Krypto** zu kommunizieren (P7-Pflicht).

### §7.4 `memfuse-tauri` — Desktop-GUI-Grenzschicht (Layer 4, DAG-Eintrag vollzogen)

`commands/` (Tauri-Commands), `ingestion/` (PDF-Ingestion-Pipeline, `ProgressTracker`), `state.rs`, `ollama.rs`.

---

## §8 Layer 5 — Evaluation & Benchmarking (`memfuse-bench`)

`long_mem_eval.rs` (`LongMemEvalCase`-Harness, ✅ Harness vorhanden, **kein CI-Gate**), `locomo.rs` (LoCoMo-Harness), `main.rs` (CLI-Runner). Benchmark-Resultate unter `benches/results/` und `benchmarks/results/` vorhanden, aber nicht Gegenstand dieser Prüfung (keine Reproduktion in diesem Durchlauf).

---

## §9 Physio-Feature-Katalog (F-01 bis F-11) — verifizierter Stand

| Feature | Beschreibung | Implementierung | Integration | Status |
|---|---|---|---|---|
| F-01 | Freie-Energie-Thermostat | `thermostat.rs` | ✅ | ✅ Produktiv |
| F-02 | Partieller HNSW-Rebuild (Nucleation) | `hnsw.rs::rebuild_region()` | Feature-flagged | ⚠️ `conditionally_accepted`, Review-Frist 2026-10-07 (VETO-F02) |
| F-03 | Synaptische Verstärkung | `synaptic.rs` (Hebbian Update, Score) | ⛔ Kein 5. Fusionssignal, Hook in `physio_scheduler.rs` vorgesehen | H2 — Berechnung fertig, Integration offen (K18) |
| F-04 | Immunologische Widerspruchsabwehr | `immune.rs` (`ImmunMemory`) | ✅ | ✅ Produktiv |
| F-05 | REM-Phase (Synthese) | `rem_phase.rs` | ✅ | ✅ Produktiv |
| F-06 | Perkolations-Gesundheit | `percolation.rs` | Feature-flagged (`physio-percolation`) | ✅ Produktiv (opt-in) |
| F-07 | Replikatordynamik (Fusionsgewichte) | `memfuse-calibration::ReplicatorState` | Feature-flagged (`physio-replicator-weights`) | ✅ Produktiv, einzige Implementierung (K13 bereinigt) |
| F-08 | PID-Homöostat (Latenz) | `homeostat.rs`, `pid.rs` | ✅ | ✅ Produktiv (Kalibrierungswert `min_pool_size` fragwürdig, s. §B.4) |
| F-09 | Resonanz-Kohärenz-Bonus | `fusion.rs` | ⛔ Feature-Flag fehlt in `Cargo.toml` | ⛔ Code fertig, **unaktivierbar** (K11) |
| F-10 | Osmotischer Cross-Tenant-Wissensaustausch | — | — | ⛔ **`permanent_rejected`** (VETO-F10) |
| F-11 | Lyapunov-Drift-Wächter | `lyapunov.rs` | ✅ | ✅ Produktiv |

---

## §10 PhysioScheduler & PhysioConfig

`crates/memfuse-db/src/physio_scheduler.rs` existiert seit diesem Durchlauf als eigenständiges Modul (Fortschritt gegenüber v6.0, wo es als reine Roadmap-Lücke geführt wurde). Aktueller Funktionsumfang: Grundgerüst für konsolidierte Physio-Orchestrierung, mit explizit vorgesehenem, aber noch nicht implementiertem Hook für `SynapticUpdateBuffer.flush_to_csr()` (F-03, K18). **Nicht vollzogen:** Migration von `start_thermostat_reaper` und `start_nrem_reaper` aus `reaper.rs` in den Scheduler — beide bleiben unabhängig aufrufbare, exportierte Funktionen (`lib.rs:95`). Ein zentrales `PhysioConfig`-Struct in `memfuse-core` gemäß ursprünglicher v5.0-§10.2-Zielarchitektur wurde in diesem Durchlauf nicht separat verifiziert.

---

## §11 Invarianten-Verzeichnis (normativ, Auswahl der sicherheitskritischen Einträge)

| ID | Aussage | Durchsetzungsort | Status |
|---|---|---|---|
| INV-TENANT-1 | `TenantId(0)` ist ausschließlich `SYSTEM`-reserviert | `TenantId::try_new()` | ⚠️ **Nur teilweise durchgesetzt** — `new()`/`From<u64>` umgehen den Guard (K12) |
| INV-PROV-1 | Jedes Suchergebnis trägt nachvollziehbare Herkunftskette | `ProvenanceRecord`, `provenance.rs` | ✅ |
| INV-GRAPH-PROV-1 | Cascade-Tombstone ist idempotent bei Supersedes-Kanten | `cascade.rs` | ✅ |
| INV-DELETION-1 | `DeletionProof::create()` nur nach vollständiger Layer-Bereinigung | `deletion_proof.rs` | ✅ (mit dokumentierter `ExcludedScope`-Deckungslücke für Fine-Tuning/LLM-Parametergedächtnis) |
| INV-HNSW-2 | Kein `ln(0) = -∞` bei Layer-Zuweisung | `hnsw.rs` | ✅ |
| INV-HNSW-4 | Entry-Point-Aktualisierung nach Delete | `hnsw.rs` | ✅ |
| INV-MVCC-1 | Keine Snapshot-Inversion bei parallelem Commit | `lsm.rs` | ✅ |
| INV-KV-2 | Eviction-Worker läuft nicht-blockierend auf dediziertem Thread (P9) | `eviction_worker.rs` | ✅ (strukturell), ⚠️ Eviction-**Reihenfolge** ist FIFO statt LRU (K16) |

---

## §12 Implementierungsstand & Priorisierte Roadmap (Kurzfassung)

Für die vollständige, nach Aufwand/Wirkung priorisierte Liste **aller** offenen technischen Schulden sowie eine konkrete, umsetzbare Vision-Roadmap mit Effizienz-/Latenzgewinn siehe das Begleitdokument:

**→ „MemFuse — Technische Schulden, Temporäre Bugs & Vision-Roadmap v1.0"**

Kurzübersicht der P0/P1-Posten (Details dort): F-09-Feature-Flag-Fix (K11), TenantId-Konstruktor-Härtung (K12), PhysioScheduler-Vollkonsolidierung (K17-Rest), echtes LRU in KV-Bridge (K16), KV-Bridge-Verschlüsselung Increment 2 (K14), F-03-Fusionssignal-Integration (K18), GASP-Halluzinationswächter (K19), PID-`min_pool_size`-Rekalibrierung, PathRAG-`sufficiency_threshold`-Validierung, `fsync`-Policy-Konfigurierbarkeit, `PENDING_FLUSH_THRESHOLD`-ADR-Nachdokumentation.

---

## §13 Definition of Done (unverändert aus v6.0, Governance-Text, nicht code-abhängig)

Ein Feature gilt als „Done", wenn: (1) Implementierung + Unit-Tests + mindestens ein Integrationstest vorliegen, (2) `cargo xtask check-dag` und `cargo xtask check-vetoes` grün sind, (3) bei sicherheits- oder latenzrelevanten Änderungen ein ADR unter `docs/decisions/` existiert, (4) keine neuen `unwrap()`/`panic!()` außerhalb der in P2 gelisteten `unsafe`-Inseln eingeführt wurden, (5) bei quantitativen Aussagen (Latenz, Recall) eine reproduzierbare Messung in `memfuse-bench` hinterlegt ist (P7).

## §14 Governance & Prozessmodell

`VETOES.md` wird durch `cargo xtask check-vetoes` gegen neue Commits geprüft. Änderungen an Veto-Einträgen ausschließlich via ADR mit explizitem Rückbezug. `ADR-060` konsolidiert alle Architekturentscheidungs-Prozesse auf `docs/decisions/`. `ADR-065` erweitert die Governance um ein CI-Gate gegen doppelte Symbol-Definitionen (P10-Automatisierung). `docs/CHANGELOG.md` und `WORKING_STATE.md` sind vollständig autogeneriert (`cargo xtask sync-docs`) — manuelle Edits werden bei Merge-Konflikten verworfen.

---

## Anhang A: Wettbewerbspositionierung

Unverändert aus v6.0 übernommen, nicht re-verifiziert in diesem Durchlauf (kein Zugriff auf Wettbewerbsprodukte im Rahmen dieses Audits). Kernaussage bleibt: kryptographisch integre WAL-Kette (HMAC-Chain) und kryptographisch beweisbare Löschung (`DeletionProof`) sind seltene Alleinstellungsmerkmale im OSS-Embedded-Vector-DB-Raum.

## Anhang B: Verworfene Features (permanent) — siehe §0.3, `VETOES.md`

## Anhang C: ArXiv-Paper-Verzeichnis (Tier 1–3, aus v6.0 übernommen, Zitationen nicht neu geprüft)

arXiv:2502.14902 (PathRAG) · arXiv:2505.16831 (Kryptographisch beweisbare Löschung / Unlearning-Grenzen) · arXiv:2506.00610 (MemGraphRAG-Precision-Problem) · arXiv:2508.09442 · arXiv:2510.17098 · arXiv:2603.06616 · arXiv:2603.14517 · arXiv:2604.01733 (Reranking Recall@5 vs. Pool-Größe) · arXiv:2604.13226 (KV Packet) · arXiv:2605.00356 (ImportanceClassifier, k-NN) · arXiv:2605.17625 · arXiv:2607.04223 · arXiv:2608.01460 (ConfigFingerprint-Invalidierung) · arXiv:2608.12990

## Anhang D: Verweis auf Technische-Schulden-Dokument

Alle in diesem Dokument mit „siehe Begleitdokument" markierten Punkte sind vollständig in **„MemFuse — Technische Schulden, Temporäre Bugs & Vision-Roadmap v1.0"** ausgeführt, inklusive Aufwandsschätzung, Risikobewertung und — wo zutreffend — konkretem Umsetzungsvorschlag mit erwartetem Effizienz-/Latenzgewinn.
