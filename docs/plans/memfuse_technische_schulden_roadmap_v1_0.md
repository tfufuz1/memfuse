# MemFuse — Technische Schulden, Temporäre Bugs & Vision-Roadmap v1.0

> **Dokument-Typ:** Ergänzendes Arbeitsdokument zur „Konsolidierten Gesamtspezifikation v7.0" — normatives Muster für alle künftigen Entwicklungsentscheidungen.
> **Stand:** 07. September 2026 · **HEAD:** `738c0ace`
> **Methodik:** Teil A listet **ausschließlich live-code-verifizierte** offene Schulden (jeder Eintrag mit Datei/Zeile-Beleg aus diesem Audit-Durchlauf). Teil B ist eine **Vision-Roadmap** — konkrete, umsetzbare Weiterentwicklungen, die entweder (a) bereits im Code als Lücke/Hook angelegt sind, oder (b) sich direkt aus den in Teil A dokumentierten Schulden als nächster Schritt ergeben. Jeder Roadmap-Punkt benennt den erwarteten Mehrwert (Korrektheit, Effizienz oder Latenz) explizit, wie von der Aufgabenstellung gefordert.

---

## Teil A — Verifizierte offene technische Schulden (Stand `738c0ace`)

Format je Eintrag: **ID · Fundort · Befund · Risiko · Aufwand · Priorität**

### A.1 — F-09-Feature-Flag fehlt in `Cargo.toml` (K11)

- **Fundort:** `crates/memfuse-db/Cargo.toml` (`[features]`-Block, Zeilen ~40-42), referenziert von `crates/memfuse-db/src/fusion.rs:41,576,1154,1209,1255,1290`.
- **Befund:** `fusion.rs` implementiert den Resonanz-Kohärenz-Bonus (F-09) vollständig hinter `#[cfg(feature = "physio-resonance-fusion")]`. Diese Feature-ID ist in **keinem** `[features]`-Eintrag von `memfuse-db` deklariert. Damit ist der Code in jeder denkbaren Cargo-Feature-Kombination unerreichbar — **totes Code-Gewicht**, das kompiliert, getestet und gepflegt wird, ohne je in Produktion zu laufen.
- **Risiko:** Kein Sicherheits-, aber ein P7-Governance-Risiko (Marketing-/Statusaussagen zu F-09 wären falsch) und ein Wartungsrisiko (Bit-Rot in unerreichbarem Code).
- **Aufwand:** 5 Minuten (eine Zeile `physio-resonance-fusion = []` ergänzen) + CI-Lauf zur Verifikation, dass der Code hinter dem Flag tatsächlich kompiliert.
- **Priorität:** P0.

### A.2 — TenantId-Konstruktor umgeht Sicherheitsinvariante INV-TENANT-1 (K12)

- **Fundort:** `crates/memfuse-core/src/types/domain.rs:70-108`.
- **Befund:** `TenantId::new(id: u64) -> Self` ist eine ungeschützte `const fn`, die `id=0` klaglos akzeptiert. `impl From<u64> for TenantId` (Zeile 105-108) delegiert ebenfalls direkt auf `Self(id)` ohne den in `try_new()` implementierten Guard. Damit existieren zwei produktiv nutzbare Konstruktionspfade, die die dokumentierte Invariante „`TenantId(0)` ist ausschließlich `SYSTEM`" umgehen können — jeder Aufrufer, der `TenantId::new(0)` oder `TenantId::from(0u64)` verwendet, erzeugt einen semantisch mehrdeutigen Mandanten, der von `SYSTEM` nicht unterscheidbar ist.
- **Risiko:** **Sicherheitsrelevant.** In einem System mit kryptographischer Mandantenisolation (VETO-F10, DeletionProof, KV-Bridge-Tenant-Isolation) ist ein stiller Bypass-Pfad für die Tenant-ID-Konstruktion ein struktureller Schwachpunkt — auch wenn aktuell kein bekannter Aufrufer ihn missbraucht, senkt seine bloße Existenz die Beweisbarkeit von INV-TENANT-1 auf „Konvention" statt „Typsystem-erzwungen".
- **Aufwand:** ~1 Stunde für den Kern-Fix (Deprecation-Attribute, Guard in `From<u64>`), zzgl. unbekannter, aber vermutlich geringer Aufwand für Migration bestehender Aufrufer (Umfang nicht in diesem Audit ermittelt — Empfehlung: `cargo build` nach Fix zeigt alle betroffenen Call-Sites als Deprecation-Warnungen).
- **Priorität:** P1.

### A.3 — KV-Cache-Eviction ist FIFO, nicht LRU (K16)

- **Fundort:** `crates/memfuse-kv-bridge/src/eviction_worker.rs:47-56`.
- **Befund:** `let evicted = segs.remove(0); // LRU an Index 0 angenommen` — der Kommentar benennt die Annahme selbst als unbewiesen. Es existiert kein Zugriffszeitstempel-Feld in `KvSegment` oder im umgebenden Store, das eine echte Least-Recently-Used-Reihenfolge ermöglichen würde. Tatsächlich implementiert ist First-In-First-Out (ältestes Segment zuerst), unabhängig davon, wie oft ein Segment seither gelesen wurde.
- **Risiko:** Funktional: bei ungleichmäßigem Zugriffsmuster (z. B. ein selten aber wiederholt genutztes „Hot"-Segment, das früh eingefügt wurde) wird dieses Segment fälschlich vor tatsächlich kalten, später eingefügten Segmenten evictet — Cache-Miss-Rate steigt gegenüber echtem LRU.
- **Aufwand:** Mittel. Zwei Implementierungsoptionen: (1) `last_accessed: AtomicU64`-Feld + periodischer Scan (einfach, etwas Overhead pro Zugriff), (2) intrusive doubly-linked list mit O(1)-Move-to-Front bei Zugriff (klassisches LRU, mehr Implementierungsaufwand, aber O(1) pro Operation statt O(n)-Scan).
- **Priorität:** P2 — funktional korrekt (kein Crash/Datenverlust), aber Effizienzlücke mit Production-Release-Sperre laut v6.0-Beschluss.

### A.4 — PhysioScheduler konsolidiert die Reaper-Pfade nicht vollständig (K17-Rest)

- **Fundort:** `crates/memfuse-db/src/physio_scheduler.rs` (neu, Grundgerüst) vs. `crates/memfuse-db/src/reaper.rs:28` (`start_nrem_reaper`) und `:161` (`start_thermostat_reaper`), beide weiterhin über `lib.rs:95` direkt exportiert.
- **Befund:** Seit v6.0 wurde `physio_scheduler.rs` neu angelegt — ein Fortschritt. Es enthält jedoch bislang nur einen Kommentar-Hook für die F-03-Integration (`Zeile 137-138`) und übernimmt noch nicht die Orchestrierung der bestehenden Thermostat- und NREM-Reaper-Tasks. Diese laufen weiterhin als unabhängige, separat gestartete Hintergrund-Tasks ohne gemeinsamen Scheduler-Takt.
- **Risiko:** Zwei unkoordinierte periodische Hintergrund-Tasks, die potenziell auf überlappende Ressourcen (Storage-Engine, Vector-Index) zugreifen, ohne dass ein gemeinsamer Scheduler Reihenfolge oder gegenseitigen Ausschluss garantiert. Aktuell kein bekannter Incident, aber architektonisch die Wurzel des ursprünglichen K17-Befunds.
- **Aufwand:** Mittel-Hoch (Migration zweier produktiver Hintergrundpfade ohne Downtime/Regressionen erfordert sorgfältige Tests).
- **Priorität:** P1.

### A.5 — KV-Bridge Increment 2 (Verschlüsselung) ohne Fortschritt seit v6.0 (K14)

- **Fundort:** `crates/memfuse-kv-bridge/src/segment.rs:11-18`; kein `kv_cipher.rs` in `crates/memfuse-crypto/src/` auffindbar.
- **Befund:** `KvSegment` enthält weiterhin nur `tenant_id`, `segment_id`, `data: Vec<u8>`. Keine AES-256-GCM-SIV-Verschlüsselung, kein `ModelFingerprint`, kein `rope_offset`. Der in v6.0 als „Increment 2" geplante Schritt wurde in diesem Zyklus nicht begonnen.
- **Risiko:** P9 („Kein Klartext-Sensitivspeicher") ist für KV-Cache-Segmente nur durch Zeroize-on-Drop, nicht durch Verschlüsselung im Ruhezustand abgedeckt. Solange KV-Segmente ausschließlich im Prozessspeicher verbleiben (kein Swap/Persistenz), ist das Risiko durch Zeroize begrenzt — sobald ein Auslagerungs- oder Persistenzpfad für KV-Segmente hinzukommt, wird die fehlende Verschlüsselung sicherheitskritisch.
- **Aufwand:** Hoch (neues Krypto-Modul, Schlüsselableitung via bestehenden `KeyManager`, Integration in Segment-Lebenszyklus, Tests gegen Timing-Seitenkanäle).
- **Priorität:** P2 (kein akutes Risiko im aktuellen Skeleton-Zustand, aber Blocker für jede Produktionsfreigabe mit Persistenz-Anspruch).

### A.6 — F-03-Fusionssignal nicht integriert (K18)

- **Fundort:** `crates/memfuse-graph/src/synaptic.rs` (Berechnungslogik vollständig: `SynapticConfig`, `apply_hebbian_update()`, `synaptic_score()`); Hook-Kommentar in `crates/memfuse-db/src/physio_scheduler.rs:137-138`.
- **Befund:** Die mathematische Berechnungslogik für synaptische Verstärkung (Hebbian Learning) ist vollständig implementiert und testbar, aber es existiert kein `SynapticUpdateBuffer`, der Scores sammelt und periodisch in den CSR-Graphen zurückschreibt (`flush_to_csr()`), und `fusion.rs` verwendet nur drei Signale (Vektor, BM25, Graph) statt der spezifizierten vier/fünf.
- **Risiko:** Kein Korrektheitsrisiko (Feature ist inaktiv), aber Retrieval-Qualitätslücke gegenüber der Zielarchitektur.
- **Aufwand:** Mittel — Buffer-Struktur + periodischer Flush-Task (idealerweise als Teil des unter A.4 zu konsolidierenden `PhysioScheduler`) + neues Fusionssignal in `fusion.rs` mit eigenem RRF-Gewicht.
- **Priorität:** H2 (Horizont 2, kein P0/P1).

### A.7 — Kein GASP/TPA-Halluzinations-Postvalidator (K19)

- **Fundort:** Workspace-weite Suche nach `gasp.rs` — kein Treffer.
- **Befund:** Der in der Produktvision genannte Post-Hoc-Halluzinations-Validator (GASP/TPA) existiert nicht als eigenständiges Modul. `memfuse-ollama::client.rs` enthält einen „präventiven Halluzinations-Guard", der jedoch architektonisch etwas anderes ist als ein Post-Hoc-Validator auf Basis der `memfuse-candle`-Inferenz-Pipeline.
- **Risiko:** Kein Regressionsrisiko (nie vorhanden gewesen), aber eine offene Produktlücke gegenüber der Vision.
- **Aufwand:** Hoch — abhängig von der noch ausstehenden `memfuse-candle`-Pipeline-Integration (Säule I, P3).
- **Priorität:** H3, blockiert durch A.9 (Candle-Pipeline-Integration).

### A.8 — PID-Regler `min_pool_size` deutlich unter arXiv-Empfehlung

- **Fundort:** `crates/memfuse-calibration/src/pid.rs:18,35,123`.
- **Befund:** `min_pool_size: usize` hat im Struct-Default `10` (Zeile 35) und in einem zweiten Preset `20` (Zeile 123). Die im Architect-Review referenzierte arXiv:2604.01733-Studie berichtet stabile Recall@5-Werte (0.888) erst ab einer Pool-Größe von 100 Rerank-Kandidaten. Kein ADR dokumentiert die Abweichung.
- **Risiko:** Mögliche Recall-Regression bei kleinen Kandidatenpools — nicht in diesem Audit empirisch nachgemessen (keine Zugriff auf `memfuse-bench`-Läufe im Rahmen dieses Durchlaufs), daher als **Verdachtsmoment, nicht als bewiesener Fehler** eingestuft.
- **Aufwand:** Gering für den Parameter-Change selbst, mittel für den erforderlichen empirischen Nachweis via LongMemEval/LoCoMo-Harness (bereits vorhanden in `memfuse-bench`).
- **Priorität:** P2 — vor jeder Aussage „Reranking ist produktionsreif kalibriert" zu klären.

### A.9 — `PENDING_FLUSH_THRESHOLD` ohne begleitenden ADR

- **Fundort:** `crates/memfuse-index/src/diskann.rs:37`.
- **Befund:** `const PENDING_FLUSH_THRESHOLD: u64 = 50;` — der Architect-Review v6 dokumentiert einen Sprung von einem früheren Wert 1.000 (v5.0) auf jetzt 50 (Faktor 20). Kein ADR unter `docs/decisions/` referenziert diese Änderung oder ihre Write-Amplification-Konsequenz für kleine Collections.
- **Risiko:** Unklare Auswirkung auf Schreib-Verstärkung (Write-Amplification) bei Collections mit wenigen Einfügungen pro Zeiteinheit — jeder 50. Insert löst einen Background-Persist aus statt jeden 1.000.
- **Aufwand:** Gering (Nachdokumentation als ADR) + mittel (Benchmark zur Quantifizierung der Write-Amplification bei verschiedenen Collection-Größen).
- **Priorität:** P2.

### A.10 — PathRAG `sufficiency_threshold` — Parametrisierung nicht isoliert verifizierbar

- **Fundort:** `crates/memfuse-graph/src/path_rag.rs`, durchgereicht via `crates/memfuse-db/src/collection/search.rs:573,854` und `query_builder.rs:74`.
- **Befund:** Der Architect-Review v6 dokumentiert einen Sprung von 0.6 (v5.0) auf 0.01 (v6.0) — Faktor 60 in derselben Metrik, ohne ADR. In diesem Audit-Durchlauf konnte der konkrete aktuell aktive Default-Wert nicht isoliert aus dem Preset-Aufrufgraphen extrahiert werden (abhängig von der jeweiligen `HybridQueryBuilder`-Konfiguration zur Laufzeit). **Diese Diskrepanz gilt daher als aus v6.0 unverändert übernommen, nicht als in diesem Zyklus neu bestätigt.**
- **Risiko:** Ein zu niedriger Schwellenwert lässt PathRAG-Pfade mit geringer Konfidenz ins Endergebnis — das im Architect-Review referenzierte MemGraphRAG-Precision-Problem (arXiv:2506.00610) könnte dadurch wieder auftreten.
- **Aufwand:** Gering (Wert-Lokalisierung + ADR), mittel (empirischer LongMemEval-Test vor endgültiger Festlegung).
- **Priorität:** P1 — sicherheitsunkritisch, aber direkt qualitätsrelevant für das Kernversprechen „belegbare Korrektheit" (Säule II).

### A.11 — `fsync`-Policy nicht konfigurierbar (Strict-only)

- **Fundort:** `crates/memfuse-store/src/wal.rs` (`sync_all()` an mehreren Commit-Pfaden, u. a. Zeilen 506, 803, 918, 1007, 1521), `crates/memfuse-store/src/util.rs:29` (Verzeichnis-`sync_all()`).
- **Befund:** Jeder WAL-Commit erzwingt einen synchronen `fsync` auf Datei- und Verzeichnis-Ebene. Es existiert kein `PhysioConfig`- oder `StorageConfig`-Parameter, der einen `Batched(n)`-Modus (z. B. Gruppen-Commit über n Einträge oder Zeitfenster) erlauben würde. Dies ist die vom Architect-Review v6 als „einzige offene Frage" bei sonst bestmöglicher WAL-Implementierung benannte Lücke.
- **Risiko:** Kein Korrektheitsrisiko (Strict ist die sicherste Policy), aber ungenutztes Latenz-Optimierungspotenzial für Deployments mit hoher Schreibfrequenz auf langsameren Datenträgern (HDD, Netzwerkspeicher) oder in Szenarien, die ein geringeres Crash-Fenster akzeptieren würden.
- **Aufwand:** Mittel-Hoch (neue Konfigurationsdimension, sorgfältige Chaos-Tests, da die bestehende `chaos_matrix.rs`/`chaos_power_cut.rs`-Suite für den Batched-Modus erweitert werden müsste, um die Crash-Konsistenz-Garantien nicht zu verwässern).
- **Priorität:** H2 — hoher potenzieller Nutzen, aber sicherheitskritischer Änderungspfad, daher nicht ohne ADR + erweiterte Chaos-Tests umzusetzen.

### A.12 — VETO-F02 Review-Frist läuft am 2026-10-07 ab

- **Fundort:** `VETOES.md`, Eintrag `VETO-F02`.
- **Befund:** Der `conditionally_accepted`-Status für den partiellen HNSW-Rebuild (F-02, `physio-nucleation`) benötigt bis zum 2026-10-07 einen 30-Tage-stabilen Recall@10-Regressionstest, sonst greift laut Governance-Modell implizit wieder der ursprüngliche restriktivere Zustand.
- **Risiko:** Kein Code-Risiko, sondern ein Prozess-Risiko — ohne aktive Beobachtung könnte die Frist unbemerkt verstreichen.
- **Aufwand:** Gering (Kalender-/CI-Erinnerung einrichten, `xtask check-vetoes` beobachten).
- **Priorität:** P1 (terminkritisch, nicht aufwandskritisch).

---

## Teil B — Vision-Roadmap: Umsetzbare Weiterentwicklungen mit Mehrwert

Dieser Teil folgt dem in der Gesamtspezifikation v7.0 etablierten Muster (P1–P12-Prinzipien, DAG-Disziplin, ADR-Pflicht bei sicherheits-/latenzrelevanten Änderungen) und schlägt **nur** Erweiterungen vor, die (a) technisch direkt an bestehende Code-Strukturen andocken und (b) einen der drei geforderten Vorteile — **Mehrwert, Effizienz oder Latenz** — konkret benennen.

### B.1 Konfigurierbare `fsync`-Policy (`Strict` / `Batched(n)` / `Timed(ms)`)

**Mehrwert-Typ:** Latenz. **Ansatzpunkt:** `crates/memfuse-store/src/wal.rs`, neuer `FsyncPolicy`-Enum in `memfuse-core::types`, verdrahtet über eine neue `PhysioConfig`- oder `StorageConfig`-Option.
**Vorschlag:** Drei Modi — `Strict` (heutiges Verhalten, Default, keine Verhaltensänderung ohne explizites Opt-in gemäß P12), `Batched(n)` (fsync erst nach n aufsummierten WAL-Einträgen, mit Recovery-Pfad, der die letzten ≤n Einträge als „möglicherweise nicht durable" markiert), `Timed(ms)` (fsync spätestens nach Zeitfenster, unabhängig von Eintragszahl — kombinierbar mit `Batched`). **Voraussetzung:** Erweiterung der bestehenden Chaos-Test-Suite (`chaos_matrix.rs`, `chaos_power_cut.rs`) um Szenarien, die gezielt einen Crash innerhalb eines offenen Batch-Fensters simulieren, um zu beweisen, dass höchstens die letzten n Einträge verloren gehen können und der WAL danach wieder konsistent ist. **Nächster Schritt:** ADR mit expliziter Nutzungsempfehlung (wann `Strict`, wann `Batched`) vor Implementierung.

### B.2 Echtes LRU für KV-Cache-Eviction

**Mehrwert-Typ:** Effizienz (Cache-Trefferquote). **Ansatzpunkt:** `crates/memfuse-kv-bridge/src/eviction_worker.rs` + `segment.rs`.
**Vorschlag:** Intrusive doubly-linked list über die bestehenden `Vec<KvSegment>`-Strukturen pro Tenant (oder ein `IndexMap`-basierter Ansatz mit O(1)-Move-to-Front bei jedem Zugriff über `store.rs`). Dies behebt A.3 direkt und macht die im Code bereits vorhandene Kommentar-Annahme „LRU an Index 0 angenommen" zur tatsächlichen Garantie. **Kopplung an A.5:** Sollte idealerweise gemeinsam mit der KV-Bridge-Increment-2-Verschlüsselung geplant werden, da beide Änderungen dieselbe Kern-Datenstruktur (`KvSegment`/`store.rs`) berühren — ein gemeinsamer ADR spart einen zweiten Migrationszyklus.

### B.3 Vollkonsolidierung des `PhysioScheduler`

**Mehrwert-Typ:** Mehrwert (Architektur-Integrität) + indirekt Effizienz (ein gemeinsamer Scheduler-Takt statt zweier unabhängiger Timer reduziert Kontext-Switches und ermöglicht Prioritäts-Ordering zwischen Reaper-Aufgaben).
**Ansatzpunkt:** `crates/memfuse-db/src/physio_scheduler.rs` übernimmt schrittweise `start_thermostat_reaper` und `start_nrem_reaper` aus `reaper.rs` als intern aufgerufene Phasen statt eigenständig gestarteter Tasks. **Migrationsstrategie:** Feature-Flag-gesteuerter Parallelbetrieb (alter Pfad + neuer Scheduler-Pfad gleichzeitig hinter einem Flag), Vergleichsmessung über mehrere Tage, dann Cutover — analog zum bereits etablierten Muster für `physio-nucleation` (VETO-F02).

### B.4 F-03-Fusionssignal-Integration (SynapticUpdateBuffer)

**Mehrwert-Typ:** Mehrwert (Retrieval-Qualität — fünftes Signal ergänzt Vektor/BM25/Graph/Kohärenz um lernbare, nutzungsbasierte Kantengewichtung).
**Ansatzpunkt:** Neuer `SynapticUpdateBuffer` in `memfuse-graph`, der Hebbian-Scores aus `synaptic.rs` sammelt und über den unter B.3 konsolidierten `PhysioScheduler` periodisch via `flush_to_csr()` in den CSR-Graphen zurückschreibt. `fusion.rs` erhält ein optionales fünftes RRF-Gewicht, feature-flagged (P12-konform, Default-unsichtbar).
**Reihenfolge-Empfehlung:** Erst B.3 (Scheduler-Konsolidierung), dann B.4 — der Flush-Hook ist im Scheduler bereits als Kommentar vorgesehen.

### B.5 Aktivierung und empirische Kalibrierung von F-09 (Resonanz-Kohärenz-Bonus)

**Mehrwert-Typ:** Mehrwert (Retrieval-Qualität bei dicht vernetzten Wissensgraphen).
**Ansatzpunkt:** Nach dem P0-Fix aus A.1 (Feature-Flag ergänzen) wird F-09 erstmals kompilierbar und testbar. **Nächster Schritt:** LongMemEval-Lauf mit `beta=0.5` (Code-Default) vs. `beta=0.15` (ursprüngliche v5.0-Designabsicht) über `memfuse-bench`, um die in K15 offene Kalibrierungsfrage empirisch statt durch Annahme zu entscheiden — direkte Anwendung des in P7 verankerten Prinzips „Marketing-Aussagen sind an Code-Nachweise gebunden".

### B.6 PID-`min_pool_size`-Rekalibrierung mit Benchmark-Beleg

**Mehrwert-Typ:** Mehrwert (Recall-Stabilität) potenziell auf Kosten von Effizienz (größerer Kandidatenpool = mehr Rerank-Aufwand) — daher explizit als Effizienz/Qualitäts-Trade-off zu vermessen, nicht blind zu erhöhen.
**Ansatzpunkt:** `crates/memfuse-calibration/src/pid.rs`. **Vorschlag:** Parametrisierten Benchmark-Sweep über `min_pool_size ∈ {10, 20, 50, 100}` gegen die bestehende `LongMemEvalCase`-Harness fahren, Recall@5 vs. p95-Rerank-Latenz gegeneinander auftragen (Pareto-Front, analog zur bereits vorhandenen `audit_benchmarks.rs`-Methodik in `memfuse-index`), danach den Default evidenzbasiert setzen und per ADR fixieren.

### B.7 PathRAG-`sufficiency_threshold`-Validierung

**Mehrwert-Typ:** Mehrwert (Korrektheit/Precision, Vermeidung des dokumentierten MemGraphRAG-Precision-Risikos).
**Ansatzpunkt:** `crates/memfuse-graph/src/path_rag.rs`. **Vorschlag:** Denselben Benchmark-Sweep-Ansatz wie B.6 auf `sufficiency_threshold ∈ {0.01, 0.1, 0.3, 0.6}` anwenden, LongMemEval-Precision als Zielmetrik, Ergebnis per ADR normativ festschreiben (schließt A.10 ab).

### B.8 `PENDING_FLUSH_THRESHOLD`-Write-Amplification-Studie

**Mehrwert-Typ:** Effizienz (Schreib-Durchsatz/SSD-Lebensdauer bei kleinen Collections).
**Ansatzpunkt:** `crates/memfuse-index/src/diskann.rs:37`. **Vorschlag:** Benchmark mit variabler Collection-Größe (100/1.000/10.000/100.000 Vektoren) und variablem Threshold (50/200/1.000), Messung von Gesamt-Schreibvolumen und p95-Insert-Latenz. Ergebnis entscheidet, ob ein statischer Wert genügt oder ein **adaptiver Threshold** (proportional zur Collection-Größe) sinnvoll ist — Letzteres wäre eine über den ursprünglichen v6.0-Rahmen hinausgehende, aber naheliegende Effizienzverbesserung.

### B.9 GASP/TPA-Halluzinations-Postvalidator — Abhängigkeitspfad klären

**Mehrwert-Typ:** Mehrwert (Ausgabe-Zuverlässigkeit, Kernversprechen der Produktvision Säule I/II).
**Ansatzpunkt:** Voraussetzung ist zunächst die in §1.2/Säule I der Gesamtspezifikation als offen markierte `memfuse-candle`-Pipeline-Integration (Candle-Factory in `memfuse-mcp/Cargo.toml`, `create_embedding_provider()`). **Empfehlung:** Diese Abhängigkeit explizit als Vorbedingung in der Roadmap-Priorisierung führen, damit GASP nicht isoliert und dann doppelt an eine später integrierte Pipeline angepasst werden muss — direkte Anwendung von P10 (Reuse-vor-Neubau) auf Roadmap-Ebene.

### B.10 TenantId-Härtung mit automatisierter Aufrufer-Migration

**Mehrwert-Typ:** Mehrwert (Sicherheits-Invariante wird typsystem- statt konventionsbasiert erzwungen).
**Ansatzpunkt:** Direkte Umsetzung von A.2. **Vorschlag:** Zweistufiges Vorgehen — (1) `#[deprecated]` auf `new()`/`DEFAULT`/`INVALID` setzen und einen vollständigen `cargo build`-Lauf zur Aufrufer-Inventarisierung nutzen (die Deprecation-Warnungen listen alle Call-Sites vollautomatisch auf), (2) `From<u64>` entweder auf `try_new()`-Semantik umstellen (mit `Result`-Rückgabe, API-Bruch) oder als `#[deprecated]` markieren und einen neuen `TryFrom<u64>` als einzigen normativen Weg einführen (kein API-Bruch, additiv). **Empfehlung:** Variante 2b (additiv), da sie P1-Migrationen ohne Breaking-Change ermöglicht und dem in `ADR-065` etablierten Muster automatisierter CI-Gates entspricht — ein analoges `xtask check-tenant-construction`-Gate wäre eine natürliche Erweiterung des bestehenden `check-vetoes`/`check-duplicate-symbols`-Mechanismus.

### B.11 VETO-F02-Fristüberwachung automatisieren

**Mehrwert-Typ:** Mehrwert (Prozesssicherheit, verhindert stilles Fristversäumnis).
**Ansatzpunkt:** `xtask/src/check_vetoes.rs`. **Vorschlag:** Erweiterung des bestehenden CI-Gates um eine Prüfung von `conditional_review_due`-Daten gegen das aktuelle Build-Datum — bei Unterschreitung eines Vorlauf-Schwellenwerts (z. B. 7 Tage) schlägt der CI-Lauf mit einer expliziten Warnung fehl, statt die Frist stillschweigend verstreichen zu lassen. Dies ist eine kleine, rein additive Erweiterung eines bereits produktiven Mechanismus.

---

## Priorisierungs-Matrix (Zusammenfassung Teil A + Teil B)

| Priorität | Schulden (Teil A) | Zugehörige Roadmap-Maßnahme (Teil B) |
|---|---|---|
| **P0** | A.1 (F-09-Flag) | B.5 (Aktivierung + Kalibrierung) |
| **P1** | A.2 (TenantId), A.4 (Scheduler), A.10 (PathRAG-Threshold), A.12 (VETO-Frist) | B.10, B.3, B.7, B.11 |
| **P2** | A.3 (LRU), A.5 (KV-Krypto), A.8 (PID-Pool), A.9 (Flush-Threshold) | B.2, (B.2-Kopplung), B.6, B.8 |
| **H2** | A.6 (F-03-Integration), A.11 (fsync-Policy) | B.4, B.1 |
| **H3** | A.7 (GASP) | B.9 |

**Governance-Hinweis für alle künftigen Umsetzungen:** Jede Maßnahme aus Teil B, die eine sicherheits- oder latenzrelevante Verhaltensänderung einführt (insbesondere B.1, B.10), erfordert gemäß P6/§14 der Gesamtspezifikation v7.0 einen ADR unter `docs/decisions/` **vor** Merge, sowie — bei quantitativen Qualitätsversprechen (B.5, B.6, B.7, B.8) — eine reproduzierbare Messung in `memfuse-bench` gemäß P7.
