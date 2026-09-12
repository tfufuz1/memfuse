# MemFuse — Projekt-Spezifikation (v2, erweitert)

**Verifiziert gegen:** frischer `git clone https://github.com/tfufuz1/memfuse`, HEAD `33226e2` (09.09.2026, im Rahmen dieser Session).
**Vorgänger-Stand:** v1 dieses Dokuments war gegen `84dc93a` verifiziert; alle Aussagen wurden für v2 gegen den aktuellen Code neu geprüft, nicht unbesehen übernommen.
**Verhältnis zu bestehenden Dokumenten:** Dieses Dokument fasst den Repository-Zustand zusammen und ergänzt ihn um eine mikrofeingranulare Schnittstellenspezifikation je Crate (Abschnitt 6), die als primäre Referenz für LLM-Agenten (Google Jules, Claude, etc.) dient, die an einzelnen Komponenten arbeiten. Es ersetzt keine der projektinternen Quellen — bei Widerspruch gilt die dort dokumentierte Hierarchie: **Code-Befund (`AGENTS.md` §1) > `GESAMTSPEZIFIKATION_v10.md` > alles andere**, inklusive dieses Dokuments.
**Ergänzendes Dokument:** `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` beschreibt *wie* dieser Code entsteht (Prompter, Google-Jules-Schwarm, CI-Gates, isolierte Fix-Prompts). Beide Dokumente sind komplementär und sollten gemeinsam gelesen werden.

---

## 0. Änderungen gegenüber v1 (Änderungsprotokoll)

| # | Änderung | Beleg |
|---|---|---|
| 1 | HEAD-Referenz von `84dc93a` auf `33226e2` aktualisiert (≈ 40 Commits neuer). | `git log` |
| 2 | Neuer Abschnitt 6: mikrofeingranulare Schnittstellenspezifikation für alle 17 Workspace-/FFI-Crates (Modul-Karten, Kern-Traits mit exakten Signaturen, kritische Invarianten, Cross-Crate-Kanten). | `crates/*/AGENTS.md`, `crates/memfuse-core/src/traits/mod.rs` |
| 3 | Neuer Abschnitt 7: Statusmatrix der zehn seit v1 laufenden Härtungs-Fix-Prompts (N1–N5, B, E1/E2, D1/D2, F/G/H), verifiziert gegen den tatsächlichen HEAD-Code, nicht gegen die Prompt-Absicht. | `MemFuse_Jules_Fix_Prompts_*.md`, Live-Grep gegen HEAD |
| 4 | Dokumentierte Namens-/Pfad-Drift: `AGENTS.md` (root, Stand `b448084`) referenziert weiterhin `crates/memfuse-kv-bridge/` als eigenständiges Crate; im tatsächlichen Code ist dieser Bestandteil seit mehreren Sessions in `crates/memfuse-crypto/src/kv_segment/` konsolidiert. `WORKING_STATE.md` (autogeneriert!) listet das Crate zudem unter dem Namen `memfuse-security` statt `memfuse-crypto`. Beide Docs sind zum jeweiligen Verifikationszeitpunkt intern inkonsistent — ein konkretes, im Live-Repo nachvollziehbares Beispiel für die in `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` §3.2 beschriebene Doku-Drift-Gate-Lücke (Gate 5 prüft Sync gegen `WORKING_STATE.md`/`ARCHITECTURE.md`/`SOURCE_OF_TRUTH.md`, aber nicht gegen `AGENTS.md` selbst). | `AGENTS.md` §2 vs. `find crates/`, `WORKING_STATE.md` Crate-Inventar-Tabelle |
| 5 | Neues Gate 15 (`check-doc-references`) in `context-gates.yml` seit v1 hinzugekommen — siehe `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` §3.2. | `.github/workflows/context-gates.yml` |
| 6 | ADR-Reihe erweitert bis `ADR-078` (v1 kannte nur bis `ADR-077`). | `DECISIONS.md` |
| 7 | Neuer CI-Workflow `nucleation-recall-history.yml` (täglich, 30-Tage-Stabilitätsmessung für `F-02`-Tombstone-Pruning, siehe `VETOES.md#VETO-F02`) sowie neues xtask-Subkommando `check-recall-stability` — beides in v1 noch nicht existent. | `.github/workflows/`, `xtask/src/check_recall_stability.rs` |

---

## 1. Was MemFuse ist

MemFuse ist eine **eingebettete, kryptographisch gehärtete AI-Memory-Infrastruktur in Rust** — eine lokale, air-gapped-fähige Speicher- und Retrieval-Engine für LLM-Agenten, die Langzeitgedächtnis, Graph-basiertes Retrieval und Mandanten-Isolation kombiniert. Kein Cloud-Dienst, keine Server-Farm-Architektur: Das System ist als **Library** konzipiert, die in andere Anwendungen eingebettet wird.

### 1.1 Produktvision (ADR-077, verbindlich seit 08.09.2026)

Nach einer Phase, in der drei Produktvisionen parallel verfolgt wurden (PyPI-Library / Desktop-Enterprise-App „MemFuse Brain" via Tauri / Voice-Assistant „Jarvis"), ist die Vision seit ADR-077 **formal entschieden**:

- **Primär: PyPI-Library** (`memfuse-py`) — Positionierung als „schlanker als MinnsDB, mit kryptographischer Härtung als Alleinstellungsmerkmal". Zielgruppe: Entwickler, die ein eingebettetes, auditierbares Memory-Substrat für eigene Agenten-Anwendungen brauchen, ohne Cloud-Abhängigkeit.
- **Deprecated, in Entfernung:** Die Tauri-Desktop-App (`memfuse-tauri`, „MemFuse Brain") — ehemals Positionierung als air-gapped Unternehmensassistent für DACH-Marktsegment. ADR-018 (Doppelstrategie) ist durch ADR-077 abgelöst. **Zielfrist der physischen Entfernung: ≈ 07.11.2026** (in v1 als Lücke benannt; zum Zeitpunkt von v2 weiterhin nicht durch ein CI-Gate überwacht, `crates/memfuse-tauri` existiert unverändert, `.github/workflows/tauri-release.yml` läuft weiterhin).
- **Formal zurückgestellt (Veto `VETO-OP3`, Wiedervorlage 2027-03-08):** Voice/Jarvis-Assistent — bindet Audio-Streaming-/WebSocket-Komplexität ohne Beitrag zur Kernstärke.

**Begründung der Fokussierung:** Das Entwicklungsmodell (Solo-Architekt + KI-Agenten-Schwarm, kein Vertriebsteam) passt strukturell nicht zu einem Enterprise-Sales-Motion mit Support-SLAs. Die stärksten differenzierenden Code-Bestandteile — kryptographischer Löschbeweis, HMAC-WAL-Kette, Mandanten-Isolation — sind als Library-Feature genauso vermarktbar wie innerhalb einer Desktop-App, ohne das UI/Packaging-Investment zu binden.

### 1.2 Kern-Differenzierungsmerkmal: Compliance-/Krypto-Härtung

Der wiederkehrende rote Faden über die gesamte Architektur ist **beweisbare, kryptographisch abgesicherte Datenintegrität und -löschung**:

- **`DeletionProof`** (`crates/memfuse-crypto/src/deletion_proof.rs`, 341 LOC): kryptographischer Löschnachweis für DSGVO Art. 17 („Recht auf Vergessenwerden") — nicht nur ein Lösch-Flag, sondern ein verifizierbarer, HMAC-SHA256-signierter Beweis, dass Daten unwiederbringlich entfernt wurden. **Seit v1 in aktiver Härtung:** Prompt D1 (siehe Abschnitt 7) adressiert, dass die Vorbedingung „physische Bereinigung MUSS vor Proof-Erstellung erfolgen" (Invariante `INV-DELETION-1`) bislang nur durch Kommentar-Konvention, nicht durch das Typsystem gesichert ist — Status: **noch nicht gemerged** zum Zeitpunkt von HEAD `33226e2`.
- **WAL-HMAC-Kette:** Jeder Write-Ahead-Log-Eintrag ist kryptographisch an seinen Vorgänger gebunden (Hash-Chaining), sodass nachträgliche Manipulation oder stille Truncation erkennbar wird. Seit v1 zusätzlich gehärtet: TOCTOU-Fenster zwischen physischer Truncation und In-Memory-`size`/`last_hmac`-Update geschlossen (Prompt N2, **gemerged**, Commit `59ce141`), Rollback-Pfad unter `commit_mutex` durch dedizierten Concurrency-Regressionstest abgesichert (Prompt N4, **gemerged**, Commit `33226e2`).
- **Mandanten-Isolation (`TenantId`, `TenantIsolatedKvStore`):** Strikte kryptographische Trennung zwischen Mandanten-Kontexten, inklusive eines formalen, permanenten Vetos (`VETO-F10`) gegen jede Form von „Cross-Tenant Knowledge Sharing". Seit v1 zusätzlich in Härtung: tenant-faire LRU-Eviction (`evict_lru_fair()`, Prompt-Vorläufer bereits **gemerged**, Commit `b5a68b9`), Nachfolge-Härtung (Sichtbarkeitsreduktion von `evict_lru_global()`, Rotations-Bias-Fix, Lock-Batching — Prompts E1/E2) **noch nicht gemerged**.

---

## 2. Architektur: Crate-Topologie (Layer 0–6, DAG-Modell)

MemFuse ist strikt als gerichteter azyklischer Graph (DAG) von Cargo-Workspace-Crates organisiert. Jeder Layer darf nur auf Layer mit niedrigerer Nummer verweisen — Zyklen und „Layer-Überspringen" werden durch das Gate `cargo xtask check-dag` in CI hart erzwungen.

> **Hinweis zur Layer-Zählung:** Das Root-`AGENTS.md` verwendet ein feingranulareres Modell (Layer 0–6, 7 Stufen) als die ursprüngliche v1-Fassung dieses Dokuments (Layer 0–4). Beide Modelle sind bezüglich der DAG-Kanten widerspruchsfrei — der Unterschied liegt allein in der Granularität der Zwischenstufen-Benennung (v1 fasste „Orchestrierung/DB" und „LLM/FFI-Anbindung" zusammen). v2 übernimmt das feinere `AGENTS.md`-Modell als Referenz, da es der aktuellen Code-Quelle näher ist.

| Layer | Zweck | Crates | LOC (`src/`, gemessen) |
|---|---|---|---|
| **0 — Fundament** | Kern-Typen, Traits, Fehlerbehandlung, Kalibrierungs-Grundbausteine | `memfuse-core` (`TenantId`, `TxId`, `DocId`, `EntityId`, `ConfigFingerprint`, `MemFuseError`, Kern-Traits), `memfuse-calibration` (Platt/Isotonic-Scaler) | core: 9.625 · calibration: 1.282 |
| **1 — Storage-Primitiven & Vertikalen** | Verschlüsselung, KV-Sicherheitsschicht, Checkpointing, Graph-Kern, Volltext, native Inferenz | `memfuse-crypto` (Encryption-at-Rest, `DeletionProof`, **inkl. konsolidiertem KV-Segment-Store, ehem. `memfuse-kv-bridge`**), `memfuse-checkpoint`, `memfuse-graph` (CSR-Graph, `ConsistencyEnforcer`, `PathRAGEngine`, `EdgeProvenance`), `memfuse-text` (BM25, DACH-Kompositum-Splitting), `memfuse-candle` (natives GGUF-Inferenz-Backend) | crypto: 2.887 · checkpoint: 2.666 · graph: 9.189 · text: 3.994 · candle: 1.216 |
| **2 — Subsysteme** | Vektorsuche, Embedding-Provider, Storage-Engine | `memfuse-embed` (`CrossEncoderReranker`, optional, Feature `onnx`), `memfuse-index` (HNSW, SQ8-Quantisierung, DiskANN hinter `experimental-diskann`), `memfuse-ollama` (`ContextPrefixEngine`), `memfuse-store` (LSM-Tree, WAL) | embed: 1.674 · index: 10.666 · ollama: 3.925 · store: 11.570 |
| **3 — Hauptdatenbank** | Kollektions-CRUD, 4-Signal-Fusion, Konsolidierung | `memfuse-db` (`ConsolidationSession`, `AdaptiveDecayController`/F-01, `ConsolidationEngine`, `MarkdownChunker`, `MultiStepEngine` mit RRF-Fusion) | db: 19.686 |
| **4 — Orchestrierung & Frontend** | Benchmark-Harness, Routing, Desktop-Shell | `memfuse-bench` (nicht Teil dieser Spezifikation im Detail), `memfuse-router` (Conformal-Routing, SLM-Profile), `memfuse-tauri` (Deprecated, ADR-077) | router: 4.838 · tauri: 4.340 |
| **5 — Agenten-Engine** | Persistente Agenten-Workflows | `memfuse-agent` (`PersistentAgentWorkflow`, State-Graph + Checkpointing) | agent: 2.757 |
| **6 — Protocol & Sandbox** | Sichere Bedienoberflächen für externe Agenten | `memfuse-mcp` (`McpSandbox`, Read-Only + Write-Authorization-Guard) | mcp: 3.175 |
| **Außerhalb des Root-Workspace (ADR-064)** | Python-FFI-Bindung | `memfuse-py` (`panic = "unwind"`, separater Build) | py: 1.617 |

**Wichtige strukturelle Sonderregel:** `memfuse-py` ist **bewusst nicht** im Root-`Cargo.toml`-Workspace `members`-Array eingebunden (ADR-064, verifiziert: 18 Members im Workspace-Array, `memfuse-py` fehlt). Grund: `memfuse-py` benötigt `panic = "unwind"` im Release-Profil, damit `catch_unwind()` an der FFI-Grenze zu Python funktioniert — der restliche Workspace nutzt `panic = "abort"` (kleinere Binaries, kein Unwind-Overhead). Diese Trennung ist dokumentierte Absicht, keine technische Schuld.

---

## 3. Kern-Subsysteme im Detail

### 3.1 Storage Engine & Transaktionalität
- **LSM-Tree** als Persistenzgrundlage (ADR-001) — hoher Schreibdurchsatz, Crash-Konsistenz über sequenzielle WAL-Writes und immutable SSTables, saubere Snapshot-Isolation. Datenpfad: `Client → TxBuffer → WAL → MemTable → SSTable → Compaction`.
- **2-Phase-Commit über 4 Indizes** (`memfuse-db`), WAL-Format V3 mit `tx_id`-HMAC-Binding.
- **Bi-temporale Graph-Gültigkeitsachsen** — Unterscheidung zwischen logischer Transaktionszeit (`TxId`) und physikalischer Gültigkeitszeit von Fakten (`valid_from`/`valid_to`, ADR-033).
- **Write-Temp-Then-Rename** für SSTables und DiskANN-Segmente — atomare Sichtbarkeit neuer Segmente.
- **`commit_mutex → state`-Lock-Hierarchie** (`memfuse-store::lsm::Lsm::commit()`): `commit_mutex` wird über die gesamte Funktionsdauer inklusive Fehler-/Rollback-Pfad gehalten; die Invariante ist Stand HEAD `33226e2` durch einen dedizierten Concurrency-Regressionstest verifiziert (Prompt N4), aber — im Gegensatz zum in Prompt N5 vorgeschlagenen Refactor — **weiterhin nur durch Konvention, nicht durch das Typsystem** gesichert (`rollback_to_tx_locked()` verlangt noch keinen Compiler-geprüften `CommitGuard`-Beweis; Prompt N5 hat niedrigste Priorität und ist optional).

### 3.2 Retrieval: Vektor, Volltext, Graph, Fusion
- **HNSW** und **DiskANN** (Out-of-Core-fähig, hinter `experimental-diskann`) als Vektorindex-Backends.
- **BM25** (`memfuse-text`) mit DACH-spezifischer Kompositum-Zerlegung.
- **CSR-Graph mit Personalized PageRank** und **PathRAGEngine** — inklusive `ConsistencyEnforcer` (F-04) und `EdgeProvenance` (Invariante `INV-GRAPH-PROV-1`).
- **`weighted_reciprocal_rank_fusion_with_options()`** (`memfuse-db/src/fusion.rs`, 1.467 LOC) — mehrstufige RRF-Signal-Fusion. **Seit v1 gehärtet:** `!weight.is_finite() || weight <= 0.0`-Guard gegen NaN/Inf-Propagation im Akkumulations-Loop, plus Frühwarn-Logging bei nicht-endlichem `doc.score` aus Upstream-Signalen (Prompt N1, **gemerged**, Commit `1fedb8d`). **Weiterhin offen:** `HeapEntry::cmp()` nutzt noch `partial_cmp().unwrap_or(Ordering::Equal)` statt des robusteren `f32::total_cmp` (analog zu `diskann.rs` bereits etabliert), und die `INV-PROV-1`-Konsistenzprüfung ist noch in einem `#[cfg(debug_assertions)]`-Block gekapselt, also in Release-Builds unbeobachtbar (Prompt H, **noch nicht gemerged**).
- **`CrossEncoderReranker`** — Reranking-Stufe nach initialer Kandidatengenerierung.

### 3.3 Memory-Konsolidierung („Sleep-Cycle-Pattern")
- **`ConsolidationEngine`** / `execute_sleep_cycle()` — Hintergrund-Konsolidierung und Community-Synthese.
- **`AdaptiveDecayController`** (F-01, „Free Energy Thermostat") — hinter Feature-Flags `adaptive-decay-control`, `adaptive-decay` (ADR-069).
- **Kaskadierende CSR-Invalidierung** — `DocEdgeIndex`-basiertes Tombstoning verknüpfter Graph-Kanten bei Dokument-Superseding (`crates/memfuse-db/src/collection/crud.rs:958`).

### 3.4 Mandanten-Isolation & Sicherheit
- **`TenantId`** (Layer 0, `memfuse-core::types::domain::TenantId(pub u64)`), durchgezogen bis in `TenantIsolatedKvStore` (Layer 1, `memfuse-crypto::kv_segment::store`).
- **Tenant-faire Eviction** (`evict_lru_fair()`, **gemerged** seit `b5a68b9`): Round-Robin-Eviction über sortierte Tenant-IDs verhindert, dass hochfrequente Tenants inaktive Tenants vollständig verdrängen (Cross-Tenant-Starvation-Fix). **Bekannte, noch offene Präzisierungsbedarfe** (Prompts E1/E2, s. Abschnitt 7): (a) es handelt sich um *Count-Fairness pro Runde* (ein Segment/Tenant/Runde), nicht *Byte-Fairness* — der bestehende Doc-Kommentar ist hier bislang unpräzise; (b) die Tenant-Rotationsreihenfolge wird pro Aufruf komplett neu nach aufsteigender ID sortiert, was bei Teil-Runden einen systematischen Bias zulasten früh angelegter Tenants erzeugt; (c) `evict_lru_fair()` hält `self.segments.write()` über die gesamte, potenziell mehrrundige Operation, was Kopf-Blockaden (Head-of-Line-Blocking) für unbeteiligte Tenants' Lesezugriffe verursacht.
- **`McpSandbox`** — Read-Only-MCP-Server-Sandbox mit explizitem Write-Authorization-Guard; MCP läuft laut ADR-010 bewusst als reines `stdio`-Protokoll ohne HTTP-Framework — Gate 4 in `context-gates.yml` erzwingt das aktiv (`grep` gegen `Cargo.toml` von `memfuse-mcp` auf `axum`).

### 3.5 Agenten-Ausführung
- **`PersistentAgentWorkflow`** (`memfuse-agent`) — Multi-Step-Agent-Execution-Loop mit State-Graph und Checkpointing, Zustandsautomat `Pending → Running → Suspended/Completed/Failed`, ausschließlich durch `OrchestratorEngine` transitionierbar.

### 3.6 API-Robustheitsgrenzen (Scan-/Range-Operationen)
Seit v1 als eigenständiges Härtungsthema identifiziert und in aktiver Bearbeitung über mehrere Fix-Prompt-Runden (Prompts B und F, siehe Abschnitt 7):
- `Collection::scan()`/`scan_prefix()` (`memfuse-db/src/collection/crud.rs`) besaßen in v1 kein verpflichtendes Limit — ein Aufruf ohne einschränkendes Präfix konnte bei großen Collections die gesamte Datenmenge in den Speicher laden (OOM-Risiko, auch über die PyO3-FFI-Grenze auslösbar, dort durch `json_to_py()`-Duplizierung zusätzlich verschärft).
- **Zwischenstand (Prompt B, gemergt, Commit `f29c398`):** Eine harte Konstante `MAX_SCAN_RESULTS = 10_000` wurde eingeführt — **abweichend vom ursprünglich in Prompt B spezifizierten Design** (expliziter `LimitExceeded`-Fehler bei Limit-Überschreitung), stattdessen als **stiller Cap** implementiert (Truncation ohne Fehler bei implizitem Limit; `invalid_input`-Fehler nur bei explizit zu großem `limit`-Parameter). Dies ist ein dokumentiertes Beispiel dafür, dass die tatsächliche Jules-Implementierung von der Prompt-Spezifikation abweichen kann und nachfolgende Prompts (hier: F) gegen den *tatsächlichen* Code, nicht gegen die *ursprüngliche Absicht*, verifiziert werden müssen (siehe `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` §6, neuer Abschnitt).
- **Weiterhin offen (Prompt F, noch nicht gemerged):** Selbst mit `MAX_SCAN_RESULTS` wird intern in `LsmStorage::scan()`/`scan_prefix_bounded()` zunächst der **gesamte** angeforderte Bereich als `BTreeMap` materialisiert, bevor das Limit angewendet wird — das Limit schützt aktuell nur das *Ergebnis*, nicht den *internen Speicherverbrauch während der Merge-Phase*. Die geplante Lösung führt eine neue Trait-Methode `scan_bounded()` (Default-Implementierung in `memfuse-core::traits::mod::Storage`, echte Überschreibung in `LsmStorage`) mit früher Merge-Abbruch-Sicherheitsgrenze (`limit × MAX_INTERNAL_MERGE_ENTRIES_FACTOR`) ein.

---

## 4. Bekannte, im Projekt selbst dokumentierte offene Punkte (Stand HEAD, `AGENTS.md` §3/§4)

1. **`memfuse-candle` nicht in Serving-Pipeline verdrahtet** — Crate existiert vollständig, ist aber weder in `memfuse-db`, `memfuse-router` noch `memfuse-ollama` eingebunden.
2. **`rebuild_region()` (F-02) ohne stabile Recall-Regressionshistorie** — reines Tombstone-Pruning in `crates/memfuse-index/src/hnsw.rs`, Feature-Flag `partial-rebuild-pruning`. **Seit v1 konkretisiert:** `VETO-F02` verlangt eine automatisierte 30-Tage-Stabilitätsmessung, die inzwischen als eigener täglicher CI-Workflow existiert (`.github/workflows/nucleation-recall-history.yml`, Ergebnisse in `benchmarks/results/nucleation_recall_history.jsonl`, geprüft via `cargo xtask check-recall-stability`) — die Freigabefrist bleibt `2026-10-07`.

Für die vollständige, laufend aktuelle Liste ist `AGENTS.md` §3/§4 im Repository selbst die maßgebliche Quelle.

---

## 5. Was aus der Architektur bewusst NICHT (mehr) verfolgt wird

- **Cross-Tenant Knowledge Sharing** (F-10, `VETO-F10`) — permanent verworfen, keine Ausnahme vorgesehen (DSGVO-Konflikt mit `DeletionProof`-Korrektheit).
- **Partial-HNSW-Rebuild mit aktivem Re-Wiring** (Kern von F-02, `VETO-F02`, `conditionally_accepted`) — nur reines Tombstone-Pruning ist erlaubt, aktives Re-Wiring bleibt geblockt bis mind. `2026-10-07`.
- **Multi-Tenant-/Enterprise-Orchestrierungs-Features generell** (u. a. F-07, Replikatordynamik) — Entfernungskandidaten seit Vision-Entscheidung (ADR-077).
- **Voice/Jarvis als Produktrichtung** (`VETO-OP3`) — formal zurückgestellt bis 2027-03-08.
- **Desktop-Enterprise-App als primärer Vertriebskanal** — durch ADR-077 abgelöst; verbleibender Code (`memfuse-tauri`) befindet sich weiterhin im Abbau, physische Entfernung zum Zeitpunkt von v2 noch nicht erfolgt.

---

## 6. Mikrofeingranulare Schnittstellenspezifikation je Crate

**Zweck dieses Abschnitts:** Für LLM-Agenten (insbesondere Google Jules), die an genau einer Datei/einem Modul arbeiten sollen, ohne den gesamten Workspace laden zu müssen. Jede Unter-Sektion enthält: Architekturrolle, Modul-Karte, öffentliche Kern-Schnittstellen (mit exakten Signaturen wo aus dem Code extrahiert), kritische Invarianten (mit Fund-ID, falls vorhanden), und Cross-Crate-Kanten. Quelle je Unter-Sektion ist primär die jeweilige `crates/<crate>/AGENTS.md` (verifiziert gegen HEAD `33226e2`), ergänzt um direkte Code-Extraktion für Layer-0-Traits.

### 6.0 Layer 0 — `memfuse-core`: Kern-Trait-Contracts (verbindlich für alle Implementoren)

`memfuse-core` enthält **kein I/O, kein async-Runtime-Binding, kein Netzwerk** in den Typ-Modulen — ausschließlich reine Datenstrukturen und Trait-Contracts (`crates/memfuse-core/src/traits/mod.rs`, 1.813 LOC; `crates/memfuse-core/src/traits/embedding.rs`, 174 LOC).

**Domänen-Newtypes** (`crates/memfuse-core/src/types/domain.rs`):
```rust
pub struct TenantId(pub u64);   // SYSTEM-Tenant = TenantId(0)
pub struct DocId(pub u64);      // Max-Sentinel = DocId(u64::MAX)
pub struct EntityId(pub u64);
pub struct TxId(pub u64);       // Invalid-Sentinel = TxId(0); TxId::INTERNAL_BASE für System-Checkpoints
pub struct ConfigFingerprint { pub model_id: String, /* Quantisierung, etc. */ }
```

**`StorageEngine`-Trait** (einziger Implementor: `memfuse-store::LsmStorage`):
```rust
pub trait StorageEngine: Send + Sync + 'static {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>>;
    fn get_at_seq<'a>(&'a self, key: &'a [u8], seq: u64) -> BoxFuture<'a, Result<Option<Vec<u8>>>>;
    fn put<'a>(&'a self, tx_id: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>>;
    fn put_if_absent<'a>(&'a self, ...) -> BoxFuture<'a, Result<bool>>;
    fn put_batch<'a>(&'a self, ...) -> BoxFuture<'a, Result<()>>;
    fn delete<'a>(&'a self, tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>>;
    fn delete_many<'a>(&'a self, tx_id: TxId, keys: Vec<Vec<u8>>) -> BoxFuture<'a, Result<u64>>;  // Default-Impl
    fn delete_prefix<'a>(&'a self, tx_id: TxId, prefix: &'a [u8]) -> BoxFuture<'a, Result<u64>>;  // Default-Impl
    fn commit<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>>;
    fn rollback<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>>;
    fn rollback_to_tx<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>>;
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>>;
    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<StorageStats>>;
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>>;
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>>;
    fn pin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>>;
    fn unpin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>>;
    fn scan_prefix<'a>(&'a self, ...) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>>;
    fn scan_prefix_bounded<'a>(&'a self, prefix: &'a [u8], limit: usize, cursor: Option<&'a [u8]>)
        -> BoxFuture<'a, Result<(Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>)>>;  // Default-Impl
    fn scan_prefix_at<'a>(&'a self, ...) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>>;
    fn scan<'a>(&'a self, start: Bound<&'a [u8]>, end: Bound<&'a [u8]>)
        -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>>;  // KEINE Default-Impl — Pflichtmethode
    // Stand HEAD 33226e2: KEINE scan_bounded()-Methode — Gegenstand von Fix-Prompt F, offen (§7).
}
```

**`VectorIndex`-, `TextIndex`-, `GraphIndex`-Traits** (Implementoren: `memfuse-index::HnswIndex`, `memfuse-text::InvertedIndex`, `memfuse-graph`) folgen demselben Muster: `insert`/`insert_batch`, `search`/`search_at`/`search_filtered` (MVCC-fähig via Sequenznummer), `delete`, `commit`/`rollback`/`rollback_to_tx`, `last_tx_id`, `len`/`is_empty` (Default-Impl über `len`), `stats`. `GraphIndex` ergänzt `traverse`/`traverse_at`/`traverse_at_time`/`traverse_at_bitemporal` (bi-temporale Achsen, ADR-033), `personalized_page_rank`, `add_entity`/`add_edge`/`add_bidirectional`/`remove_edge`, `multi_traverse`.

**`Checkpoint`/`CheckpointCoordinator`-Traits**: `take_snapshot(tx: TxId) -> WorkflowState`, `restore(state: &WorkflowState)`, `create_named_checkpoint`/`restore_named_checkpoint`/`drop_named_checkpoint`/`list_named_checkpoints`.

**`TextEmbeddingEngine`/`LlmTextGenerator`-Traits**: `embed(text: &str) -> Vec<f32>`, `embed_batch` (Default-Impl über `embed`), `generate(prompt: &str) -> String`.

**`DistanceCalculator`-Trait**: `compute_f32(a, b) -> f32`, `compute_u8(a, b) -> u32` (SQ8-Quantisierung).

**`MemFuseError`** (`crates/memfuse-core/src/error.rs`) — **die einzige Fehler-Enum im gesamten Workspace**, append-only für Binärkompatibilität, `From`-Impls ausschließlich in `error.rs`:
```rust
pub enum MemFuseError {
    Internal(String),
    InvalidInput(String),
    NotFound(String),
    PolicyViolation(String),
    Storage(String),
    Io(#[from] std::io::Error),
    WalCorruption { offset: u64, reason: String },          // #[non_exhaustive]
    ChecksumMismatch { path: String, block_id: ... },        // #[non_exhaustive]
    // ... weitere domänenspezifische Varianten (Crypto, Validation, LimitExceeded (in Härtung, s. §7), ...)
}
```

**Kritische Invarianten (memfuse-core):**
- **Keine-I/O-Garantie:** Layer 0 darf niemals Dateisystem-, Netzwerk- oder async-Operationen enthalten — nur Contracts.
- **Trait-Abwärtskompatibilität:** Neue Trait-Methoden benötigen Default-Implementierungen, um bestehende Implementoren nicht zu brechen.

---

### 6.1 `memfuse-store` (Layer 1, LSM-Tree Storage Engine)
> ~11.570 LOC · 10 Dateien · einziger Implementor von `StorageEngine`

**Modul-Karte:** `lsm.rs` (`LsmStorage` — Orchestrator, implementiert `StorageEngine`) · `wal.rs` (Write-Ahead-Log, HMAC-Chaining, CRC32/Entry) · `memtable.rs` (Skip-List mit Sequenznummern, Tombstones) · `sstable.rs` (Block-komprimierte On-Disk-Segmente, Bloom-Filter, CRC32) · `compaction.rs` (`CompactionEngine`, Tiered/Leveled-Merge) · `checkpoint.rs` (`pub(crate)`, internes MVCC-Pinning — **nicht** die öffentliche API von `memfuse-checkpoint`) · `mmap.rs` (Memory-Mapped-Utilities) · `util.rs` (`pub(crate)`, u. a. `load_or_create_integrity_key`).

**Kern-API (`LsmStorage`, Auszug):** `commit()` (hält `commit_mutex` über gesamte Funktionsdauer inkl. Rollback-Pfad), `rollback_to_tx_locked()` (Doc-Kommentar-Vorbedingung: MUSS nur unter `commit_mutex` aufgerufen werden — s. §3.1/§7 Prompt N5), `WriteAheadLog::truncate(offset: u64, new_last_hmac: [u8; 32])`, `WriteAheadLog::restore_last_hmac(hmac: [u8; 32])`.

**Kritische Invarianten:**
- **fsync Error Propagation (ABSOLUT):** Jeder `sync_all()`/`sync_data()`-Aufruf MUSS Fehler mit `?` propagieren; `let _ = dir.sync_all()` ist verboten (CI Gate 3 erzwingt dies automatisch via `grep`).
- **`last_committed_tx` — Single-Load-Rule:** In `get_at_seq()`/`scan_prefix_at()` einmalig am Start lesen, nicht während der Iteration neu — sonst Bruch der Snapshot-Isolation unter konkurrierenden Writes.
- **WAL-HMAC-Key-Sourcing:** immer `load_or_create_integrity_key()`, niemals hartcodierte Schlüssel.
- **TOCTOU-geschlossen (seit Prompt N2):** `size`/`last_hmac` werden in `truncate()` aktualisiert, während der `file`-Mutex-Guard noch gehalten wird — kein Zeitfenster mehr, in dem ein Leser eine veraltete Größe bei bereits physisch verkürzter Datei sieht.

**Cross-Crate-Kanten:** Implementiert `memfuse-core::StorageEngine`; wird von `memfuse-db`, `memfuse-text`, `memfuse-checkpoint` (intern) konsumiert.

---

### 6.2 `memfuse-index` (Layer 2, HNSW/DiskANN Vektorsuche)
> ~10.666 LOC · 7 Dateien · Implementor von `VectorIndex`

**Modul-Karte:** `hnsw.rs` (`HnswIndex`, Layer-Traversal, Heuristic Node Selection) · `distance.rs` (SIMD-Intrinsics: AVX-512 > AVX2 > NEON > Skalar-Fallback) · `quantize.rs` (`ScalarQuantizer`/SQ8) · `persistence.rs` (`MmapIndex`, `HnswHeader`, Binärformat) · `diskann.rs` (DiskANN Out-of-Core, hinter `experimental-diskann`, **nicht** in `default`-Features).

**Kern-API:** `search_internal(query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>` (öffentlicher DiskANN-Sucheinstiegspunkt, `pub async fn`), interner Robust-Pruning-Vergleich `alpha * dist_p_cand < cand.distance` in den Vamana-Build-Pässen (`for alpha in [1.0f32, 1.2f32]`).

**Kritische Invarianten:**
- **SIMD-Hardware-Dispatch:** Fallback auf Skalar muss exakt dieselben mathematischen Ergebnisse liefern wie die SIMD-Pfade.
- **`unsafe`-Scope (ADR-017):** `distance.rs` darf `#![allow(unsafe_code)]`; `diskann.rs`/`persistence.rs` dürfen exakt ein `unsafe { Mmap::map(&file) }` enthalten; jedes `unsafe` benötigt einen `// SAFETY:`-Beweis-Kommentar; modulweites `#![allow(unsafe_code)]` ist workspaceweit verboten.
- **NaN/Inf-Guards im HNSW-Pfad bereits etabliert** (`is_nan() || is_infinite()`-Checks, `SearchCandidate::cmp` via `f32::total_cmp`) — **DiskANN (`diskann.rs`) hatte dieselbe Härtungslücke wie ursprünglich `fusion.rs`, Stand v1 unadressiert.**
- **Seit v1, Status offen (Fix-Prompts D2 und G, §7):** (a) α-Pruning-Vergleiche bei ca. Zeile 499/834 prüfen `dist_p_cand`/`cand.distance` noch nicht explizit auf `is_finite()` vor dem Vergleich (Fail-Open-Verhalten bei NaN bleibt beabsichtigt, soll aber defensiv geloggt werden); (b) `search_internal()` validiert den `k`-Parameter bereits, aber **nicht** den `query`-Vektor selbst auf `is_finite()`-Komponenten, bevor er in den HNSW-Fallback- oder DiskANN-Traversal-Pfad eintritt — führt zu stillschweigend ungewöhnlich zusammengesetzten statt klar fehlschlagenden Ergebnissen bei korruptem Upstream-Embedding.

**Cross-Crate-Kanten:** Implementiert `memfuse-core::VectorIndex`; konsumiert von `memfuse-db` (Signal 1 der 4-Signal-Fusion).

---

### 6.3 `memfuse-db` (Layer 3, Hauptdatenbank / Collection API)
> ~19.686 LOC · 27 Dateien · primäre High-Level-API für lokale Agenten

**Modul-Karte:** `collection/` (`Collection`, Tx-Allokation via `next_tx`, Insert/Search) · `fusion.rs` (4-Signal-RRF, `weighted_reciprocal_rank_fusion_with_options()`, `HeapEntry`) · `context.rs` (`ContextManager`, `SpatialFence`, Token-Counting) · `context_compaction.rs` (`ContextCompactor`, `ConsolidationSession`) · `chunker.rs` (`MarkdownChunker`) · `multistep.rs` (`MultiStepEngine`, `QueryRewriter`) · `transaction.rs` (`CommitIntent`) · `background_workers.rs` (Expiry-/Orphan-Cleanup-Worker).

**Kern-API:**
```rust
// collection/crud.rs
pub async fn scan_prefix(&self, prefix: &str, limit: Option<usize>) -> Result<Vec<(String, serde_json::Value)>>;
pub async fn scan(&self, start: Bound<...>, end: Bound<...>, limit: Option<usize>) -> Result<Vec<(String, serde_json::Value)>>;
pub const MAX_SCAN_RESULTS: usize = 10_000;  // seit Prompt B (gemergt) als stiller Cap, kein LimitExceeded-Fehler bei implizitem Limit
```
```rust
// fusion.rs
pub fn weighted_reciprocal_rank_fusion_with_options(result_sets: ..., k: ..., options: ...) -> Vec<SearchResult>;
impl Ord for HeapEntry { fn cmp(&self, other: &Self) -> Ordering { /* aktuell partial_cmp().unwrap_or(Equal) — s. §7 Prompt H */ } }
```

**Kritische Invarianten:**
- **`TxId`-Generierung (AGT-DB-001):** MUSS immer über `collection.allocate_tx().await` bezogen werden, niemals `SystemTime` (Kausalitätsbruch bei Graph & LSM).
- **4-Signal-Fusion-Pflicht:** `collection.search()` fragt Vector (HNSW), Text (BM25), Graph (PPR), Storage (LSM) asynchron parallel ab; Ergebnisse MÜSSEN durch `reciprocal_rank_fusion` laufen.
- **`MarkdownChunker`-Pflicht:** Agenten-Wissen darf nicht als monolithischer String an die Embedding-Engine gegeben werden.
- **RRF-NaN-Härtung (seit Prompt N1, gemergt):** `weight`-Guard vor Akkumulation, Frühwarn-Logging für nicht-endliche `doc.score`-Werte aus Upstream-Signalen.
- **Scan-Limit-Härtung:** s. Abschnitt 3.6 und §7 (Prompts B/F).

**Cross-Crate-Kanten:** Konsumiert `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-graph`, `memfuse-embed`/`memfuse-ollama` (Embeddings); wird konsumiert von `memfuse-py`, `memfuse-agent`, `memfuse-mcp`, `memfuse-tauri`.

---

### 6.4 `memfuse-crypto` (Layer 1, Encryption-at-Rest, HMAC, Zeroize, KV-Segment-Store)
> ~2.887 LOC · 11 Dateien · **enthält seit Konsolidierung auch den ehemaligen `memfuse-kv-bridge`-Bestandteil** (`src/kv_segment/`)

**Modul-Karte:** `crypto.rs` (`KeyManager` — HKDF-Subkey-Derivation, AES-256-GCM-SIV) · `wal_crypto.rs` (`WalHmac`, `IntegrityVerifier`, `EncryptedWal`) · `anti_tamper.rs` (`VolatileEncryptionKey`, Zeroize) · `deletion_proof.rs` (`DeletionProof`, `DeletionLayer`, `DeletionScope`, `ExcludedScope`) · `kv_segment/store.rs` (`TenantIsolatedKvStore`, `evict_lru_fair()`, `evict_lru_global()`).

**Kern-API:**
```rust
// deletion_proof.rs — Zeile ~104
pub fn create(
    scope: DeletionScope,
    deleted_keys: Vec<Vec<u8>>,
    deleted_after_tx: TxId,
    covered_layers: Vec<DeletionLayer>,   // Härtungsziel Prompt D1: -> Vec<LayerCleanupProof>, s. §7
    excluded_scopes: Vec<ExcludedScope>,
    proof_key: &[u8],
) -> Result<Self>;

pub enum DeletionLayer {
    LsmMemtable, SsTableAllLevels, HnswIndex,
    WalAllSegments { seq_after: ... }, CsrGraph, KvCacheSegments, EmbeddingCache,
}
```
```rust
// kv_segment/store.rs
pub fn evict_lru_global(&self, target_free_bytes: usize) -> usize;  // Sichtbarkeit Härtungsziel, s. §7 Prompt E1
pub fn evict_lru_fair(&self, target_free_bytes: usize) -> usize;    // Round-Robin über sortierte TenantIds
```

**Kritische Invarianten:**
- **Key Derivation Kette:** Passphrase + Salt → Master Key → `derive_file_key(file_id)` → Subkey; hartcodierte Schlüssel = SECURITY BLOCKER.
- **HMAC Chaining (WAL):** Bruch der Kette → `MemFuseError::WalCorruption`, hartes Abbrechen.
- **Zeroize-Garantie:** Schlüsselmaterial implementiert `ZeroizeOnDrop`, darf Scope nicht als Klartext (`String`/`Vec<u8>`) verlassen.
- **Nonce-Uniqueness:** `encrypt_auto_nonce` erzeugt für jede Operation eine neue `OsRng`-Nonce, trotz GCM-SIV-Nonce-Reuse-Resistenz.
- **`INV-DELETION-1`:** Aufrufreihenfolge — (1) alle `covered_layers` physisch bereinigen, (2) WAL-Commit mit Lösch-Intent, (3) `DeletionProof::create()` — Stand v1/v2 **nur durch Kommentar-Konvention gesichert**, Typsystem-Erzwingung Gegenstand von Prompt D1 (§7, offen).
- **Cross-Tenant-Starvation-Fix:** s. Abschnitt 3.4/§7 (Prompts E1/E2, offen).

**Cross-Crate-Kanten:** `memfuse-store` (WAL-Verschlüsselung), `memfuse-index`/`memfuse-graph` (KV-Cache-Segmente), Aufrufer von `DeletionProof::create()` verteilt über mehrere Crates (genaue Liste ist Teil der Prompt-D1-Analysepflicht, s. §7).

---

### 6.5 `memfuse-graph` (Layer 1, CSR-Graph, Entity-Relation-Traversal)
> ~9.189 LOC · 12 Dateien · Implementor von `GraphIndex`

**Kritische Invarianten:**
- **System-Präfixe:** Interne LSM-Scan-Präfixe müssen vor normalen User-Daten verborgen werden.
- **Bi-temporale Kanten (ADR-033):** `valid_from`/`valid_to` basierend auf `TxId`; bei `insert_edge_direct_with_bitemporal_validity` müssen verfallene Kanten (`current_tx > valid_to`) bei Traversierungen ausgeblendet werden.
- **`TxId`-Origin-Invariante (AGT-GRAPH-001):** `TxId`-Argumente für Graph-Updates müssen aus der Collection-eigenen `next_tx`-Sequenz oder `TxId::INTERNAL_BASE` stammen — Wall-Clock-Zeit korrumpiert `rollback_to_tx()`.
- **`GraphEdge`-Relation-Synchronisation:** `relate()`-Aufrufe in Layer 2 (Collection) MÜSSEN synchron `graph_index.add_edge()` triggern.

**Cross-Crate-Kanten:** Implementiert `memfuse-core::GraphIndex`; konsumiert von `memfuse-db` (Signal 3 der 4-Signal-Fusion).

---

### 6.6 `memfuse-text` (Layer 1, BM25 Volltextsuche)
> ~3.994 LOC · 5 Dateien · Implementor von `TextIndex`

**Modul-Karte:** `inverted.rs` (`InvertedIndex`, transaktional via `StorageEngine`) · `bm25.rs` (`BM25`-Scoring) · `tokenizer.rs` (`Tokenizer`-Trait, `DefaultTokenizer`, `GermanMorphTokenizer`) · `morphology.rs` (`GermanCompoundSplitter`, Umlaut-Normalisierung).

**Kritische Invarianten:**
- **Determinismus der Tokenisierung:** Query-Pfad MUSS exakt dieselbe Pipeline wie der Indexierungs-Pfad durchlaufen — Diskrepanz führt zu Silent-Recall-Drops.
- **Snapshot Isolation (`search_at`):** Nutzt `get_at_seq()`/`scan_prefix_at()` für MVCC-Korrektheit.
- **Transaction-Aware Storage:** Persistiert keine eigenen Dateien, nutzt `StorageEngine`; Mutationen müssen `TxId` weiterreichen für atomare Commits mit Vektor-/Graph-Updates.

**Cross-Crate-Kanten:** Implementiert `memfuse-core::TextIndex`; konsumiert von `memfuse-db` (Signal 2 der 4-Signal-Fusion).

---

### 6.7 `memfuse-checkpoint` (Layer 1, Snapshot-Pinning, RAII-Transaktions-Guards)
> ~2.666 LOC · 1 Datei (`lib.rs`, bewusst monolithisch gehalten)

**Kern-API:** `CheckpointGuard` (RAII), `PersistentCheckpointStore`, `StateCheckpoint`.

**Kritische Invarianten:**
- **RAII-Semantik:** `CheckpointGuard` muss konsumiert werden — `commit()` überführt in dauerhaften Checkpoint, `rollback()`/`drop(guard)` verwirft; da `Drop` in async-Rust nicht asynchron sein kann, registriert der synchrone Drop-Handler die Transaktion als „orphaned"; ein asynchroner Reaper (`recover_orphaned_checkpoints`) räumt später auf.
- **`TxId`-Zuweisung für System-Checkpoints:** nutzt `TxId::INTERNAL_BASE` aufwärts, um Konflikte mit regulären Dokument-/Kanten-Insertionen zu vermeiden.
- **Snapshot Pinning:** `PersistentCheckpointStore` ruft `storage.pin_checkpoint(seq_no)`; gepinnte Checkpoints verhindern LSM-Compaction von noch benötigten Versionen; bei Löschung eines Checkpoints MUSS `unpin_checkpoint` erfolgen.

**Cross-Crate-Kanten:** Konsumiert `memfuse-core::Checkpoint`/`CheckpointCoordinator`, `memfuse-store` (Pinning).

---

### 6.8 `memfuse-calibration` (Layer 0, Kalibrierungsprimitive)
> ~1.282 LOC · 5 Dateien

**Kern-API:** `IsotonicCalibrator`, `PlattScaler`, `invalidate_on_config_change(new_fingerprint: ConfigFingerprint)`.

**Kritische Invarianten:** Layer-0-Platzierung (hängt nur von `memfuse-core` ab); `#![forbid(unsafe_code)]`; P8-Compliance (Kalibrierung MUSS bei Konfigurationsänderung via `ConfigFingerprint` invalidiert werden).

**Cross-Crate-Kanten:** Konsumiert von `memfuse-router`, `memfuse-embed`, `memfuse-db`.

---

### 6.9 `memfuse-candle` (Layer 1, natives GGUF-Inferenz-Backend)
> ~1.216 LOC · 7 Dateien · **Stand HEAD: nicht in Serving-Pipeline verdrahtet** (s. Abschnitt 4)

**Kern-API:** `CandleLlmGenerator`, `CandleEmbedClient`.

**Kritische Invarianten:** Zero-Panic-Doktrin (kein `.unwrap()`/`.expect()`); Candle-Tensor-Operationen sind CPU-blockierend und MÜSSEN via `tokio::task::spawn_blocking` ausgeführt werden.

---

### 6.10 `memfuse-embed` (Layer 2, ONNX-Embeddings & Reranking, optional)
> ~1.674 LOC · 2 Dateien · Feature-Gate `onnx`, default deaktiviert

**Kern-API:** `TextEmbedder`, `TextEmbedderConfig`, `SessionPool` (`pub(crate)`), `CrossEncoderReranker`.

**Kritische Invarianten:** Gesamte Crate hinter `#[cfg(feature = "onnx")]`; ONNX-`session.run()`-Aufrufe MÜSSEN via `spawn_blocking` (Methode `embed_async`) gekapselt werden; `TextEmbedder` wird via `Arc` geteilt, nicht pro Aufruf neu konstruiert.

---

### 6.11 `memfuse-ollama` (Layer 2, Ollama-HTTP-Client)
> ~3.925 LOC · 6 Dateien · Implementor von `TextEmbeddingEngine`

**Modul-Karte:** `client.rs` (`OllamaClient`, Retry-Logik) · `embedding.rs` (`OllamaEmbedder`) · `context_prefixer.rs` (`ContextPrefixEngine`, 50–100-Token-Präfixe) · `importance.rs` (`score_importance`) · `model_info.rs` (`ModelInfo`, `known_dimension()`).

**Kritische Invarianten:** `try_embed_batch`/`try_generate_text` implementieren Exponential-Backoff-Retry; `ContextPrefixEngine`-Präfixe müssen strikt via `truncate_prefix` an Token-Grenzen gehalten werden; HNSW-Dimension muss statisch oder via Dummy-Embedding ermittelt werden.

---

### 6.12 `memfuse-mcp` (Layer 6, Model Context Protocol Server)
> ~3.175 LOC · 7 Dateien

**Modul-Karte:** `protocol.rs` (`McpError`, JSON-RPC-2.0-Parser) · `sandbox.rs` (`McpSandbox`, `SandboxPolicy`, `VolatileToolResult`) · `prompt_injection.rs` (`PromptInjectionGuard`, `SecurityAuditLogger`).

**Kritische Invarianten:**
- **Nur-Stdio (ADR-010):** Kein `axum`/`hyper` — Security-Blocker, durch Gate 4 erzwungen.
- **Sandbox-Defaults:** `allow_db_reads: true`, `allow_db_writes: false` (Opt-in via `MEMFUSE_MCP_WRITE_ALLOW`), `allow_code_execution: false` (strikt).
- **Prompt-Injection-Quarantäne:** Treffer auf Patterns wie „ignore all previous instructions" lösen sofortige Quarantäne + unwiderrufliche `SecurityAuditLogger`-Erfassung aus.
- **Volatile Results:** Große/sensitive Tool-Ergebnisse (`MAX_VOLATILE_OUTPUT_BYTES = 16 MB`) werden verschlüsselt im RAM gehalten statt persistiert.

---

### 6.13 `memfuse-agent` (Layer 5, Agenten-Orchestrierung)
> ~2.757 LOC · 8 Dateien

**Modul-Karte:** `engine.rs` (`OrchestratorEngine`, `run_event_loop`) · `graph.rs` (`StateGraph`, `AgentNode`, `WorkflowEdge`) · `context.rs` (`AgentContext`, `AgentStatus`) · `event_source.rs` (`EventSource`-Trait, `PollingDocumentEventSource`) · `step.rs` (`AgentTool`-Trait, `StepResult`) · `audit.rs` (`AuditLog`, `AuditEntry`).

**Kritische Invarianten:**
- **State-Machine:** `Pending → Running → Suspended/Completed/Failed`; nur `OrchestratorEngine` darf Transitionen auslösen.
- **Identifier-Validierung (AGT-AGN-001):** `task_id`/`node_id` nicht leer, keine Null-Bytes, max. 256 Bytes, keine Pfad-Trenner.
- **Resource Limits:** `MAX_EVENT_SOURCE_CAPACITY` (z. B. 1000 Events/Source).

---

### 6.14 `memfuse-router` (Layer 4, SLM-Routing, Conformal Calibration)
> ~4.838 LOC · 8 Dateien

**Modul-Karte:** `router.rs` (`RouterEngine`, `RoutingDecision`, `ConfidenceMetrics`) · `profile.rs` (`SlmProfile`, `ConformalCalibrator`, `ProfileCalibrationState`) · `dispatch.rs` (`dispatch_to_slm`).

**Kritische Invarianten:**
- **Token-Budget-Einhaltung:** `RouterEngine` darf nie ein Modell wählen, dessen `max_context_tokens` für das aktuelle `ContextWindow` nicht ausreicht — sonst muss `ContextCompactor` (Layer 3) vorab trimmen.
- **Conformal Calibration Update (AGT-RTR-001):** `ProfileCalibrationState` muss nach jeder Interaktion anhand des Non-Conformity-Scores geupdated werden.
- **Community-Score-Boost:** Domänen-Fine-Tuning erhält internen Score-Boost (z. B. `1.2×`).

---

### 6.15 `memfuse-py` (außerhalb Root-Workspace, PyO3-FFI, ADR-064)
> ~1.617 LOC · 1 Datei

**Modul-Karte:** `lib.rs` (`PyMemFuse`, `PyCollection`) · `types.rs` (NumPy/Dict/String-Konvertierung) · `error.rs` (FFI-Error-Mapping) · `gil.rs` (GIL-Management).

**Kritische Invarianten:**
- **GIL-Release-Protokoll (AGT-PY-001):** Blockierende/asynchrone MemFuse-Aufrufe MÜSSEN in `py.allow_threads(|| { ... })` gewrappt werden.
- **FFI-Error-Mapping:** `NotFound → KeyError/ValueError`, `Storage → IOError`, `Validation → ValueError`, `Internal → RuntimeError`.
- **NumPy-Zero-Copy:** Vektor-Embeddings möglichst als `PyReadonlyArray1<f32>` entgegennehmen.
- **Separater Build:** `cd crates/memfuse-py && cargo build --release` (abweichendes Panic-Profil, s. Abschnitt 2).

---

### 6.16 `memfuse-tauri` (Layer 4, Desktop-Backend, DEPRECATED via ADR-077)
> ~4.340 LOC · 18 Dateien · **physische Entfernung ausstehend, Zielfrist ≈ 07.11.2026**

**Modul-Karte:** `state.rs` (`AppState`) · `ollama.rs` (`OllamaBridge`) · `commands/` (`ingest.rs`, `chat.rs`, `transform.rs`) · `ingestion/` (`pipeline.rs`, `pdf.rs`, `docx.rs`, `email.rs`, `progress.rs`).

**Kritische Invarianten (weiterhin gültig bis zur Entfernung):**
- **AppState Guard-Drop-vor-Await-Regel (AGT-TAU-001):** `parking_lot::RwLock`-Guards auf `AppState` dürfen niemals über einen `.await`-Punkt gehalten werden — sonst UI-Thread-Deadlock.
- **IPC-Command-Protokoll:** Alle `#[tauri::command]`-Funktionen mit I/O müssen async sein; Fehler MÜSSEN in `MemFuseErrorDto` gemappt werden.
- **Progress Emission Throttling:** `IngestProgressThrottler` begrenzt UI-Updates auf z. B. 100 ms-Intervalle.

---

## 7. Statusmatrix: laufende Härtungs-Fix-Prompts (verifiziert gegen HEAD `33226e2`)

Diese zehn Fix-Prompts (drei Runden: N1–N5, B/E1/E2/D1/D2, F/G/H) wurden aus drei separaten, vertieften Stabilisierungsanalysen abgeleitet und sind isoliert, direkt an Google Jules sendbare Prompts (Details zum Prompt-Aufbau und zur Parallelisierungslogik: `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` §6). Der folgende Status wurde durch direkte Code-Verifikation gegen HEAD `33226e2` ermittelt, **nicht** aus den Prompt-Dateien übernommen — die Prompt-Dateien selbst dokumentieren nur die *Absicht*, nicht notwendigerweise das *tatsächliche Implementierungsergebnis* (s. Abschnitt 3.6, Prompt B als dokumentiertes Abweichungsbeispiel).

| ID | Titel | Crate/Datei | Schweregrad | Status HEAD `33226e2` | Beleg |
|---|---|---|---|---|---|
| **N1** | RRF-Fusion: NaN/Inf-Guard für `weight`/`doc.score` | `memfuse-db/src/fusion.rs` | Hoch | ✅ **Gemergt** (`1fedb8d`) | `!weight.is_finite()`-Guard verifiziert im Code |
| **N2** | WAL `truncate()`: atomare `size`/`last_hmac`-Aktualisierung | `memfuse-store/src/wal.rs` | Mittel | ✅ **Gemergt** (`59ce141`) | Commit-Message „close TOCTOU window during WAL truncation" |
| **N3** | LSM: Drift-Zähler für fehlschlagendes `budget.consume_memory()` | `memfuse-store/src/lsm.rs` | Mittel | ✅ **Gemergt** (`4dff111`) | `budget_tracking_drift_bytes`-Atomic-Metrik verifiziert |
| **N4** | Regressionstest: `restore_last_hmac()`-Rollback-Race | neue Testdatei | Niedrig-Mittel | ✅ **Gemergt** (`33226e2`) | Commit „verify WAL HMAC rollback concurrency invariant" |
| **N5** | Typsystemische Erzwingung `commit_mutex → state` (optional) | `memfuse-store/src/lsm.rs` | Niedrig | ✅ **Gemergt** (`de319a4`) | Commit „enforce commit_mutex for rollback_to_tx_locked" |
| **B** | Verpflichtendes Limit für `scan()`/`scan_prefix()` | `memfuse-db`, `memfuse-py` | Hoch | ✅ **Gemergt, abweichend implementiert** (`f29c398`) | `MAX_SCAN_RESULTS`-Konstante als stiller Cap statt `LimitExceeded`-Fehler (s. §3.6) |
| **E1** | Härtung `evict_lru_fair()`: toter Pfad, Fairness-Bias | `memfuse-crypto/src/kv_segment/store.rs` | Mittel | ⏳ **Offen** | `evict_lru_global()` weiterhin `pub`, kein `eviction_round_offset`-Feld im Code gefunden |
| **E2** | Lock-Granularität `evict_lru_fair()` (Batch-Freigabe) | `memfuse-crypto/src/kv_segment/store.rs` | Mittel | ⏳ **Offen** (setzt E1 voraus) | Kein `MAX_ROUNDS_PER_LOCK_ACQUISITION` im Code gefunden |
| **D1** | Typsystemische Erzwingung `DeletionProof::create()`-Vorbedingung | `memfuse-crypto/src/deletion_proof.rs` + Call-Sites | Hoch (Compliance) | ⏳ **Offen** | Kein `LayerCleanupProof`-Typ im Code gefunden |
| **D2** | NaN/Inf-Guard im DiskANN-Robust-Pruning | `memfuse-index/src/diskann.rs` | Mittel | ⏳ **Offen** | Kein `is_finite()`-Check in den referenzierten α-Pruning-Zeilen gefunden |
| **F** | Speicherbeschränkter Range-Scan `scan_bounded` | `memfuse-core`, `memfuse-store`, `memfuse-db` | Hoch (OOM) | ⏳ **Offen** (setzt B voraus, erfüllt) | Keine `scan_bounded`-Trait-Methode in `memfuse-core::traits::mod` gefunden |
| **G** | DiskANN `search_internal()`: Query-Vektor-Validierung | `memfuse-index/src/diskann.rs` | Niedrig-Mittel | ⏳ **Offen** | Kein `query.iter().all(\|v\| v.is_finite())`-Check gefunden |
| **H** | Nachtrag N1: `HeapEntry` auf `total_cmp`, Release-Sichtbarkeit `INV-PROV-1` | `memfuse-db/src/fusion.rs` | Mittel | ⏳ **Offen** (setzt N1 voraus, erfüllt) | `impl Ord for HeapEntry` nutzt weiterhin `partial_cmp().unwrap_or(Ordering::Equal)` |

**Zusammenfassung:** 6 von 10 Fix-Prompts sind zum Zeitpunkt dieser Spezifikation gemergt (N1–N5, B), davon einer (B) mit dokumentierter Implementierungsabweichung von der ursprünglichen Prompt-Spezifikation. Vier Fix-Prompts (E1, E2, D1, D2, F, G, H — korrigiert: sieben, siehe Tabelle) sind noch offen; drei davon (E2, F, H) haben eine explizite Merge-Reihenfolge-Abhängigkeit von bereits gemergten bzw. noch offenen Vorgänger-Prompts, die bei künftiger Bearbeitung beachtet werden muss (s. `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` §6.2).
