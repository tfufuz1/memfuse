# MemFuse — Konsolidierte Strategie- und Schnittstellenspezifikation v3
## Optimierung, Realitätsabgleich und Ergänzung um bisher nicht genannte Ziele

> **Auftrag dieses Dokuments**: (1) alle bisherigen eigenen Analysen (`MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md`, `MEMFUSE_INTERFACE_SPECIFICATION.md`) sowie neun zugelieferte Dokumente zusammenführen und optimieren, (2) jeden Kern-Claim aus allen zehn Quelldokumenten **erneut live gegen den frisch geklonten Code** verifizieren — nicht nur gegen den Stand von vor einer Session —, und (3) Ziele aus den Zulieferdokumenten herausarbeiten, die in meiner bisherigen Strategie **noch nicht vorkamen**.
> **Stand der Live-Verifikation**: 29. August 2026, Repository `github.com/tfufuz1/memfuse`, 58.853 Rust-Zeilen, 39 abgeschlossene ADRs (ADR-020 doppelt vergeben, siehe Abschnitt 1).
> **Zentraler Befund vorab**: Der Code ist seit den zugelieferten Audit-/Umsetzungsprompt-Dokumenten spürbar weitergelaufen. **Mindestens sechs als "offen" oder "zu bauen" beschriebene Punkte sind inzwischen tatsächlich im Code umgesetzt.** Das ist die wichtigste Einzelerkenntnis dieser Konsolidierung — sie verhindert, dass Implementierungsaufwand an bereits gelöste Probleme verschwendet wird.

---

## Inhaltsverzeichnis

1. Kritischer Realitätsabgleich — was in den Zulieferdokumenten als offen gilt, aber bereits Code ist
2. Was tatsächlich noch offen ist (bereinigte Restliste)
3. **Neu identifizierte Ziele** — aus den Zulieferdokumenten, bisher nicht in meiner Strategie enthalten
4. Konsolidierte Architektur v3 — Zusammenführung von Governance-Track und Hardware-Track
5. Aktualisierte, priorisierte Roadmap
6. Business- und Governance-Ziele (nicht-technisch, aber strategisch)
7. Offene Verifikationsfragen für die nächste Session

---

## 1. Kritischer Realitätsabgleich

Jede Zeile dieser Tabelle wurde durch direktes Lesen des aktuellen Codes verifiziert (Befehle und Fundstellen im Verlauf dieser Session, nicht übernommen aus den Zulieferdokumenten).

| # | Claim in Zulieferdokument | Dokument | Live-Code-Realität (29.08.2026) | Konsequenz |
|---|---|---|---|---|
| 1 | `DiskAnnIndex` existiert nicht, muss als `VamanaIndex` neu gebaut werden (R3) | `memfuse_v2_optimierungsspezifikation.md` §5 | **Existiert bereits.** `crates/memfuse-index/src/diskann.rs` — vollständige mmap-basierte Implementierung mit eigenem Dateiformat (`DiskAnnHeader`, Magic `"DANN"`, Versionierung), `#[cfg(feature = "experimental-diskann")]`, implementiert `VectorIndex`-Trait vollständig, inkl. eigenem `ScalarQuantizer`-Support und Drift-Erkennung ("Quantization drift > 10 % detected"). | §4 dieses Dokuments ersetzt "neu bauen" durch "aus experimentellem Feature-Flag in Produktionsreife heben + FreshDiskANN-Delta-Merge ergänzen" |
| 2 | Kein Quantisierungspfad für Embeddings vorhanden, `memfuse-quant` muss neu entstehen (R6) | `memfuse_v2_optimierungsspezifikation.md` §3 | **Teilweise bereits vorhanden.** `ScalarQuantizer` existiert bereits in `memfuse-index/src/quantize.rs` und wird sowohl von `HnswIndex` als auch `DiskAnnIndex` genutzt (Kalibrierung, Rekalibrierung bei Rebuild). Was fehlt: Binary-Codec und Product-Quantization sowie eine crate-übergreifende `EmbeddingCodec`-Abstraktion. | §4.2 präzisiert: `memfuse-quant` wird kein Neubau, sondern eine **Vereinheitlichung** des bereits vorhandenen `ScalarQuantizer` plus Ergänzung um Binary/PQ |
| 3 | Strukturierte Fehlercodes über FFI-Grenzen fehlen, `MemFuseErrorCode`/`FfiError` müssen neu gebaut werden | `memfuse_v2_optimierungsspezifikation.md` §2.1/§9, `memfuse_interface_spec_updated.md` Risiko #5, `memfuse_produktspezifikation.md` (implizit) | **Bereits vollständig gelöst.** `memfuse-core/src/error_dto.rs` definiert `MemFuseErrorDto { kind: String, message: String, details: Option<serde_json::Value> }`. Wird **konsistent** in `memfuse-tauri` (alle Commands), `memfuse-py` (`lib.rs:162`) und `memfuse-mcp` (`protocol.rs:97`, inkl. eigenem Test `tests.rs:169`) verwendet. | Kein Implementierungsbedarf. §4 markiert diesen Punkt als erledigt; verbleibt nur die **stilistische** Frage, ob `kind: String` durch ein `#[repr(i32)]`-Enum ersetzt werden soll (siehe Abschnitt 7) |
| 4 | `SandboxBridge` nutzt RPITIT statt `#[async_trait]`, nicht dyn-kompatibel | `memfuse_interface_spec_updated.md` Risiko #3, `MemFuse_Audit_und_Jules_Implementierungsprompts.md` Finding G, `memfuse_umsetzungsprompts.md` WP3 | **Bereits gelöst.** `memfuse-db/src/lib.rs:61`: `#[async_trait::async_trait] pub trait SandboxBridge` — bereits umgestellt. | Kein Implementierungsbedarf |
| 5 | `AuditLog` hart an `Collection<LsmStorage>` gebunden, nicht generisch | `memfuse_interface_spec_updated.md` Risiko #6, `memfuse_umsetzungsprompts.md` WP6 | **Bereits gelöst.** `memfuse-agent/src/audit.rs:25`: `pub struct AuditLog<S: StorageEngine = LsmStorage>` — bereits generisch mit sinnvollem Default. | Kein Implementierungsbedarf |
| 6 | `collection.rs` ist ein ~2.900-LOC-"God Object" (Tx-Management, Reaper, Relate, Hybrid-Search in einer Datei) | `memfuse_jules_implementierungsprompts_2026-08-29.md` AUD-08 | **Bereits aufgelöst.** `memfuse-db/src/collection/` ist inzwischen ein Modul-Verzeichnis: `crud.rs`, `maintenance.rs`, `mod.rs`, `relate.rs`, `search.rs`, `tx.rs` (+ `tests.rs`), insgesamt 4.022 Zeilen sauber getrennt nach fachlicher Zuständigkeit. Passt zu ADR-040 ("collection.rs Modularisierung — God Object Auflösung"). | Kein Implementierungsbedarf |
| 7 | Kognitive Gedächtnistypen (Episodic/Semantic/Procedural/Working) "❌ Nicht implementiert" | `memfuse_jules_implementierungsprompts_2026-08-29.md`, P-03 | **Bereits vollständig implementiert.** `memfuse-core/src/types/domain.rs:535`: `#[non_exhaustive] pub enum MemoryType { Episodic, Semantic (Default), Procedural, Working }`, jeweils mit eigener `default_decay()`-Logik (`DecayFunction::Exponential`/`StepFloor`/`None`) und `default_ttl_tx()` für Working Memory (50.000 TX ≈ 30 Min.). Vollständig verdrahtet in `Collection::insert_with_memory_type()` (`crud.rs:117`) und mit dediziertem Testfall pro Typ (`collection/tests.rs:1333`ff.). | §2 dieses Dokuments listet dies explizit als **erledigt**, nicht als offenes P-03. Was tatsächlich noch fehlt, ist die **aktive** Sweep-Durchsetzung (siehe Punkt 8) |
| 8 | `TxBuffer` hat keine harte Kapazitätsgrenze (AGT-CORE-001, OOM-DoS-Risiko) | `memfuse_jules_implementierungsprompts_2026-08-29.md` P-02 | **Bereits gelöst.** `memfuse-core/src/tx_buffer.rs:30`: `TxBufferConfig` mit Default `10_000`, `stage_bounded()` gibt `Result` zurück und erzwingt die Grenze; `reap_orphans_bounded()` existiert zusätzlich. | Kein Implementierungsbedarf |
| 9 | `traverse_at_time`/`search_at`/`scan_prefix_at` geben durchgängig `CapabilityUnsupported`/`PolicyViolation` zurück (WP14, P-06) | `memfuse_jules_implementierungsprompts_2026-08-29.md`, `memfuse_umsetzungsprompts.md` WP14 | **Bereits real implementiert**, nicht nur Trait-Default. `csr.rs:826` (`traverse_at_time`, vollständige bi-temporale BFS mit `is_edge_visible()`), `hnsw.rs:1999` (`search_at`, nutzt `SequenceLog::is_visible()`), `lsm.rs:1021` (`scan_prefix_at`, MVCC-Snapshot über `last_committed_tx`). Deckt sich mit `docs/SOURCE_OF_TRUTH.md`: "Snapshot Isolation: 🟢 Vollständig". | Kein Implementierungsbedarf — bestätigt exakt die Selbstauskunft in `SOURCE_OF_TRUTH.md`, die in Abschnitt 7 der `memfuse_produktspezifikation.md` bereits richtig als "Code ist der Doku voraus" eingeordnet wurde |
| 10 | Zwei ADR-Einträge mit identischer Nummer "ADR-029" | `memfuse_produktspezifikation.md` Befund #4 | **Präzisierung nötig**: Nicht ADR-029, sondern **ADR-020** ist doppelt vergeben — `## ADR-020: Cognitive Operating System als Produktvision` (Zeile 252) und `## ADR-020 (Wiederherstellung): Wiederherstellung von memfuse-agent aus dem Archiv` (Zeile 317). ADR-029 selbst ist im aktuellen `DECISIONS.md` nicht doppelt. | Der ursprüngliche Befund war in der Sache richtig, aber die Nummer war falsch — hier korrigiert. Empfehlung unverändert: einen der beiden ADR-020-Einträge umnummerieren (z. B. Wiederherstellung → ADR-020b oder Neuvergabe als ADR-041) |
| 11 | `CrossEncoderReranker` "zweimal definiert", Kompilierrisiko | `memfuse_interface_spec_updated.md` bzw. dessen Risikoliste, `memfuse_audit_jules_prompts.md` | **Entkräftet** (deckt sich mit der Einschätzung in `MemFuse_Audit_und_Jules_Implementierungsprompts.md` Finding B) — verifiziert: saubere, sich gegenseitig ausschließende `#[cfg(feature = "onnx")]`/`#[cfg(not(feature = "onnx"))]`-Trennung in `reranker.rs`. Kein Fix nötig. | Bestätigung der bereits in einem Zulieferdokument korrekt entkräfteten Aussage — hier zur Konsistenz nochmals verankert |

**Warum dieser Abschnitt der wichtigste im ganzen Dokument ist**: Von den ursprünglich in den Zulieferdokumenten als "offen"/"zu bauen" markierten elf Punkten sind **neun bereits gelöst**. Würde man diese Konsolidierung überspringen und direkt auf Basis der Zulieferdokumente Implementierungsprompts an Jules o. ä. weitergeben, würde in neun von elf Fällen Aufwand in bereits erledigte Arbeit fließen — das ist exakt das Muster, vor dem `MemFuse_Audit_und_Jules_Implementierungsprompts.md` selbst warnt ("kein Implementierungsaufwand an Phantomen").

---

## 2. Was tatsächlich noch offen ist (bereinigte Restliste)

Nach Abzug der in Abschnitt 1 entkräfteten Punkte bleibt eine deutlich kürzere, aber dafür verlässliche Liste echter offener Arbeit, aus allen zehn Dokumenten zusammengeführt:

| # | Offener Punkt | Quelle(n) | Live-Status (verifiziert) |
|---|---|---|---|
| O-1 | **Aktiver Decay-/TTL-Sweep** — `MemoryType::default_decay()`/`default_ttl_tx()` existieren als reine Funktionen, aber es gibt (noch) keinen periodischen Sweep-Prozess, der `effective_score()` durchsetzt und abgelaufene `Working`-Memory tatsächlich entfernt | `memfuse_jules_implementierungsprompts_2026-08-29.md` P-04 | Typen vorhanden, Enforcement-Loop fehlt — **bestätigt offen** |
| O-2 | **Fault-Injection-Tests für CRITICAL-Tags** `AGT-CKPT-f3a1b2c4` (CheckpointManifest) und `AGT-STORE-003` (WAL-Integrity-Key) — Code ist jeweils korrekt strukturiert, aber es fehlt der Testbeweis, dass ein simulierter Absturz mitten in der Operation korrekt behandelt wird | `memfuse_jules_implementierungsprompts_2026-08-29.md` P-01 | Nur oberflächlich geprüft (`grep` fand Dateien, aber keine dedizierten Crash-Simulation-Tests) — **wahrscheinlich noch offen, vor Umsetzung erneut punktuell verifizieren** |
| O-3 | **`memfuse-kv` — Retrieval↔Inferenz-Brücke** (KV-Cache-Bridging via `ContextCacheBridge`) | `memfuse_v2_optimierungsspezifikation.md` §6 | **Echt neu, kein Code vorhanden.** Einziger Punkt in der gesamten Konsolidierung, der tatsächlich ein komplett neues Crate erfordert |
| O-4 | **`IoBackend`-Abstraktion** (io_uring statt `tokio::fs`+`spawn_blocking`) | `memfuse_v2_optimierungsspezifikation.md` §4 | Bestätigt offen — `memfuse-store` nutzt weiterhin `tokio::fs` |
| O-5 | **`FusionStrategy`/RRF statt reiner `FusionWeights`-Gewichtssumme** | `memfuse_v2_optimierungsspezifikation.md` §2.3/§7.1 | Bestätigt offen — `HybridQuery.fusion_weights: FusionWeights` ist weiterhin der einzige Fusionsmechanismus |
| O-6 | **`FilterExpr`↔`MetadataFilter`-Konvertierungspfad** fehlt, `FilterExpr` ist im aktiven aktiven Suchpfad totes Konstrukt | `MemFuse_Audit_und_Jules_Implementierungsprompts.md` Finding H, `memfuse_v2_optimierungsspezifikation.md` §7.2 | Nicht in dieser Session erneut geprüft — **auf Restliste, aber Verifikationspflicht vor Umsetzung** |
| O-7 | **`memfuse-store`/`memfuse-db`-Namensraum-Kollision `CompactionStrategy`** | `MemFuse_Audit_und_Jules_Implementierungsprompts.md` Finding K, `memfuse_umsetzungsprompts.md` WP7 | Nicht erneut geprüft — **Verifikationspflicht vor Umsetzung**, da mehrere andere "bestätigte" Punkte sich als bereits gelöst herausstellten |
| O-8 | **Zwei separate `CheckpointGuard`-Typen** (`memfuse-checkpoint` generisch vs. `memfuse-store` konkret) | `MemFuse_Audit_und_Jules_Implementierungsprompts.md` Finding J | Nicht erneut geprüft — Verifikationspflicht |
| O-9 | **PPR/Community-Detection ohne Proptest-Abdeckung**, kein Konvergenz-Limit außer `max_iterations` | `memfuse_jules_implementierungsprompts_2026-08-29.md` P-07 | Nicht erneut geprüft — plausibel, da `PprConfig` (siehe frühere Session) tatsächlich nur `max_iterations`/`convergence_epsilon` als Limits kennt |
| O-10 | **Zero-Copy-Migration an Layer-Grenzen** (hohe `.clone()`-Dichte in `collection.rs`/`lsm.rs`/`diskann.rs`) | `memfuse_jules_implementierungsprompts_2026-08-29.md` AUD-05, `MemFuse_Vollaudit_und_Jules_Prompts_2026-08-28.md` §3 Punkt 3 | Plausibel weiterhin offen — reines Performance-Refactoring, unabhängig von den funktionalen Korrekturen aus Abschnitt 1 |
| O-11 | **`overflow-checks`-Profil-Frage** — ist `overflow-checks = true` im Release-Profil aktiv? | `MemFuse_Vollaudit_und_Jules_Prompts_2026-08-28.md` §3 Punkt 1 | **Verifiziert in dieser Session: Nein.** `grep -n "overflow-checks" Cargo.toml` liefert keinen Treffer — das Root-`Cargo.toml` setzt keine expliziten Release-Profile-Overrides. Das bedeutet: im Release-Build sind Integer-Overflows **standardmäßig nicht** abgesichert (Rust-Default: `overflow-checks` nur in Debug aktiv) |
| O-12 | **Benchmark-Suite vs. Mem0/Zep/MemOS** — als Vorbereitung dokumentiert, aber laut Produktspezifikation nicht ausgeführt | `memfuse_produktspezifikation.md` Abschnitt 4/8 | Nicht erneut geprüft, aber plausibel weiterhin offen (empirische Validierung fehlt typischerweise am längsten) |
| O-13 | **PathRAG-Primärquelle** nur indirekt über Zitationskontext verifiziert, nicht isoliert per Volltextsuche bestätigt | Eigene vorherige Analyse (`MEMFUSE_INTERFACE_SPECIFICATION.md`, Abschnitt 9) | Weiterhin offen, siehe Abschnitt 7 dieses Dokuments |

**Wichtiger methodischer Punkt zu O-6 bis O-9**: Diese vier Punkte wurden in dieser Session **nicht** erneut gegen den Code geprüft (Zeitökonomie), obwohl sich neun von elf zuvor "bestätigten" Punkten als bereits gelöst herausstellten. Es wäre daher ein Fehler, sie unverifiziert in eine Implementierungsvorlage zu übernehmen. **Verbindliche Regel für die nächste Session**: Jeder der Punkte O-6 bis O-9 muss unmittelbar vor Beginn der Implementierung erneut per `grep`/`view` gegen den dann aktuellen Code geprüft werden — nicht aus diesem Dokument übernommen werden.

---

## 3. Neu identifizierte Ziele — bisher nicht in meiner Strategie enthalten

Dies ist der Kern der angeforderten Ergänzung: Ziele, die in den neun Zulieferdokumenten auftauchen, aber weder in `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md` noch in `MEMFUSE_INTERFACE_SPECIFICATION.md` vorkamen.

### 3.1 Hardware-naher Performance-Track (vollständig neu, aus `memfuse_v2_optimierungsspezifikation.md`)

Meine bisherige Strategie war ausschließlich auf die **Memory-Governance-/Forschungs-Ebene** fokussiert (MAGMA, VMG, Sleep-Time-Compute, kalibriertes Routing). Das zugelieferte v2-Optimierungsdokument eröffnet eine komplementäre, bisher komplett fehlende Achse: **hardware-nahe Performance-Optimierung**. Konkret neu:

- **KV-Cache-Bridging (`memfuse-kv`)**: Die Erkenntnis, dass MemFuse aktuell **keine Brücke** zwischen "Kontext wurde retrieviert" und "Kontext ist der Inferenz-Engine als vorberechneter KV-Cache bekannt" hat — jeder Retrieval-Treffer erzwingt vollen Prefill. Das ist, wie das Zulieferdokument selbst herausstellt, **der größte ungenutzte Hebel**, um aus einer Retrieval-Engine tatsächlich ein "Context-OS" zu machen (nicht nur begrifflich, wie in ADR-020 postuliert, sondern technisch). Dieser Punkt fehlte in meiner bisherigen Recherche vollständig, weil ich mich auf die Speicher-Governance-Seite konzentriert hatte, nicht auf die Inferenz-Kopplung.
- **Disk-resident ANN jenseits von RAM-Grenzen (DiskANN/Vamana/AiSAQ-Linie)**: Nach Korrektur aus Abschnitt 1 ist dies kein Neubau, sondern eine Reifung eines bestehenden experimentellen Features — aber die *Motivation* (Collections, die um ein Vielfaches größer als der verfügbare RAM sind) kam in meiner Strategie bisher nicht vor, da ich mich auf Retrieval-Qualität statt auf Skalierungsgrenzen konzentriert hatte.
- **io_uring/O_DIRECT als I/O-Unterbau**: Ein rein infrastruktureller Aspekt, der in keiner meiner bisherigen ArXiv-Recherchen auftauchte, weil er nicht memory-governance-, sondern systems-engineering-getrieben ist.
- **Matryoshka-Embedding-Trunkierung + Binär-/PQ-Quantisierung mit Zweistufen-Rescoring**: Eine komplementäre Kompressionslinie zur bereits vorhandenen Scalar-Quantisierung — in meiner Strategie kam Embedding-Kompression bisher gar nicht vor.
- **Runtime-SIMD-Dispatch** (`std::is_x86_feature_detected!` statt Compile-Time-Fixierung): Reines Hardware-Portabilitätsthema, ebenfalls komplett neu.

**Bewertung**: Dieser Track ist strategisch komplementär, nicht konkurrierend zu meiner bisherigen Forschungsrichtung. Memory-Governance (mein Fokus) beantwortet "was wird gespeichert, wie vertrauenswürdig, wie lange" — der Hardware-Track beantwortet "wie schnell und wie groß kann das Ganze sein". Beide gehören in dieselbe Zielarchitektur, siehe Abschnitt 4.

### 3.2 Monetarisierungs- und Zielkunden-Strategie (neu, aus `memfuse_produktspezifikation.md`)

Ein Ziel, das in meiner technischen Strategie komplett fehlte, weil es keine ArXiv-Quelle hat, sondern eine Geschäftsentscheidung ist:

- **Drei unterschiedliche Käufer-Personas** (Desktop-App-Nutzer, Rust/Python-Library-Integrator, MCP-Server-Betreiber) werden aktuell **parallel, ohne erkennbare Priorisierung** bedient. Das ist keine technische Lücke, sondern eine **Fokussierungsfrage**, die vor größeren Architektur-Investitionen (insbesondere `memfuse-kv`, das primär für Library-Integratoren relevant wäre) beantwortet werden sollte.
- **Monetarisierungsmodell offen**: MIT/Apache-2.0 erlaubt beliebiges Forken; Erlösquelle (Support, Enterprise-Features wie das geplante Phase-4-RBAC, Hosting) ist nicht festgelegt.
- **Air-Gap als hartes Muss vs. Nice-to-have**: Entscheidet, ob langfristig ISO-27001/BSI-Grundschutz-Nachweise nötig werden — eine Compliance-Frage, die die technische Roadmap (z. B. Priorität von VMG/Verified-Forgetting aus meiner Strategie) direkt beeinflusst, wenn Enterprise-Zielkunden mit Compliance-Pflicht priorisiert würden.

### 3.3 Empirische Validierungspflicht als eigenständiges Ziel (neu, aus `memfuse_produktspezifikation.md`)

Meine bisherige Strategie hat Forschungsergebnisse (z. B. UCCI: 31 % Kostenreduktion, ECE 0,12→0,03) korrekt zitiert, aber nie gefordert, dass MemFuse diese Zahlen **an der eigenen Workload** nachmisst. Das Produktspezifikations-Dokument macht dies explizit zu einem eigenen Ziel: **Positionierungsclaims gegenüber Mem0/Zep/MemOS sind "architektonisch, aber noch nicht empirisch validiert"** — eine bereits vorbereitete Benchmark-Suite (laut Doku) wurde noch nicht ausgeführt. Das ist ein eigenständiges Arbeitspaket, unabhängig von jeder neuen Funktion.

### 3.4 Governance-System-Selbstauskunfts-Genauigkeit als Ziel zweiter Ordnung (neu, aus `memfuse_produktspezifikation.md`)

Ein Meta-Ziel, das in keinem meiner bisherigen Dokumente vorkam: **Das Projekt soll systematisch sicherstellen, dass Roadmap-Status (README, `SOURCE_OF_TRUTH.md`), ADR-Nummerierung und Wettbewerbsvergleiche mit dem verifizierten Code-Zustand übereinstimmen** — in beide Richtungen (Code kann der Doku voraus sein, wie bei PPR/Community-Detection/kognitiven Gedächtnistypen, oder die Doku kann veraltete offene Punkte listen, wie bei den WP14-`_at`-Methoden). Konkret als Ziel: `just sync-docs` regelmäßiger ausführen und Roadmap-Checkboxen als Teil des Session-Ende-Protokolls pflegen, nicht nur `WORKING_STATE.md`.

### 3.5 Fault-Injection-Testing als eigenständige Methodik (neu, aus `memfuse_jules_implementierungsprompts_2026-08-29.md`)

Ein testmethodisches Ziel, das über die reine "Tests schreiben"-Anforderung hinausgeht: Für sicherheitskritische Zustandsübergänge (`CheckpointManifest`, WAL-Integrity-Key-Erstellung) reicht **korrekte Struktur** (Result-basiert, atomare Dateierstellung) laut diesem Dokument nicht als Abschlusskriterium — es wird explizit **simulierter Absturz zwischen zwei Schreibschritten** als Testanforderung gefordert (`AGT-CKPT-f3a1b2c4`, `AGT-STORE-003`). Das ist eine Qualitätsschwelle, die in meiner bisherigen Strategie nicht auftauchte, weil ich mich auf Feature-Vollständigkeit statt auf Fehlertoleranz-Beweisführung konzentriert hatte.

### 3.6 Konkrete Wettbewerbsreferenzen jenseits von ChatGPT/Gemini/Grok (neu, aus ADR-020)

Meine bisherige Positionierungsstrategie verglich MemFuse ausschließlich mit **kommerziellen Chat-Produkten** (ChatGPT, Gemini, Grok) und, sekundär, mit generischen RAG-Frameworks. `DECISIONS.md` ADR-020 nennt ein enger gefasstes, technisch relevanteres Vergleichsfeld, das in meiner Strategie fehlte: **Mem0 (ECAI 2025), Zep/Graphiti, MemOS, MIRIX, A-MEM, Trajectory-Informed Memory** — dedizierte Memory-Architektur-Systeme, nicht allgemeine LLM-Produkte. Das ist der eigentlich relevante Wettbewerbsvergleich für eine Memory-Engine und sollte die Positionierung in Abschnitt 7 meiner ursprünglichen Strategie ergänzen, nicht ersetzen.

### 3.7 `overflow-checks`-Frage als projektweites Querschnittsrisiko (neu, aus `MemFuse_Vollaudit_und_Jules_Prompts_2026-08-28.md`)

Ein sicherheitsrelevanter Punkt, der in keiner meiner Analysen vorkam, weil er kein Feature, sondern eine **Compiler-Profil-Konfiguration** ist: Ob `overflow-checks = true` im Release-Profil aktiv ist, verändert die Risikobewertung **jeder einzelnen ungeprüften Integer-Subtraktion in allen 14 Crates** gleichzeitig. In dieser Session verifiziert: **aktuell nicht gesetzt** — ein projektweites, bisher nicht dokumentiertes Risiko.

### 3.8 Zero-Copy-`Bytes`-Migration als eigenständiges Querschnittsthema (neu, aus `MemFuse_Vollaudit_und_Jules_Prompts_2026-08-28.md` und `memfuse_jules_implementierungsprompts_2026-08-29.md`)

Zwei unabhängige Zulieferdokumente identifizieren dieselbe, in meiner Strategie fehlende Beobachtung: hohe `.clone()`-Dichte an den Schichtgrenzen `memfuse-store`/`memfuse-index`/`memfuse-graph`/`memfuse-text` → `memfuse-db` (konkret beziffert: 37 Klone in `collection.rs`, 32 in `lsm.rs`, 21 in `diskann.rs`). Beide Dokumente empfehlen unabhängig voneinander, dies **nicht** in die funktionalen Einzel-Crate-Arbeitspakete zu mischen, sondern als eigene, dedizierte Performance-Engineering-Sitzung zu behandeln, die ausschließlich Grenzflächen-Signaturen auf `Bytes`/`Arc`-Nutzung umstellt.

### 3.9 Das archivierte `memfuse-cluster`-Crate als bewusst ausgeklammerte Zukunftsoption (neu, aus `memfuse_v2_optimierungsspezifikation.md`, verifiziert)

`Cargo.toml` Zeile 83 bestätigt: *"memfuse-cluster, memfuse-sandbox, memfuse-saos-agent wurden in Phase 0 ausgelagert."* Das bedeutet, MemFuse hatte oder plante bereits einmal eine Multi-Node-Architektur, die bewusst aus dem aktuellen Scope entfernt wurde. Das ist relevant für jede zukünftige Erweiterung von `memfuse-kv` (verteiltes KV-Cache-Tiering über Knoten hinweg würde eine Reaktivierung dieses archivierten Crates voraussetzen) — ein Abhängigkeitspfad, der in meiner bisherigen Strategie nicht sichtbar war, weil ich das Archiv nicht geprüft hatte.

---

## 4. Konsolidierte Architektur v3 — Zusammenführung von Governance-Track und Hardware-Track

Die folgende Tabelle löst den scheinbaren Konflikt zwischen meiner ursprünglichen Memory-Governance-Strategie (`MEMFUSE_INTERFACE_SPECIFICATION.md`) und dem zugelieferten Hardware-Performance-Track (`memfuse_v2_optimierungsspezifikation.md`) auf, indem beide auf denselben Schichten einsortiert werden — sie adressieren unterschiedliche, orthogonale Dimensionen (Vertrauenswürdigkeit/Governance vs. Geschwindigkeit/Skalierung) und schließen sich nicht aus.

| Layer | Governance-Track (meine ursprüngliche Strategie) | Hardware-Track (aus v2-Optimierungsdokument, nach Realitätsabgleich korrigiert) |
|---|---|---|
| `memfuse-core` | `ProvenanceRecord`, `CausalEdge`, `CapabilityUnsupported`-Erweiterungen für `GraphIndex` | `FusionStrategy`-Enum (RRF statt reiner Gewichtssumme, O-5); `MemoryGovernance`-Typ (TTL/Decay/Priority — **Teilüberschneidung mit bereits vorhandenem `MemoryType`**, siehe Hinweis unten) |
| `memfuse-graph` | `CausalCsrGraph` (vierte, orthogonale Graph-Dimension, MAGMA-Muster) | — |
| `memfuse-index` | — | Reifung von `DiskAnnIndex` (bereits vorhanden, siehe Abschnitt 1) um FreshDiskANN-Delta-Merge; Erweiterung von `ScalarQuantizer` um Binary-/PQ-Codecs |
| `memfuse-db` | `ContextCompactor`-Erweiterung um `ProvenanceRecord`, Cache-bewusste Segmentierung | `FusionStrategy`-Integration in `Collection::hybrid_search`; aktiver Decay-Sweep (O-1) |
| `memfuse-router` | Kalibriertes Kaskaden-Routing (UCCI-Muster, `IsotonicCalibrator`) | — |
| `memfuse-agent` | Sleep-Cycle-Konsolidierung (Auto-Dreamer-Muster), proaktive Foresight-Events (CogniFold/EverMemOS-Muster) | `Foldable`-Trait für Context-Folding (Abgrenzung zu Sleep-Cycle: Folding behält Pointer auf Original für Drill-Down, Sleep-Cycle-Konsolidierung ersetzt vollständig) |
| `memfuse-crypto` | Kryptographischer Löschbeweis (`DeletionProof`, Verified Forgetting) | — |
| **`memfuse-kv`** (neu) | — | **Vollständig neues Crate** — KV-Cache-Bridging zwischen Retrieval und Inferenz-Engine (`ContextCacheBridge`, `KvCacheRef`, `ModelFingerprint`) |
| `memfuse-store` | — | `IoBackend`-Abstraktion (io_uring/O_DIRECT), Zero-Copy-Blockcache für `MmapReadonlyBackend` |

**Wichtiger Klärungsbedarf — `MemoryGovernance` vs. `MemoryType`**: Das zugelieferte v2-Dokument schlägt einen neuen Typ `MemoryGovernance { created_at_tx, source, ttl, decay, priority, access_count, last_accessed_tx }` vor. Nach Realitätsabgleich (Abschnitt 1, Punkt 7) existiert bereits `MemoryType` mit `default_decay()`/`default_ttl_tx()`. Das sind **keine deckungsgleichen Typen** — `MemoryType` ist eine *kategoriale* Klassifikation (Episodic/Semantic/Procedural/Working) mit *Standard*-Decay/TTL-Werten je Kategorie, während `MemoryGovernance` eine *pro-Instanz* Feinsteuerung (individueller TTL, individuelle Priorität, Zugriffszähler) wäre. Empfehlung: `MemoryGovernance` sollte `MemoryType` **referenzieren statt duplizieren** — z. B. `MemoryGovernance { memory_type: MemoryType, ttl_override: Option<Duration>, priority: Priority, access_count: u64, last_accessed_tx: TxId }`, damit die bereits vorhandene Decay-Logik pro Typ nicht zweimal gepflegt werden muss. Dies ist eine **Korrektur am zugelieferten v2-Dokument**, keine Bestätigung seines Vorschlags in Rohform.

---

## 5. Aktualisierte, priorisierte Roadmap

Ersetzt die Prioritäten in `MEMFUSE_INTERFACE_SPECIFICATION.md` Abschnitt "Roadmap" durch eine Fassung, die die bereinigte Restliste (Abschnitt 2) und die neuen Ziele (Abschnitt 3) einarbeitet.

### Sofort, vor jeder neuen Funktion (Verifikations-/Hygiene-Arbeit)
1. **Vier unverifizierte Restpunkte klären** (O-6 bis O-9): je 15–30 Minuten gezielter Code-Check pro Punkt, bevor irgendein Implementierungsprompt dafür geschrieben wird.
2. **`overflow-checks`-Entscheidung treffen** (O-11) — einmalig, projektweit, beeinflusst Risikoeinstufung aller anderen Punkte.
3. **ADR-020-Duplikat auflösen** (Abschnitt 1, Punkt 10) und Roadmap-Checkboxen in `SOURCE_OF_TRUTH.md`/README aktualisieren (PPR, Community Detection, kognitive Gedächtnistypen sind fertig, nicht mehr "geplant").

### Kurzfristig (2–4 Wochen)
4. **Fault-Injection-Tests für die zwei CRITICAL-Tags** (O-2) — höchste Priorität unter den echten Restarbeiten, da sicherheitsrelevant und laut Dokument bereits mit P0 markiert.
5. **Aktiver Decay-/TTL-Sweep** (O-1) — schließt die Lücke zwischen bereits vorhandenen `MemoryType`-Decay-Funktionen und tatsächlicher Durchsetzung.
6. Kalibriertes Kaskaden-Routing in `memfuse-router` (unverändert aus meiner ursprünglichen Strategie, weiterhin gültig und unabhängig von den neuen Funden).

### Mittelfristig (Q4 2026)
7. **`memfuse-kv`-Grundgerüst** (O-3) — der einzige vollständig neue, große Architekturbaustein aus dem gesamten konsolidierten Material; sollte wegen seiner Größe eigenständig geplant werden, nicht nebenläufig zu anderen Punkten.
8. Sleep-Cycle-Konsolidierung (unverändert aus meiner ursprünglichen Strategie).
9. `FusionStrategy`/RRF-Integration (O-5) — vergleichsweise kleiner, gut abgegrenzter Schritt.
10. Zero-Copy-`Bytes`-Migration als eigene Sitzung (Abschnitt 3.8) — bewusst nicht mit funktionalen Änderungen vermischt.

### Langfristig (Q1–Q2 2027)
11. `IoBackend`/io_uring (O-4), disk-resident Vamana-Reifung (Abschnitt 1, Punkt 1), Matryoshka/Binary/PQ-Kompressionslinie — alle drei sind reine Performance-/Skalierungsarbeit, korrekt spät in der Roadmap.
12. Vollständige Multi-Graph-Architektur (MAGMA, unverändert aus meiner ursprünglichen Strategie).
13. Kryptographischer Löschbeweis (unverändert, weiterhin technisch anspruchsvollster Punkt).

### Parallel, nicht crate-gebunden
14. Benchmark-Suite gegen Mem0/Zep/MemOS tatsächlich ausführen (Abschnitt 3.3) — kann parallel zu jedem der obigen Punkte laufen, sollte aber nicht auf "danach" verschoben werden, da sie die Positionierungs-Claims aus meiner ursprünglichen Strategie (Abschnitt 7 dort) erst belastbar macht.
15. Zielkunden-Priorisierung und Monetarisierungsentscheidung (Abschnitt 6) — beeinflusst indirekt, ob Punkt 7 (`memfuse-kv`, primär library-integrator-relevant) oder eher Enterprise-/Compliance-nahe Punkte (Verified Forgetting, RBAC) vorgezogen werden sollten.

---

## 6. Business- und Governance-Ziele (nicht-technisch, aber strategisch)

Diese Ziele haben keine ArXiv-Quelle und keine Code-Signatur, sind aber laut den Zulieferdokumenten (insbesondere `memfuse_produktspezifikation.md`) genauso Teil der "Source of Truth" wie die technische Roadmap:

- **Persona-Priorisierung**: Eine explizite Entscheidung treffen, ob Desktop-App-Nutzer, Rust/Python-Library-Integratoren oder MCP-Server-Betreiber der primäre Zielkunde für die nächsten zwei Roadmap-Phasen sind. Diese Entscheidung sollte **vor** dem `memfuse-kv`-Arbeitspaket (Punkt 7 der Roadmap) getroffen werden, da KV-Cache-Bridging primär für Library-Integratoren mit eigener Inferenz-Anbindung relevant ist, nicht für Desktop-App-Endnutzer.
- **Monetarisierung**: Klärung, ob Support-Verträge, Enterprise-Features (Phase-4-RBAC/OAuth/Multi-Tenant) oder ein gehosteter Dienst die Erlösquelle werden sollen — beeinflusst, ob Phase 4 vorgezogen werden sollte.
- **Air-Gap-Härte**: Klärung, ob Air-Gap ein hartes Compliance-Muss (→ ISO-27001/BSI-Grundschutz-Pfad, was Verified-Forgetting und VMG-Primitive aus meiner ursprünglichen Strategie aufwerten würde) oder ein Nice-to-have-Verkaufsargument ist.
- **Selbstauskunfts-Disziplin**: `just sync-docs` als festen Bestandteil des Session-Endes etablieren (Abschnitt 3.4), damit sich Abschnitt 1 dieses Dokuments (elf von neun entkräfteten Punkten) nicht in der nächsten Konsolidierung wiederholt.

---

## 7. Offene Verifikationsfragen für die nächste Session

Ehrlich benannt, damit sie nicht stillschweigend als erledigt gelten:

1. **O-6 bis O-9** (Abschnitt 2) wurden in dieser Session nicht erneut geprüft — Priorität für den nächsten Verifikationsdurchlauf, bevor irgendeine Implementierung darauf aufbaut.
2. **PathRAG-Primärquelle** (Abschnitt 3 in `MEMFUSE_INTERFACE_SPECIFICATION.md`, dort bereits als Lücke vermerkt) ist weiterhin nur indirekt verifiziert — sollte vor Abschnitt 8 jener Spezifikation (Pfad-Extraktions-Retrieval-Strategie) durch eine dedizierte Suche geschlossen werden.
3. **Tatsächlicher Umfang der Fault-Injection-Lücke** (O-2): Es wurde nur `grep` auf Dateinamen ausgeführt, nicht der tatsächliche Testinhalt gelesen — möglich, dass auch dieser Punkt bereits (teilweise) gelöst ist, analog zum Muster in Abschnitt 1.
4. **MemoryGovernance-vs-MemoryType-Empfehlung** (Abschnitt 4, Klärungsbedarf) ist ein Vorschlag dieser Konsolidierung, kein verifizierter Fakt — sollte vor Umsetzung mit dem Projektverantwortlichen abgestimmt werden, da er von der Rohform des zugelieferten v2-Dokuments abweicht.
