# MemFuse — Konsolidierungs- & Fortschritts-Prompts für Google Jules
## Stand HEAD `05b382d8` (07.09.2026, Abend) — Live-Audit gegen 5 Eingangsdokumente

> **Methodik dieses Dokuments:** Jede Behauptung der fünf hochgeladenen Dokumente (`memfuse_gesamtspezifikation_v5_0.md`, `memfuse_temporaere_probleme_2026-09-07.md`, `memfuse_principal_architect_review_2026-09-07.md`, sowie die Vorgänger `memfuse_architektur_praezisierung.md`/`memfuse_spec.md`) wurde vor Aufnahme in dieses Dokument **erneut, unabhängig gegen den frisch gepullten Code verifiziert** (`git log`, `grep`, Dateiinspektion) — nicht unkritisch übernommen. Alle drei Vorgänger-Prüfungen (HEAD `36ad007a` → `bb099dc2` → `ade2f12f`) sind zum Zeitpunkt dieser Prüfung selbst bereits teilweise überholt: Die überwiegende Mehrheit der darin gemeldeten P0/P1-Befunde (Duplikat-Konstanten in `diskann.rs`, fehlender `memfuse-kv-bridge`-DAG-Eintrag, `persist_delta()`-Hot-Path-Blockierung, `VETOES.md`, F-02-Recall-Test, F-11-Integration, Cascading-Invalidation) ist **bereits behoben**. Dieses Dokument berichtet daher nur, was zum jetzigen Zeitpunkt tatsächlich noch zutrifft — inklusive **eines neuen, in keinem der fünf Dokumente erfassten Befunds** (Prompt 1).

**Wichtige Korrektur gegenüber meiner eigenen vorherigen Prüfung:** In einer früheren Runde wurde `memfuse-py` fälschlich als kritischer Build-/Sicherheits-Bug eingestuft. Das Temp-Dokument (§4.1, jüngere und gründlichere Quelle) stellt korrekt klar: Das separate `[workspace]`-Manifest in `crates/memfuse-py/Cargo.toml` ist **bewusste, im Code selbst begründete Architektur** (getrenntes `panic = "unwind"`-Profil für die PyO3-FFI-Grenze, da der Haupt-Workspace `panic = "abort"` fährt), CI deckt dies über separate `--manifest-path`-Aufrufe ab. Dieser Punkt wird hier nicht erneut als Bug behandelt, sondern als reine Dokumentationspflicht in Prompt 3 abschließend geklärt.

---

## Gliederung

| Teil | Inhalt | Prompts |
|---|---|---|
| **A** | Bugfixes & Korrekturen — **vor jeder weiteren Spec-Arbeit auszuführen** | 1–3 |
| **B** | Nächste Implementierungsschritte gemäß Gesamtspezifikation v5.0 | 4–8 |

**Empfohlene Ausführungsreihenfolge bei begrenzter Parallelität:** Teil A vollständig vor Teil B, da Prompt 1 (totes Duplikat) und Prompt 4 (PhysioScheduler) denselben Aufrufpfad (F-07-Replicator) berühren. Bei ausreichender Parallelkapazität sind alle 8 Prompts dennoch **datei-disjunkt** und können gleichzeitig an Jules gesendet werden — siehe Abhängigkeits-Hinweise am Dokumentende.

---

# TEIL A — Bugfixes & Korrekturen

## Prompt 1 — NEU (in keinem Dokument erfasst): Totes Duplikat der F-07-Replikatordynamik entfernen (`memfuse-db`)

```md
ROLLE: Du bist Senior Rust Architect mit Spezialisierung auf Code-Konsolidierung, Dead-Code-Elimination und Multi-Crate-Workspace-Hygiene.

REPOSITORY: https://github.com/tfufuz1/memfuse, aktueller HEAD (main-Branch).

BEFUND (durch eigene, unabhängige Code-Analyse verifiziert — dieser Fund taucht in KEINEM der Architektur-Dokumente auf, verifiziere ihn daher selbst vor Beginn, PFLICHT):

Es existieren AKTUELL ZWEI vollständig unabhängige, parallele Implementierungen von Feature F-07 (Replikatordynamik für adaptive RRF-Signalgewichte):

1. **`crates/memfuse-db/src/replicator.rs`** (`AdaptiveFusionWeights`) — multiplikatives Update mit Clamping, KEINE `ConfigFingerprint`-Anbindung (P8-Compliance fehlt), KEIN `#[cfg(feature = ...)]`-Gate.
2. **`crates/memfuse-calibration/src/replicator.rs`** (`ReplicatorState`) — Multiplicative-Weights-Update-Method nach Arora et al. (2012) mit Regret-Bound-Garantie, VOLLSTÄNDIGE `ConfigFingerprint`-Integration (P8-konform), hinter `#[cfg(feature = "physio-replicator-weights")]` gated.

VERIFIZIERE SELBST per `grep -rn "AdaptiveFusionWeights\|ReplicatorState" --include=*.rs crates/ | grep -v "src/replicator.rs"`, welche der beiden Implementierungen TATSÄCHLICH im Produktionspfad verwendet wird. Zum Zeitpunkt dieser Analyse gilt:
- `crates/memfuse-db/src/collection/query_builder.rs` importiert und verwendet AUSSCHLIESSLICH `memfuse_calibration::ReplicatorState` (per `Arc<parking_lot::RwLock<memfuse_calibration::ReplicatorState>>`).
- `crates/memfuse-db/src/replicator.rs` (`AdaptiveFusionWeights`) hat KEINEN einzigen Aufrufer außerhalb seiner eigenen Datei und ihrer eigenen `#[cfg(test)]`-Tests — es ist vollständiger, unbenutzter Dead Code, der nur über `pub mod replicator;` in `crates/memfuse-db/src/lib.rs` öffentlich exportiert wird, ohne dass irgendein interner oder externer Aufrufer ihn nutzt.

Dies ist ein klarer Verstoß gegen das im Projekt etablierte Prinzip P10 (Reuse-vor-Neubau) — zwei unabhängige Implementierungen derselben mathematischen Anforderung wurden vermutlich in zwei getrennten, nicht koordinierten Agenten-Sessions gebaut, exakt das Muster, das laut `memfuse_goldstandard_kritische_bewertung.md` §6.1 bereits einmal (für Kalibrierungsprimitive) korrekt konsolidiert wurde. Diese Konsolidierung muss jetzt für F-07 nachgeholt werden.

AUFGABE — Entferne das unbenutzte Duplikat vollständig und sauber:

1. Bestätige VOR jeder Änderung durch eigene, frische Ausführung der obigen `grep`-Kommandos, dass `AdaptiveFusionWeights`/`crates/memfuse-db/src/replicator.rs` zum Zeitpunkt DEINER Bearbeitung immer noch keinen produktiven Aufrufer hat (Codebasis kann sich zwischen dieser Analyse und deiner Ausführung weiterentwickelt haben — falls doch ein Aufrufer existiert, BRICH AB und dokumentiere den abweichenden Befund, statt produktiven Code zu löschen).
2. Lösche `crates/memfuse-db/src/replicator.rs` vollständig.
3. Entferne `pub mod replicator;` aus `crates/memfuse-db/src/lib.rs` sowie jeden zugehörigen `pub use replicator::...`-Re-Export.
4. Durchsuche den GESAMTEN Workspace (`grep -rn "memfuse_db::replicator\|db::replicator\|AdaptiveFusionWeights" --include=*.rs crates/`) nach verwaisten Referenzen (Doku-Kommentare, Tests, andere Module) und bereinige sie.
5. Prüfe, ob `crates/memfuse-db/src/fusion.rs` oder `crates/memfuse-db/src/collection/query_builder.rs` irgendwelche Typen aus dem gelöschten Modul importieren (auch indirekt über `use crate::fusion::SignalKind;` in der gelöschten Datei — das war eine EINSEITIGE Abhängigkeit von `replicator.rs` auf `fusion.rs`, nicht umgekehrt, sollte also unkritisch sein, aber VERIFIZIERE dies explizit durch Kompilierbarkeits-Prüfung).
6. Ergänze in `crates/memfuse-calibration/src/replicator.rs` (der jetzt EINZIGEN verbleibenden Implementierung) einen kurzen Kommentarblock am Dateikopf: `// KONSOLIDIERUNGS-HINWEIS: Dies ist die einzige F-07-Implementierung im Workspace (Stand <Datum>). Eine zweite, unbenutzte Implementierung existierte zuvor in memfuse-db/src/replicator.rs und wurde entfernt (P10-Konsolidierung). Vor jeder künftigen F-07-Änderung: prüfe zuerst, ob diese Datei bereits die benötigte Funktionalität bietet.`

TESTS:
- `cargo build --workspace` bleibt vollständig grün nach der Löschung (Beweis, dass keine versteckte Abhängigkeit übersehen wurde).
- `cargo test -p memfuse-db` bleibt grün (alle bestehenden Tests, die NICHT das gelöschte Modul betrafen).
- `cargo test -p memfuse-calibration --features physio-replicator-weights` bleibt grün (unverändertes Verhalten der verbleibenden, einzigen Implementierung).
- Falls beim Löschen Tests in `crates/memfuse-db/src/replicator.rs` verloren gehen, die eine FUNKTIONAL ANDERE Eigenschaft prüften als die Tests in `memfuse-calibration/src/replicator.rs` (z. B. ein Grenzfall, der in der verbleibenden Implementierung nicht abgedeckt ist): portiere GENAU DIESEN Testfall (angepasst an die `ReplicatorState`-API) nach `crates/memfuse-calibration/src/replicator.rs`, statt ihn ersatzlos zu verlieren. Dokumentiere im Abschlussbericht, ob dieser Fall aufgetreten ist.

AKZEPTANZKRITERIEN:
- `crates/memfuse-db/src/replicator.rs` existiert nicht mehr.
- `cargo build --workspace` und `cargo test -p memfuse-db -p memfuse-calibration` grün.
- Keine verwaisten Imports/Kommentare/Doku-Referenzen auf das gelöschte Modul im gesamten Workspace.
- Abschlussbericht listet explizit: (a) das Ergebnis der Vorab-Verifikation (Schritt 1), (b) ob Testfälle portiert werden mussten (letzter Absatz oben), (c) den vollständigen `git diff`.

WICHTIG: Ändere NICHT `crates/memfuse-calibration/src/replicator.rs` inhaltlich (außer dem in Schritt 6 geforderten Kommentar) und NICHT `crates/memfuse-db/src/collection/query_builder.rs` — beide sind bereits korrekt. Diese Aufgabe ist eine reine, chirurgische Löschung von totem Code.
```

---

## Prompt 2 — Dokumentations-Drift beheben: Veralteter Datenverlust-Kommentar in `diskann.rs` (`memfuse-index`)

```md
ROLLE: Du bist Senior Rust Engineer für Storage-Engines mit Fokus auf Code-Dokumentations-Konsistenz und Crash-Recovery-Beweisführung.

REPOSITORY: https://github.com/tfufuz1/memfuse. Arbeite ausschließlich in `crates/memfuse-index/src/diskann.rs`, beschränkt auf einen Kommentarblock (Zeilen ~30–33, exakte Zeilennummer per `grep -n "PENDING_FLUSH_THRESHOLD" crates/memfuse-index/src/diskann.rs` verifizieren, da sich Zeilennummern durch andere parallele Prompts verschieben können).

BEFUND (verifiziere selbst vor Beginn — PFLICHT):
Der Kommentar über der Konstante `PENDING_FLUSH_THRESHOLD` lautet aktuell:
```rust
/// Pending-Threshold: nach 50 pending inserts → auto-trigger persist_delta.
/// RISIKO-FENSTER: Maximal 50 ungeflushte Vektoren befinden sich vor einem synchronen persist_delta()
/// ausschließlich im In-Memory pending_inserts Buffer. Bei einem unvorhergesehenen Absturz / OOM
/// innerhalb dieses 50-Insert-Fensters sind nicht-geflushte Vektoren unpersistent.
const PENDING_FLUSH_THRESHOLD: u64 = 50;
```

Dieser Kommentar ist FAKTISCH VERALTET: Die Funktion `insert()` (`impl VectorIndex for DiskAnnIndex`) ruft bereits VOR dem In-Memory-Push `Self::append_to_pending_wal(&pending_wal, id, embedding).await?;` auf — d. h. jeder eingefügte Vektor wird ZUERST WAL-first persistiert, BEVOR er in den In-Memory-Puffer `pending_inserts` aufgenommen wird. Zusätzlich existiert bereits die Funktion `recover_pending_delta()`, die diesen WAL beim Neustart nach einem Absturz wiederherstellt. Das im Kommentar beschriebene Datenverlust-Risiko ("nicht-geflushte Vektoren unpersistent") besteht damit NICHT mehr — es wurde durch den WAL-rückgestützten `pending.wal`-Pfad bereits behoben (verifiziere dies durch Lektüre von `append_to_pending_wal()` und `recover_pending_delta()` vor deiner Änderung).

RISIKO EINES SOLCHEN DRIFT-KOMMENTARS: Ein künftiger Bearbeiter (menschlich oder Agent) könnte diesen veralteten Kommentar als aktuelle Risikobeschreibung fehlinterpretieren und unnötigen, redundanten Aufwand in eine bereits gelöste Problemstellung investieren, oder — schlimmer — bei einem Refactoring fälschlich annehmen, dass VOR dem WAL-Write kein Persistenz-Schutz besteht, und eine bereits vorhandene Sicherheitsgarantie versehentlich wieder entfernen.

AUFGABE — Aktualisiere AUSSCHLIESSLICH diesen einen Kommentarblock, um den tatsächlichen, aktuellen Schutzmechanismus korrekt zu beschreiben:

1. Lies `append_to_pending_wal()` und `recover_pending_delta()` vollständig, um die exakte Garantie präzise (nicht vage) formulieren zu können — insbesondere: Wird der WAL-Write per `fsync` durchgesetzt, bevor die Funktion zurückkehrt? Verifiziere dies im Code, bevor du eine Zusicherung dazu in den neuen Kommentar schreibst.
2. Ersetze den Kommentar durch eine Version, die (a) den historischen Kontext kurz erwähnt (warum die Schwelle 50 ist — reine Performance-Batching-Entscheidung, kein Sicherheits-Kompromiss mehr), (b) explizit auf `append_to_pending_wal()` und `recover_pending_delta()` als aktuellen Schutzmechanismus verweist, (c) die tatsächlich noch bestehenden Grenzen ehrlich benennt, falls beim Lesen in Schritt 1 welche auffallen (z. B. falls `fsync` nicht bei jedem einzelnen WAL-Write erzwungen wird, sondern gebündelt — dann ist das Fenster kleiner als vorher, aber nicht zwingend null; formuliere PRÄZISE, was tatsächlich zutrifft, erfinde keine perfekte Garantie, die der Code nicht hält).

BEISPIEL-ZIELFORMULIERUNG (als Ausgangspunkt, an tatsächlichen Code-Befund anpassen, NICHT ungeprüft übernehmen):
```rust
/// Pending-Threshold: nach 50 pending inserts → auto-trigger persist_delta (reine
/// Batching-Performance-Entscheidung, siehe trigger_background_persist_delta()).
/// CRASH-SICHERHEIT: Jeder insert() schreibt VOR dem In-Memory-Push per
/// append_to_pending_wal() in `<index>.pending.wal`. Bei Absturz/OOM innerhalb
/// des Flush-Fensters stellt recover_pending_delta() beim nächsten Öffnen alle
/// WAL-persistierten, noch nicht in persist_delta() übernommenen Vektoren wieder
/// her — kein Datenverlust. (Historischer Hinweis: vor Einführung des WAL-Pfads
/// bestand hier ein Risikofenster; das ist seit <Commit-Referenz falls bekannt,
/// sonst weglassen> nicht mehr der Fall.)
const PENDING_FLUSH_THRESHOLD: u64 = 50;
```

TESTS: Keine Code-Logik-Änderung, daher keine neuen Tests erforderlich. Stelle jedoch sicher, dass ein bereits vorhandener Test die WAL-Recovery-Garantie tatsächlich abdeckt (suche z. B. nach `test_diskann_pending_wal_recovery` o. ä. per `grep -n "fn test.*pending.*wal\|fn test.*recover_pending" crates/memfuse-index/src/diskann.rs`) — falls ein solcher Test NICHT existiert, obwohl der Kommentar nun eine "kein Datenverlust"-Garantie behauptet, ergänze einen minimalen Regressionstest, der genau das verifiziert (Vektoren einfügen bis knapp unter Threshold, `pending.wal` simuliert "crashen" lassen — d. h. Index-Instanz verwerfen ohne `persist_delta()` aufzurufen —, neue Instanz öffnen, `recover_pending_delta()` aufrufen, prüfen dass alle zuvor eingefügten Vektoren wieder auffindbar sind).

AKZEPTANZKRITERIEN:
- `cargo build -p memfuse-index` und `cargo test -p memfuse-index` grün.
- Der neue Kommentar macht ausschließlich Aussagen, die durch tatsächliche Code-Lektüre in Schritt 1 verifiziert wurden — keine Übernahme der Beispielformulierung ohne eigene Prüfung.
- Keine Änderung an Code-Logik, ausschließlich Kommentar (+ ggf. ein neuer, in Schritt "TESTS" beschriebener Regressionstest, falls die Lücke dort tatsächlich besteht).

WICHTIG: Dies ist eine rein dokumentarische Korrektur mit optionaler Testergänzung. Ändere KEINE Konstanten-Werte, KEINE Funktionslogik in `insert()`, `append_to_pending_wal()` oder `recover_pending_delta()`.
```

---

## Prompt 3 — Governance-Dokumentation: ADR für `memfuse-py`-Workspace-Isolation verifizieren und finalisieren

```md
ROLLE: Du bist Senior Rust Engineer für Architektur-Governance, spezialisiert auf ADR-Prozesse (Architecture Decision Records) und Cargo-Workspace-Design.

REPOSITORY: https://github.com/tfufuz1/memfuse.

KONTEXT (verifiziere selbst vor Beginn — PFLICHT): Es besteht historisch Verwirrung darüber, ob `crates/memfuse-py` NICHT im Haupt-Workspace geführt zu werden ein Bug oder eine bewusste Entscheidung ist — frühere Audits haben dies gegensätzlich bewertet. Der ZUM JETZIGEN ZEITPUNKT korrekte, verifizierte Sachstand (bestätige dies selbst):
1. `crates/memfuse-py/Cargo.toml` enthält ein eigenständiges `[workspace]`-Manifest — das Crate ist ABSICHTLICH ein eigener Cargo-Workspace, kein Mitglied des Haupt-Workspace.
2. Der Grund ist im Code dokumentiert (`crates/memfuse-py/src/lib.rs`, Kommentar bei/um `run_blocking_ffi`): Nur als eigenständiger Workspace lässt sich für die PyO3-Bindings `panic = "unwind"` setzen, während der Haupt-Workspace `panic = "abort"` im Release-Profil fährt. Ohne diese Trennung wäre `catch_unwind()` an der FFI-Grenze wirkungslos — ein Rust-Panic würde den CPython-Interpreter per SIGABRT beenden statt als `PyRuntimeError` abfangbar zu sein.
3. Prüfe per `find .github/workflows -name "*.yml" | xargs grep -l "memfuse-py"`, ob CI diesen Crate tatsächlich separat abdeckt (z. B. über `--manifest-path crates/memfuse-py/Cargo.toml`-Aufrufe für `cargo check`/`cargo clippy`/`cargo test`/`maturin build`).
4. Prüfe per `find docs/decisions -iname "*memfuse-py*" -o -iname "*ADR-064*"`, ob bereits ein ADR existiert, der diese Entscheidung dokumentiert (ein Dokument erwähnt `ADR-064-memfuse-py-separater-workspace-panic-strategie.md`).

AUFGABE — Stelle sicher, dass diese Architekturentscheidung VOLLSTÄNDIG, WIDERSPRUCHSFREI und an ALLEN relevanten Stellen konsistent dokumentiert ist, damit sie in künftigen Audits nicht erneut fälschlich als Bug gemeldet wird:

1. FALLS `docs/decisions/ADR-064-memfuse-py-separater-workspace-panic-strategie.md` bereits existiert: Lies ihn vollständig. Prüfe, ob er (a) die Panic-Strategie-Begründung enthält, (b) explizit erwähnt, WIE CI diesen Crate dennoch abdeckt (Punkt 3 oben), (c) explizit als "kein technisches Schuld-Item, sondern bewusste Isolation" klassifiziert ist. Falls einer dieser drei Punkte fehlt: ergänze ihn minimal-invasiv, ohne den bestehenden Text unnötig umzuschreiben.
2. FALLS der ADR NICHT existiert: Erstelle ihn nach dem im Projekt etablierten ADR-Format (prüfe das Format von 2-3 bestehenden ADRs in `docs/decisions/` als Vorlage, z. B. `ADR-063-f02-nucleation-tombstone-pruning-vs-ursprungliches-veto.md`, und übernimm Struktur/Tonfall konsistent). Inhalt: Kontext (Panic-Strategie-Konflikt), Entscheidung (eigenständiger Workspace), Konsequenzen (CI-Abdeckung separat, `cargo build --workspace` erfasst diesen Crate NICHT, das ist gewollt), Alternativen-Abwägung (Root-Workspace-Member mit Profil-Override — technisch nicht möglich, da Cargo `[profile.*]` nicht pro Package innerhalb eines Workspace überschreiben lässt, verifiziere diese Cargo-Einschränkung als Begründung).
3. Prüfe `Anhang A`/„Wettbewerbspositionierung"/„Technische Schulden"-Abschnitte in den vorhandenen Markdown-Spezifikationsdateien im Repository (falls solche Dateien selbst Teil des Repos sind, z. B. unter `docs/specs/` — NICHT die extern hochgeladenen Dokumente dieser Session) auf verbliebene Erwähnungen von „`memfuse-py` fehlt im Workspace" als offene Lücke oder technische Schuld. Falls gefunden: korrigiere die Einordnung unter Verweis auf den ADR aus Schritt 1/2.
4. Ergänze — FALLS NICHT BEREITS VORHANDEN — im `AGENTS.md`/`.jules/`-Bootstrap-Kontext (prüfe `find . -iname "AGENTS.md" -o -path "*/.jules/*"`) einen kurzen, prominenten Hinweis, damit ein künftiger Jules-Agent, der `cargo build --workspace` ausführt und `memfuse-py` darin vermisst, nicht reflexhaft einen „Fix"-Versuch startet, sondern zuerst den ADR konsultiert.

VERIFIKATION (PFLICHT): Stelle sicher, dass CI (Punkt 3 der Kontext-Analyse) TATSÄCHLICH alle notwendigen Qualitätsprüfungen für `memfuse-py` durchführt, die auch für Haupt-Workspace-Crates gelten (Build, Clippy, Tests). Falls eine dieser Prüfungen in der CI-Konfiguration für `memfuse-py` FEHLT (z. B. Clippy wird aufgerufen, aber `cargo test` nicht, oder umgekehrt): ergänze den fehlenden CI-Schritt minimal-invasiv nach dem Muster der bereits vorhandenen `--manifest-path`-Aufrufe in derselben Workflow-Datei. Dies ist der EINZIGE Punkt, an dem diese Aufgabe tatsächlichen CI-Code ändern darf — alles andere ist reine Dokumentationsarbeit.

AKZEPTANZKRITERIEN:
- Ein vollständiger, in sich konsistenter ADR zu dieser Entscheidung existiert unter `docs/decisions/`.
- CI deckt `memfuse-py` mit Build+Clippy+Test (mindestens) nachweislich ab — falls eine Lücke gefunden und geschlossen wurde, ist dies im Abschlussbericht mit vorher/nachher-Diff dokumentiert.
- Keine verbliebenen, widersprüchlichen Aussagen im Repository, die `memfuse-py`s Workspace-Isolation als offenen Bug statt als dokumentierte Entscheidung führen.
- KEINE Änderung an `crates/memfuse-py/Cargo.toml`, `Cargo.toml` (Root) oder `crates/memfuse-py/src/lib.rs` — diese Aufgabe ist rein dokumentarisch/CI-verifizierend, die Architekturentscheidung selbst steht nicht zur Debatte.

WICHTIG: Diese Aufgabe darf unter KEINEN Umständen `crates/memfuse-py` zum Haupt-Workspace hinzufügen — das wäre eine Reversion der bewussten, technisch begründeten Entscheidung und würde die dokumentierte FFI-Panic-Sicherheitsgarantie brechen.
```

---

# TEIL B — Nächste Implementierungsschritte (Gesamtspezifikation v5.0)

## Prompt 4 — Zentraler `PhysioScheduler` & `PhysioConfig` (§10 der Spezifikation) — Konsolidierung der vier unabhängigen Reaper-Tasks

```md
ROLLE: Du bist Senior Rust Architect für Agenten-Infrastruktur, spezialisiert auf Hintergrund-Task-Orchestrierung, WAL-Intent-Pattern und Konfigurationsmanagement.

REPOSITORY: https://github.com/tfufuz1/memfuse. Primäres Arbeitsverzeichnis: `crates/memfuse-db/src/`, neue Datei `physio_scheduler.rs` + Erweiterung von `physio_config.rs` (neu) + minimal-invasive Anpassung von `reaper.rs`.

KONTEXT (verifiziere selbst vor Beginn — PFLICHT):
Lies `crates/memfuse-db/src/reaper.rs` VOLLSTÄNDIG. Aktuell existieren VIER unabhängige, jeweils eigenständig per `tokio::spawn` + `tokio::time::interval` laufende Hintergrund-Tasks: `start_nrem_reaper`, `start_expiry_reaper`, `start_thermostat_reaper`, `start_orphan_reaper`. Jeder hat seinen eigenen Ticker, sein eigenes Intervall, keine gemeinsame Koordination, kein gemeinsames WAL-Intent-Pattern.

Die Spezifikation (§10.1) fordert stattdessen EINEN zentralen `PhysioScheduler` mit EINEM konfigurierbaren Tick-Intervall (Default 60s), der sequenziell/koordiniert folgende Schritte durchführt:
```
├── F-01 Thermostat-Update (tombstone_ratio + query_rate)
├── F-03 SynapticUpdateBuffer.flush_to_csr() (wenn Buffer > threshold) [siehe Prompt 5 — hier nur als OPTIONALER Hook einbauen]
├── F-06 Perkolations-BFS-Sampling (wenn nicht im Active-Session-Fenster)
├── F-07 Replikatordynamik-Update (adaptive RRF-Gewichte)
├── F-09 Kohärenz-Bonus-Parameter-Adaption (wenn F-07 aktiv)
├── F-11 LyapunovDriftWatcher.update() [BEREITS event-driven im Router integriert — NICHT hierher verschieben, siehe unten]
└── SleepCycle-Trigger (wenn active_agent_sessions() == 0)
```
Alle Scheduler-Aktionen: WAL-Intent vor Arbeit (`write_consolidation_intent()`), abschließend `complete_consolidation_intent()`.

WICHTIGE VORAB-KORREKTUR (verifiziere selbst): F-11 (Lyapunov) ist NICHT als periodischer Tick zu implementieren — es ist bereits korrekt EVENT-DRIVEN in `crates/memfuse-router/src/router.rs` integriert (`lyapunov_watchers: RwLock<HashMap<String, LyapunovDriftWatcher>>`, Update nach jeder Routing-Entscheidung). Nimm F-11 NICHT in den periodischen `PhysioScheduler`-Tick auf — das wäre eine architektonische Regression (Event-driven ist für Drift-Erkennung reaktionsschneller als ein 60s-Tick). Dokumentiere diese bewusste Abweichung von der wörtlichen §10.1-Aufzählung explizit im Code-Kommentar des Schedulers.

Prüfe VOR Implementierungsbeginn per `grep -rn "compute_percolation_health\|should_trigger_rebonding" crates/memfuse-db/src/collection/maintenance.rs`, WIE F-06 (Perkolation) aktuell aufgerufen wird — falls dies bereits an einer Stelle geschieht, die funktional gleichwertig zu einem periodischen Tick ist, portiere den AUFRUF (nicht die Logik) in den neuen Scheduler, statt ihn zu duplizieren.

Prüfe per `grep -n "ReplicatorState" crates/memfuse-db/src/collection/query_builder.rs`, wie F-07 aktuell instanziiert/gehalten wird (`Arc<parking_lot::RwLock<memfuse_calibration::ReplicatorState>>`), um denselben Zustand im Scheduler ohne Duplikation wiederzuverwenden (P10).

AUFGABE:

1. **`crates/memfuse-db/src/physio_config.rs`** [NEU]: Definiere `PhysioConfig` exakt nach der in der Spezifikation (§10.2) vorgegebenen Feldliste, ABER OHNE die F-03-Felder (Synaptic) — diese kommen in Prompt 5 hinzu, um Merge-Konflikte zu vermeiden; reserviere stattdessen einen Kommentar-Platzhalter `// F-03-Felder werden in einem separaten Schritt ergänzt (siehe synaptic.rs)`:
   ```rust
   #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
   pub struct PhysioConfig {
       pub tick_interval_secs: u64,        // Default: 60
       // F-01 Thermostat
       pub thermostat_enabled: bool,       // Default: true
       pub thermostat_w1: f32, pub thermostat_w2: f32, pub thermostat_kappa: f32,
       pub thermostat_base_half_life: u64,
       // F-03-Felder werden in einem separaten Schritt ergänzt (siehe synaptic.rs)
       // F-06 Perkolation — Felder gemäß bestehender PercolationConfig übernehmen/wiederverwenden, NICHT duplizieren
       // F-07 Replikatordynamik
       pub replicator_enabled: bool,       // Default: true
       pub replicator_lr: f32,             // Default: 0.05
       // F-09 Kohärenz-Bonus
       pub coherence_bonus_beta: f32,      // Default: 0.15
       // SleepCycle
       pub sleep_cycle_enabled: bool,      // Default: false (erfordert LLM)
       pub sleep_episode_threshold: usize, // Default: 50
   }
   impl Default for PhysioConfig { /* Werte exakt wie in Spec-Kommentaren oben */ }
   ```
   Prüfe, ob `ThermostatConfig`, `PercolationConfig` bereits existierende, äquivalente Felder haben — falls ja, nutze `#[serde(flatten)]` oder Komposition (`pub thermostat: ThermostatConfig`) statt Felder zu duplizieren (P10).

2. **`crates/memfuse-db/src/physio_scheduler.rs`** [NEU]:
   - `pub struct PhysioScheduler<S: StorageEngine, V: VectorIndex> { config: PhysioConfig, /* Handles auf bestehende Zustände: Thermostat, ReplicatorState-Arc, etc. */ }`
   - `pub fn start(...) -> tokio::task::JoinHandle<()>` — EIN `tokio::spawn` mit EINEM `tokio::time::interval(Duration::from_secs(config.tick_interval_secs))`.
   - Pro Tick, SEQUENZIELL (nicht parallel — vermeidet Lock-Contention zwischen den Physio-Subsystemen, die potenziell dieselben CSR-/Storage-Locks berühren):
     a. WAL-Intent schreiben (prüfe, ob `write_consolidation_intent()`/`complete_consolidation_intent()` bereits irgendwo im Code existiert, z. B. für den 2PC-Pfad in `crud.rs` — falls ja, wiederverwenden/nach demselben Muster ein physio-spezifisches Intent-Paar bauen; falls nicht, ein minimales analoges Muster einführen: ein `PhysioTickIntent`-Log-Eintrag vor der Arbeit, ein Completion-Marker danach, damit ein Crash mitten im Tick beim nächsten Start erkennbar ist — OHNE ein vollständiges neues 2PC-System zu bauen, das wäre außerhalb des Scopes).
     b. Falls `config.thermostat_enabled`: bestehenden Thermostat-Update-Aufruf (aus `start_thermostat_reaper` übernehmen, NICHT duplizieren).
     c. Falls Perkolations-Bedingung erfüllt (`nicht im Active-Session-Fenster` — prüfe, ob bereits eine Funktion `active_agent_sessions()`/äquivalent existiert; falls nicht, als einfachen Zähler mit klar dokumentierter Semantik neu einführen, aber MINIMAL, kein komplexes Session-Tracking-Subsystem): bestehenden Percolation-Aufruf aus `maintenance.rs` übernehmen.
     d. Falls `config.replicator_enabled`: `ReplicatorState`-Update aufrufen (prüfe existierende API in `memfuse-calibration`, welche Methode für ein periodisches Update vorgesehen ist — falls keine existiert, die für periodische statt Event-getriebene Aktualisierung geeignet ist, dokumentiere dies als Blocker und beschränke den Scheduler-Hook auf das, was die bestehende API tatsächlich hergibt, statt eine neue Methode in `memfuse-calibration` zu erfinden — das wäre Scope-Überschreitung in ein anderes Crate).
     e. F-03-Hook: ein LEERER, klar kommentierter Platzhalter-Call `// F-03 SynapticUpdateBuffer.flush_to_csr() — Hook wird von separatem Arbeitspaket ergänzt, siehe crates/memfuse-graph/src/synaptic.rs`, geschützt durch `#[cfg(feature = "physio-synaptic-edges")]` und eine Existenzprüfung, sodass dieser Prompt UNABHÄNGIG von Prompt 5 kompiliert und funktioniert, auch wenn Prompt 5 noch nicht gemergt ist.
     f. SleepCycle-Trigger: bestehenden `execute_nrem_cycle`-Aufruf (aus `start_nrem_reaper` übernehmen, NICHT duplizieren) — Bedingung `active_agent_sessions() == 0` gemäß Spec, oder falls diese Bedingung nicht sinnvoll umsetzbar ist ohne größere Session-Tracking-Infrastruktur, dokumentiere dies explizit und behalte vorerst die bestehende reine Intervall-Bedingung aus `start_nrem_reaper` bei (kein Rückschritt in der Funktionalität, aber auch keine erfundene Session-Erkennung).
     g. Completion-Marker (a) abschließen.
   - Fehlerbehandlung: Ein Fehler in EINEM Teilschritt (z. B. Thermostat-Update schlägt fehl) darf NICHT die übrigen Teilschritte im selben Tick verhindern — fange jeden Teilschritt-Fehler einzeln ab, logge via `tracing::error!`, fahre mit dem nächsten Teilschritt fort (P2 Zero-Panic-Doctrine, kein Absturz des Schedulers durch einen einzelnen fehlerhaften Teilschritt).

3. **`crates/memfuse-db/src/reaper.rs`**: Markiere `start_thermostat_reaper` und `start_nrem_reaper` als `#[deprecated(note = "Konsolidiert in PhysioScheduler — siehe physio_scheduler.rs. Wird nach Migrationsfrist entfernt.")]`, OHNE sie zu löschen (Rückwärtskompatibilität für etwaige externe Aufrufer, Migrationspfad statt Breaking Change). `start_expiry_reaper` und `start_orphan_reaper` bleiben UNVERÄNDERT (diese sind reine Daten-Hygiene-Tasks, keine Physio-Features im Sinne der Spezifikation — NICHT in den Scheduler integrieren, das wäre eine unnötige Vermischung von Konzepten).
4. Ergänze in `crates/memfuse-db/src/lib.rs` die Modul-Registrierung und öffentlichen Re-Exports für `physio_config` und `physio_scheduler`.

TESTS (`crates/memfuse-db/src/physio_scheduler.rs` und `crates/memfuse-db/tests/`):
- `PhysioScheduler` mit `thermostat_enabled=false, replicator_enabled=false` → Tick läuft durch, ruft NUR die aktiven Teilschritte auf (Mock-Verifikation via Zähler/Flag).
- Ein simulierter Fehler in einem Teilschritt (z. B. Mock-Thermostat, das einen Fehler zurückgibt) verhindert NICHT die Ausführung der nachfolgenden Teilschritte im selben Tick.
- WAL-Intent wird vor Arbeitsbeginn geschrieben und nach Abschluss als vollständig markiert (Regressionstest gegen unvollständige Intents bei simuliertem Absturz mitten im Tick, sofern die Intent-Infrastruktur dies technisch abbildet).
- Regressionstest: Bestehende `start_expiry_reaper`/`start_orphan_reaper`-Tests bleiben unverändert grün.

AKZEPTANZKRITERIEN:
- `cargo build -p memfuse-db` und `cargo test -p memfuse-db` grün.
- `start_thermostat_reaper`/`start_nrem_reaper` weiterhin vorhanden (deprecated, nicht gelöscht) und weiterhin funktionsfähig, falls noch irgendwo aufgerufen.
- Der neue `PhysioScheduler` kompiliert und läuft eigenständig, UNABHÄNGIG davon, ob Prompt 5 (F-03) bereits gemergt wurde (Platzhalter-Hook-Muster aus Schritt 2e).
- Kein `unwrap()`/`panic!` im Produktionscode; jeder Teilschritt-Fehler wird isoliert behandelt.

WICHTIG: Ändere NICHT die INTERNE Logik von `FreeEnergyThermostat`, `compute_percolation_health`, `ReplicatorState` oder `execute_nrem_cycle` — diese Aufgabe ist reine Orchestrierungs-Konsolidierung, kein Feature-Rewrite.
```

---

## Prompt 5 — F-03 Synaptische Verstärkung vollständig verdrahten: `SynapticUpdateBuffer` + CSR-Flush + 5. Fusionssignal (`memfuse-graph`, `memfuse-db`)

```md
ROLLE: Du bist Senior Rust Engineer für Graph-Datenstrukturen und lock-freie Nebenläufigkeit, spezialisiert auf Hebbian-Learning-inspirierte Kantengewicht-Adaption.

REPOSITORY: https://github.com/tfufuz1/memfuse. Arbeitsverzeichnisse: `crates/memfuse-graph/src/` (neue Datei `synaptic_buffer.rs`), `crates/memfuse-db/src/fusion.rs` und `crates/memfuse-db/src/collection/search.rs` (minimal-invasive Erweiterung).

KONTEXT (verifiziere selbst vor Beginn — PFLICHT, dieser Fund ist ungewöhnlich präzise vorbereitet, aber funktional unvollständig):
Der Vorbereitungsgrad für F-03 ist HÖHER als der Feature-Status "📋 H2 (geplant)" in der Spezifikation vermuten lässt, aber die eigentliche Funktionalität fehlt komplett:
1. `crates/memfuse-graph/src/synaptic.rs` enthält bereits die REINEN Berechnungsfunktionen: `synaptic_score()`, `apply_hebbian_update()`, `apply_pheromone_update()`, `apply_homeostatic_scaling()` — lies diese VOLLSTÄNDIG, sie sind die mathematische Grundlage, NICHT neu implementieren.
2. `crates/memfuse-graph/src/csr.rs`, `struct Edge`, hat bereits `#[cfg(feature = "physio-synaptic-edges")] pub hebbian_weight: f32` und `pub pheromone: f32` als Felder — die Datenstruktur ist vorbereitet.
3. `crates/memfuse-db/src/fusion.rs`, `enum SignalKind`, hat bereits `#[cfg(feature = "physio-synaptic-edges")] Synaptic` als Variante, und der Provenienz-Aggregations-Code (Zeile ~453) hat bereits einen `match`-Arm dafür — ABER dieser Arm setzt nur `index_type = Some("synaptic")`, es gibt aber NIRGENDS im Code einen tatsächlichen Retrieval-Pfad, der jemals ein Suchergebnis MIT `signal_kind = SignalKind::Synaptic` erzeugt. Verifiziere dies selbst: `grep -rn "SignalKind::Synaptic" --include=*.rs crates/memfuse-db/src crates/memfuse-graph/src` — alle Treffer sind Infrastruktur/Vorbereitung, KEIN tatsächlicher Produzent dieses Signals.
4. Es existiert AKTUELL KEIN `SynapticUpdateBuffer` (per `grep -rln "SynapticUpdateBuffer" --include=*.rs crates/` verifizieren — kein Treffer erwartet). Das ist der zentrale, laut Spezifikation geforderte Baustein: ein lock-freier (DashMap-basierter) Hot-Path-Puffer, der Co-Aktivierungs-Events sammelt, OHNE für jedes Update direkt in die CSR-Struktur zu schreiben (Contention-Vermeidung), mit asynchronem Flush in einem Background-Worker (WAL-first, P3-konform).

AUFGABE — Schließe die Lücke zwischen den bereits vorhandenen Bausteinen (1–3) und einer tatsächlich funktionierenden Ende-zu-Ende-Kette:

1. **`crates/memfuse-graph/src/synaptic_buffer.rs`** [NEU, hinter `#[cfg(feature = "physio-synaptic-edges")]`]:
   - `pub struct SynapticUpdateBuffer { pending: DashMap<(EntityId, EntityId), f32> /* (source, target) -> aufsummierte co_activation seit letztem Flush */, flush_threshold: usize }` (prüfe, ob `ahash`/`dashmap` bereits als Workspace-Dependency verfügbar ist — `memfuse-graph/Cargo.toml` prüfen; `ahash` ist laut vorherigem Audit bereits vorhanden, `dashmap` ggf. neu, als Abhängigkeit klar kennzeichnen falls neu hinzugefügt).
   - `pub fn record_co_activation(&self, source: EntityId, target: EntityId, strength: f32)` — Hot-Path-Methode, O(1) amortisiert, KEIN Lock auf die CSR-Struktur selbst (nur der interne DashMap-Shard-Lock, der per Design für parallele Writes aus verschiedenen Such-Threads ausgelegt ist).
   - `pub fn should_flush(&self) -> bool` — `self.pending.len() >= self.flush_threshold`.
   - `pub fn drain_for_flush(&self) -> Vec<((EntityId, EntityId), f32)>` — leert den Puffer atomar (nutze `DashMap`s Iterations-plus-Clear-Semantik korrekt, um Race Conditions mit gleichzeitigen `record_co_activation`-Aufrufern während des Drains zu vermeiden — dokumentiere die gewählte Nebenläufigkeitsstrategie explizit im Kommentar, z. B. `std::mem::take` auf einem `RwLock<DashMap<...>>`-Wrapper falls eine atomare Drain-Operation nötig ist, oder Begründung, warum minimal überlappende Writes während des Drains tolerierbar sind, da der nächste Flush sie ohnehin erfasst).

2. **`crates/memfuse-graph/src/synaptic.rs`** [Erweiterung, additiv]:
   - `pub fn flush_buffer_to_csr(buffer: &SynapticUpdateBuffer, csr: &mut CsrGraph, config: &SynapticConfig) -> FlushStats` — orchestriert: `drain_for_flush()` → für jedes `((source, target), accumulated_strength)`-Paar die bereits vorhandene `apply_hebbian_update()` auf die entsprechende CSR-Kante anwenden (finde die Kante über die bestehende CSR-Lookup-API, erfinde keine neue) → nach allen Updates eines Knotens `apply_homeostatic_scaling()` auf dessen ausgehende Kanten anwenden (Hub-Explosion-Schutz gemäß `Σ_j w_ij ≤ W_max`, bereits als Funktion vorhanden).
   - `pub struct FlushStats { pub edges_updated: usize, pub homeostatic_scaling_applied_to: usize }` (für Observability/Tests).
   - WAL-First-Konformität (P3): Bevor `flush_buffer_to_csr` tatsächlich CSR-Kantengewichte mutiert, muss die geplante Änderungsmenge protokolliert werden. Prüfe, ob CSR bereits einen WAL-Pfad für Kanten-Updates hat (z. B. über den bestehenden bi-temporalen Kanten-Mechanismus aus ADR-033/038) — falls ja, diesen wiederverwenden; falls CSR-Mutationen aktuell NICHT WAL-protokolliert werden (rein In-Memory mit periodischem Snapshot), dokumentiere dies als bestehende, außerhalb des Scopes dieses Prompts liegende Eigenschaft, statt ein neues WAL-Subsystem nur für diesen Anwendungsfall zu bauen.

3. **Retrieval-Pfad, der tatsächlich `SignalKind::Synaptic`-Ergebnisse produziert** (`crates/memfuse-db/src/collection/search.rs`, additiv, hinter demselben Feature-Flag):
   - Implementiere eine Suchfunktion, die für eine gegebene Ausgangs-Entity (z. B. aus dem Graph-Signal-Pfad bereits identifizierte Knoten) die `k` Nachbarn mit dem höchsten `synaptic_score(hebbian_weight, pheromone, alpha)` (bereits vorhandene Funktion) zurückgibt, als eigenständige Kandidatenliste mit `signal_kind = SignalKind::Synaptic` an die Fusion übergibt (`weighted_reciprocal_rank_fusion_with_options()`, bestehende Funktion, NICHT verändern — nur einen zusätzlichen Signal-Kandidaten-Vektor als Eingabe hinzufügen, an der Stelle, an der bereits Vector/Text/Graph-Kandidaten gesammelt werden).
   - Diese Funktion wird NUR aufgerufen, wenn `#[cfg(feature = "physio-synaptic-edges")]` UND eine Laufzeit-Config (`synaptic_enabled: bool`, aus `PhysioConfig` bzw. lokal, je nachdem ob Prompt 4 bereits gemergt ist — falls `PhysioConfig` noch nicht existiert, definiere ein lokales, minimales `SynapticSearchConfig { enabled: bool }` als Übergangslösung mit einem Kommentar, dass dies später in `PhysioConfig` konsolidiert werden sollte, sobald Prompt 4 gemergt ist) aktiviert ist.

4. **Hot-Path-Integration von `record_co_activation`**: Identifiziere die Stelle im bestehenden Suchpfad, an der "Co-Aktivierung" fachlich sinnvoll definiert ist (z. B.: zwei Entities, die im selben Retrieval-Ergebnis gemeinsam hoch gerankt wurden, oder zwei Knoten, die in derselben PathRAG-Traversierung aufeinanderfolgen — wähle EINE klar begründete, im Code als Kommentar dokumentierte Definition, erfinde nicht mehrere konkurrierende Heuristiken). Rufe an dieser Stelle `SynapticUpdateBuffer::record_co_activation()` auf. Diese Integration MUSS hinter dem Feature-Flag stehen und darf bei deaktiviertem Feature ABSOLUT KEINEN Overhead im Hot-Path erzeugen (kompiliere dies weg, nicht nur Laufzeit-Check).

TESTS (`crates/memfuse-graph/src/synaptic_buffer.rs`, `crates/memfuse-graph/src/synaptic.rs`, Integrationstest in `crates/memfuse-db/tests/`):
- `SynapticUpdateBuffer`: parallele `record_co_activation`-Aufrufe aus mehreren Threads akkumulieren korrekt (kein Lost-Update).
- `flush_buffer_to_csr`: nach Flush spiegeln die CSR-Kantengewichte (`hebbian_weight`) exakt die akkumulierten Co-Aktivierungs-Werte gemäß der Hebbian-Update-Formel wider (Regressionstest gegen `apply_hebbian_update()`s bereits bestehende Unit-Tests — konsistente Ergebnisse).
- Homöostatische Skalierung: Ein Knoten mit vielen stark verstärkten ausgehenden Kanten überschreitet nach Flush NIEMALS `Σ_j w_ij > W_max`.
- End-to-End: Nach mehreren simulierten Co-Aktivierungs-Events UND einem Flush liefert eine Suche mit aktiviertem `physio-synaptic-edges`-Feature tatsächlich Kandidaten mit `signal_kind = SignalKind::Synaptic` in der Fusion — UND die bestehende Invariante `INV-PROV-1` (`sum(contributions.rrf_contribution) ≈ rrf_score`) bleibt auch mit dem neuen 5. Signal erhalten (kritischer Regressionstest — verifiziere den bestehenden `INV-PROV-1`-Test in `fusion.rs` und stelle sicher, dass er bei aktiviertem Feature weiterhin grün ist).
- Feature deaktiviert (Standardfall, `physio-synaptic-edges` nicht kompiliert): Bestehendes Suchverhalten (nur Vector/Text/Graph, 3 Signale) bleibt BYTE-FÜR-BYTE unverändert — expliziter Regressionstest.

AKZEPTANZKRITERIEN:
- `cargo build -p memfuse-graph -p memfuse-db` (ohne Feature-Flag) grün, UNVERÄNDERTES Verhalten.
- `cargo build -p memfuse-graph -p memfuse-db --features physio-synaptic-edges` grün.
- `cargo test -p memfuse-graph -p memfuse-db --features physio-synaptic-edges` grün, inklusive aller oben beschriebenen neuen Tests.
- `INV-PROV-1` hält bei aktiviertem Feature mit 5 statt 4 Signalen.
- Kein `unwrap()`/`panic!` im Produktionscode.
- Feature bleibt gemäß P12 (Physio-Feature-Default-Unsichtbarkeit) NICHT default-aktiv.

WICHTIG: Das im Principal-Review (§F-03) genannte Akzeptanzkriterium ("30-Tage-Replay-Simulation: SynapticScore verbessert RRF-Recall@10 um ≥3pp ohne Hub-Übergewicht") ist NICHT Teil dieses Prompts — das ist eine spätere Evaluationsaufgabe nach LongMemEval-Verfügbarkeit (siehe Prompt 7). Diese Aufgabe stellt nur sicher, dass das Feature FUNKTIONAL korrekt end-to-end verdrahtet ist, hinter einem Flag, ohne bestehende Signale zu beeinträchtigen — nicht, dass es bereits nachweislich die Retrieval-Qualität verbessert.
```

---

## Prompt 6 — F-05 REM-Phase: Generative Wissenssynthese vervollständigen (`memfuse-db`)

```md
ROLLE: Du bist Senior Rust Engineer für ML-Systemdesign, spezialisiert auf Provenance-sichere LLM-Pipeline-Orchestrierung und Memory-Konsolidierungsarchitekturen (Sleep-Cycle-Pattern).

REPOSITORY: https://github.com/tfufuz1/memfuse. Arbeite additiv in `crates/memfuse-db/src/sleep_cycle.rs` und `crates/memfuse-db/src/sleep_cycle_executor.rs`.

KONTEXT (verifiziere selbst vor Beginn — PFLICHT):
Lies `crates/memfuse-db/src/sleep_cycle.rs` VOLLSTÄNDIG. Aktuell ist AUSSCHLIESSLICH die NREM-Phase implementiert: `NremConfig`, `TurnSegment`, `NremPhaseResult`, `group_turns_into_segments()`, `detect_near_duplicates()`, `run_nrem_phase()`, `compact_segment_via_context_compactor()`. Es gibt KEINE REM-Phase (`RemConfig`, `MetaChunk`, `run_rem_phase()` existieren nicht — verifiziere per `grep -n "Rem\|REM" crates/memfuse-db/src/sleep_cycle.rs`, erwarte NUR Treffer im Wort "Remove"/"remaining" o. ä., keine tatsächliche REM-Phasen-Logik).

Lies zusätzlich `crates/memfuse-db/src/sleep_cycle_executor.rs` VOLLSTÄNDIG — dort ist `execute_nrem_cycle` bereits produktiv in `reaper.rs::start_nrem_reaper` verdrahtet. `execute_sleep_cycle` wird bereits re-exportiert (`crates/memfuse-db/src/lib.rs:100`), prüfe, ob diese Funktion bereits einen (auch nur stub-artigen) REM-Aufruf enthält oder ob sie aktuell ein Alias/Wrapper NUR für die NREM-Phase ist.

Lies `crates/memfuse-graph/src/community.rs` (Label-Propagation, deterministisch via `SimpleRng` LCG, ADR-027) — diese Komponente MUSS für die Community-Erkennung wiederverwendet werden, NICHT neu implementiert.

Prüfe die exakte, AKTUELLE Signatur von `LlmTextGenerator` in `crates/memfuse-core/src/traits/mod.rs` per `grep -n "trait LlmTextGenerator" -A 20 crates/memfuse-core/src/traits/mod.rs` — verwende AUSSCHLIESSLICH diese, nicht eine vermutete oder aus Dokumenten übernommene Fassung.

AUFGABE — Implementiere die REM-Phase additiv in `crates/memfuse-db/src/sleep_cycle.rs`:

1. `pub struct RemConfig { pub min_community_size: usize /* Default: 4 */, pub stability_cycles_required: u32 /* Default: 3 */, pub max_llm_calls_per_cycle: u32 /* Default: 10, P12-Kostenschutz */ }`.
2. `pub struct CommunityStabilityTracker { history: std::collections::HashMap<u64, u32> }` mit `pub fn observe(&mut self, community_members_hash: u64) -> u32` und `pub fn reset_if_absent(&mut self, currently_observed: &std::collections::HashSet<u64>)`.
3. `pub struct MetaChunk { pub content: String, pub abstracts_from: Vec<DocId> /* PFLICHT: len() >= 1 */, pub source_community_hash: u64, pub created_at_tx: memfuse_core::TxId, pub llm_model_id: String }`.
4. `pub struct RemPhaseResult { pub synthesized: Vec<MetaChunk>, pub deferred_community_hashes: Vec<u64> }`.
5. `pub async fn run_rem_phase(stable_communities: &[(u64, Vec<DocId>)], source_texts: &std::collections::HashMap<DocId, String>, llm: &dyn <exakter Trait-Name aus Schritt Kontext>, config: &RemConfig) -> memfuse_core::Result<RemPhaseResult>`:
   - Filtere nach `min_community_size`.
   - Begrenze LLM-Aufrufe strikt auf `max_llm_calls_per_cycle` — überschüssige qualifizierte Communities landen in `deferred_community_hashes`, NICHT verworfen.
   - Pro verarbeiteter Community: Baue einen Synthese-Prompt aus den `source_texts` der Mitglieder-Chunks, rufe die LLM-Methode auf, konstruiere `MetaChunk` mit `abstracts_from` = exakt den beteiligten `DocId`s.
   - PFLICHT: `MetaChunk.content` MUSS mit dem maschinenlesbaren Präfix `"[SYNTHESIZED FROM {n} SOURCES] "` beginnen (n = `abstracts_from.len()`), damit ein künftiger Halluzinations-Guard diese Chunks als Synthese-Produkt erkennen kann.
   - Ein einzelner fehlschlagender LLM-Aufruf darf NICHT die gesamte REM-Phase abbrechen — fange den Fehler pro Community ab, logge via `tracing::error!`, fahre fort (P2 Zero-Panic-Doctrine).
6. Integration in `crates/memfuse-db/src/sleep_cycle_executor.rs`: Erweitere `execute_sleep_cycle` (oder erstelle sie, falls sie bisher nur ein Alias für `execute_nrem_cycle` war) so, dass sie NACH der NREM-Phase optional die REM-Phase aufruft — NUR wenn eine übergebene `RemConfig` UND ein `LlmTextGenerator`-Handle vorhanden sind (Dependency Injection, kein hartcodierter Ollama-Import in `memfuse-db`, das würde die Layer-Trennung verletzen — prüfe, wie `memfuse-db` aktuell (falls überhaupt) mit LLM-Aufrufen umgeht, z. B. über einen injizierten `Arc<dyn LlmTextGenerator>` im `Collection`/`Store`-Konstruktor, und folge demselben Muster).
7. Community-Stabilität: Die REM-Phase darf eine Community ERST synthetisieren, nachdem `CommunityStabilityTracker` sie über `stability_cycles_required` aufeinanderfolgende Aufrufe hinweg als stabil erkannt hat — verdrahte den Tracker so, dass er zwischen aufeinanderfolgenden `PhysioScheduler`-Ticks (siehe Prompt 4, falls bereits gemergt — falls nicht, halte den Tracker als Feld im Executor-Zustand, das zwischen `execute_sleep_cycle`-Aufrufen persistiert) seinen Zustand behält.

TESTS (Mock-`LlmTextGenerator`, deterministisch, kein echter Netzwerkaufruf):
- Community unter `min_community_size` wird NICHT synthetisiert.
- Community mit weniger als `stability_cycles_required` beobachteten Zyklen wird NICHT synthetisiert.
- `max_llm_calls_per_cycle` wird strikt eingehalten (15 qualifizierte Communities, Limit 10 → genau 10 synthetisiert, 5 deferred).
- Jeder `MetaChunk` hat `abstracts_from.len() >= 1` und den `"[SYNTHESIZED FROM {n} SOURCES]"`-Präfix mit korrektem `n`.
- Ein simulierter LLM-Fehler bei einer Community beendet nicht die Verarbeitung der übrigen.
- Integrationstest: `execute_sleep_cycle` mit NREM+REM zusammen liefert ein konsistentes Gesamtergebnis (NREM-Deduplizierung UND REM-Synthese laufen ohne Konflikt in derselben Ausführung).

AKZEPTANZKRITERIEN:
- `cargo build -p memfuse-db` und `cargo test -p memfuse-db` grün.
- Keine Modifikation von `memfuse-graph` (nur Konsum von `community.rs`s öffentlicher API) und keine Modifikation der bestehenden NREM-Funktionen.
- Kein `unwrap()`/`panic!` im Produktionscode.
- Community-Detection selbst wird NICHT neu implementiert.

WICHTIG: Falls die exakte `LlmTextGenerator`-Signatur oder die Art, wie `memfuse-db` LLM-Handles injiziert bekommt, von der hier angenommenen Beschreibung abweicht, richte dich strikt nach dem tatsächlich vorgefundenen Code, nicht nach dieser Beschreibung.
```

---

## Prompt 7 — LongMemEval & LoCoMo als CI-Regressionsgate aktivieren (`.github/workflows/`, `benchmarks/memfuse-bench`)

```md
ROLLE: Du bist Senior Rust Engineer für Evaluationsinfrastruktur und CI/CD-Pipeline-Design, spezialisiert auf Retrieval-Qualitäts-Regressionstests.

REPOSITORY: https://github.com/tfufuz1/memfuse. Arbeitsverzeichnisse: `.github/workflows/` (neue oder erweiterte Workflow-Datei), `benchmarks/memfuse-bench/`.

KONTEXT (verifiziere selbst vor Beginn — PFLICHT):
`benchmarks/memfuse-bench/src/long_mem_eval.rs`, `benchmarks/memfuse-bench/src/locomo.rs` sowie die zugehörigen Test-Fixtures (`benchmarks/memfuse-bench/tests/fixtures/long_mem_eval_fixture.json`, `.../locomo_fixture.json`) existieren bereits — lies sie VOLLSTÄNDIG, um die vorhandene Loader-/Runner-API zu verstehen (Funktionsnamen für `load_from_jsonl`/`run_long_mem_eval` bzw. LoCoMo-Äquivalente). Diese Bausteine sind FUNKTIONAL fertig — deine Aufgabe ist NICHT, sie neu zu implementieren, sondern sie als AUTOMATISIERTES CI-Regressionsgate zu aktivieren.

Prüfe per `find .github/workflows -name "*.yml" | xargs grep -l "long_mem_eval\|locomo\|memfuse-bench"`, ob IRGENDEIN bestehender CI-Workflow diese Benchmarks bereits aufruft. Zum Zeitpunkt dieser Analyse: KEIN Treffer — die Infrastruktur existiert, wird aber nicht automatisiert ausgeführt. Dies ist laut Principal-Architect-Review (§3.6) "die dringendste verbleibende Infrastrukturarbeit", da ohne automatisiertes Regressionsgate stille Qualitätsverschlechterungen (wie der im selben Review dokumentierte F-02-Fall) bei der aktuellen Commit-Taktung unbemerkt bleiben können.

Prüfe, ob die verwendeten Fixture-Dateien (`long_mem_eval_fixture.json`, `locomo_fixture.json`) MINIMALE Test-Fixtures sind (wenige Beispiele, für Unit-Tests der Parser gedacht) oder bereits vollständige, repräsentative Datensätze — falls es sich um Minimal-Fixtures handelt, ist ein CI-Lauf gegen diese NUR ein Parser-Korrektheits-Test, KEIN echtes Recall-Regressionsgate. Kläre dies, bevor du die CI-Integration baust, und dokumentiere die Einschränkung klar, falls sie zutrifft.

AUFGABE:

1. Prüfe `benchmarks/memfuse-bench/src/main.rs` auf die vorhandenen CLI-Subbefehle (`long-mem-eval`, `locomo` — verifiziere exakte Befehlsnamen per `grep -n "\"long-mem-eval\"\|\"locomo\"\|Subcommand\|clap::" benchmarks/memfuse-bench/src/main.rs`).
2. Erstelle (oder erweitere, falls ein passender Workflow bereits existiert, z. B. `rust-ci.yml`) einen CI-Job `benchmark-regression`, der:
   a. Den Workspace baut (`cargo build -p memfuse-bench --release`).
   b. `memfuse-bench long-mem-eval --dataset <Pfad>` und `memfuse-bench locomo --dataset <Pfad>` ausführt. FALLS die echten, vollständigen Datensätze aus Lizenz-/Größengründen NICHT im Repository liegen (verifiziere dies: `find benchmarks -iname "*.jsonl" -o -iname "*long_mem_eval*.json"` außerhalb des `fixtures/`-Verzeichnisses), beschränke den CI-Lauf zunächst auf die vorhandenen Fixture-Dateien (Parser-/Pipeline-Korrektheitstest) UND dokumentiere im Workflow-Kommentar sowie im Abschlussbericht explizit, dass dies noch KEIN vollwertiges Recall-Regressionsgate ist, bis die vollständigen externen Datensätze bezogen und (z. B. als Git-LFS-Objekt oder über einen versionierten Download-Schritt) verfügbar gemacht werden — schlage LETZTERES als klar markierten, NICHT in diesem Prompt umzusetzenden Folgeschritt vor.
   c. Das Ergebnis (Accuracy/Recall-Metriken aus `LongMemEvalReport`/`LocomoReport`) in einer maschinenlesbaren Form ablegt (z. B. JSON-Artefakt, das der Workflow als CI-Artifact hochlädt).
3. Baseline-Vergleich (Kern des Regressionsgates): Implementiere einen Mechanismus, der das aktuelle Ergebnis mit einer im Repository versionierten Baseline-Datei (`benchmarks/memfuse-bench/baseline_metrics.json` [NEU]) vergleicht. Falls `overall_accuracy` (oder die jeweils relevante Kernmetrik) UM MEHR ALS EINEN KONFIGURIERBAREN SCHWELLWERT (Default: 5 Prozentpunkte relativ) UNTER der Baseline liegt: CI-Job schlägt fehl (`exit 1`), mit einer klaren, für Menschen lesbaren Fehlermeldung (Baseline-Wert, aktueller Wert, Differenz). Falls das aktuelle Ergebnis die Baseline ÜBERSTEIGT: schlage NICHT automatisch die Baseline-Datei überschreiben (das würde Regressionen verschleiern können, falls jemand versehentlich einen CI-Lauf mit degradiertem Code committed, der zufällig knapp über der alten, bereits laxen Baseline liegt) — stattdessen: gib einen INFO-Hinweis aus, dass die Baseline manuell aktualisiert werden könnte, mit dem Kommando dafür, aber erzwinge dies NICHT automatisch.
4. Implementiere das Vergleichs-/Baseline-Tooling als kleines, eigenständiges Rust-Binary oder Shell-Skript innerhalb von `benchmarks/memfuse-bench/` (z. B. `src/bin/compare_baseline.rs`), NICHT als reine CI-YAML-Bash-Logik, damit es auch lokal ausführbar und testbar ist.
5. Erstelle eine initiale `baseline_metrics.json` durch tatsächliche Ausführung der Benchmarks gegen den aktuellen Code-Stand (deine eigene erste Ausführung definiert die Startbaseline) — dokumentiere im Commit/PR klar, dass dies die INITIALE Baseline ist, kein bereits verifizierter Qualitätsstandard.

TESTS:
- Unit-Test für das Vergleichs-Tooling: Simuliertes aktuelles Ergebnis 10pp unter Baseline → Exit-Code ungleich 0, korrekte Fehlermeldung.
- Simuliertes aktuelles Ergebnis innerhalb der Toleranz → Exit-Code 0.
- Simuliertes aktuelles Ergebnis über Baseline → Exit-Code 0, INFO-Hinweis in der Ausgabe, KEINE automatische Datei-Änderung.
- CI-Workflow-Syntax ist gültiges YAML (lokal mit einem YAML-Linter oder `act`-artigem Tool prüfen, falls verfügbar; sonst zumindest sorgfältige manuelle Konsistenzprüfung gegen bestehende Workflow-Dateien im selben Repository als Stil-Vorbild).

AKZEPTANZKRITERIEN:
- Neuer/erweiterter CI-Workflow ist syntaktisch gültig und referenziert ausschließlich tatsächlich vorhandene Cargo-Befehle/Binaries.
- `cargo build -p memfuse-bench` und `cargo test -p memfuse-bench` (inklusive des neuen Vergleichs-Tools) grün.
- Abschlussbericht dokumentiert klar, ob die CI-Integration gegen die vollständigen externen Datensätze oder (mangels deren Verfügbarkeit im Repository) vorerst nur gegen die Fixture-Dateien läuft — und was der konkrete nächste Schritt wäre, um dies zu vervollständigen.
- Keine Änderung an `long_mem_eval.rs`/`locomo.rs`-Kernlogik, außer falls beim Verifizieren tatsächlich ein Bug auffällt (dann klar separat dokumentieren, nicht stillschweigend mit-ändern).

WICHTIG: Lade oder erfinde KEINE fiktiven, echten LongMemEval-/LoCoMo-Datensatz-Inhalte innerhalb dieses Prompts — falls die echten Datensätze nicht bereits im Repository liegen, ist das Beschaffen und lizenzkonforme Einbinden dieser Datensätze explizit ALS FOLGESCHRITT zu dokumentieren, nicht Teil dieser Aufgabe.
```

---

## Prompt 8 — `memfuse-candle` in die Serving-Pipeline verdrahten: Erster Schritt — Provider-Factory-Erweiterung (`memfuse-mcp`, `memfuse-core`)

```md
ROLLE: Du bist Senior Rust Engineer für ML-Inferenz-Backend-Integration, spezialisiert auf Trait-basierte Dependency Injection und Backend-Auswahlmechanismen.

REPOSITORY: https://github.com/tfufuz1/memfuse. Arbeitsverzeichnisse: `crates/memfuse-mcp/src/config.rs` (Erweiterung), `crates/memfuse-core/src/traits/` (nur falls in Schritt 2 als nötig identifiziert, sonst nicht anfassen).

KONTEXT (verifiziere selbst vor Beginn — PFLICHT):
1. `crates/memfuse-candle` existiert bereits vollständig funktional: `CandleEmbedClient` implementiert `memfuse_core::EmbeddingProvider` (`crates/memfuse-candle/src/embedding.rs`), `CandleLlmClient` implementiert `memfuse_core::LlmTextGenerator` (`crates/memfuse-candle/src/inference.rs`). Beide Trait-Implementierungen sind FERTIG — diese Aufgabe implementiert KEINE neue Inferenzlogik, sondern verdrahtet ausschließlich die bereits vorhandenen Implementierungen in die Backend-Auswahl.
2. Verifiziere per `grep -rl "memfuse_candle\|memfuse-candle" crates/memfuse-db/src crates/memfuse-ollama/src crates/memfuse-router/src crates/memfuse-mcp/src`, dass `memfuse-candle` zum Zeitpunkt deiner Bearbeitung TATSÄCHLICH noch nirgends importiert wird (falls doch bereits ein Import existiert, hat sich die Codebasis zwischenzeitlich weiterentwickelt — prüfe den Fortschritt und passe deine Aufgabe entsprechend an, statt blind fortzufahren).
3. `crates/memfuse-mcp/src/config.rs` enthält bereits eine funktionierende Provider-Auswahl-Factory für Embeddings: `create_embedding_provider(provider_type: &str, ...) -> Result<Arc<dyn EmbeddingProvider>, MemFuseError>`, mit `match provider_type { "ollama" => ..., "onnx" => ..., ... }`. Lies diese Funktion VOLLSTÄNDIG als Vorbild für Konsistenz (Fehlerbehandlung, Feature-Gating-Muster mit `#[cfg(feature = "onnx")]`).
4. Es existiert AKTUELL KEINE äquivalente Factory-Funktion für `LlmTextGenerator` (nur für `EmbeddingProvider`) — verifiziere dies per `grep -rn "fn.*-> .*dyn LlmTextGenerator\|create_llm" crates/memfuse-mcp/src crates/memfuse-router/src crates/memfuse-agent/src`. Prüfe, WIE `OllamaClient`/ein `LlmTextGenerator`-Implementierer aktuell in `memfuse-router`/`memfuse-agent`/`memfuse-mcp` instanziiert wird (vermutlich direkte, hartkodierte Konstruktion ohne Auswahlmechanismus) — das ist der zweite Teil dieser Aufgabe.

AUFGABE — Schaffe die Backend-Auswahl-Infrastruktur, OHNE bereits den vollständigen "Ollama-Ausstieg" (Standardumschaltung) vorzunehmen — dieser Prompt liefert die WAHLMÖGLICHKEIT, keine Verhaltensänderung des Defaults:

1. **Erweiterung von `create_embedding_provider()` in `crates/memfuse-mcp/src/config.rs`** (additiv, bestehende Zweige NICHT verändern):
   ```rust
   #[cfg(feature = "candle")]
   "candle" => {
       let model_dir = candle_model_dir.ok_or_else(|| {
           MemFuseError::InvalidInput(
               "candle_model_dir is required when embedding provider is 'candle'".to_string(),
           )
       })?;
       let quantization = /* aus Config ableiten, Default sinnvoll wählen, z.B. Q4KM */;
       let embedder = memfuse_candle::embedding::CandleEmbedClient::new(model_dir, quantization)
           .map_err(|e| MemFuseError::Internal(format!("Failed to load Candle embed model: {e}")))?;
       Ok(Arc::new(embedder))
   }
   #[cfg(not(feature = "candle"))]
   "candle" => Err(MemFuseError::CapabilityUnsupported {
       capability: "candle embedding backend".to_string(),
       reason: "memfuse-mcp was built without the 'candle' feature".to_string(),
   }),
   ```
   Passe die exakte Signatur von `CandleEmbedClient::new()` an die TATSÄCHLICHE, in `crates/memfuse-candle/src/embedding.rs` vorgefundene API an (nicht die obige, möglicherweise leicht abweichende Beispielsignatur ungeprüft übernehmen). Erweitere die Parameter von `create_embedding_provider()`/`build_provider()` um die für Candle nötigen zusätzlichen Optionen (`candle_model_dir: Option<&Path>`, ggf. Quantisierungs-Auswahl) nach demselben Muster wie das bereits vorhandene `onnx_model_path`-Optional-Parameter-Muster.
   Füge `memfuse-candle` als OPTIONALE Dependency (`optional = true`) in `crates/memfuse-mcp/Cargo.toml` hinzu, mit einem neuen Cargo-Feature `candle = ["dep:memfuse-candle"]`, analog zum bereits vorhandenen `onnx`-Feature-Muster — verifiziere das exakte bestehende Muster für `onnx` in derselben `Cargo.toml` und übernimm es konsistent.

2. **Neue, analoge Factory-Funktion für `LlmTextGenerator`**: Da laut Kontext-Punkt 4 aktuell KEINE zentrale Auswahl-Factory existiert, schaffe eine an geeigneter Stelle — bevorzugt ebenfalls in `crates/memfuse-mcp/src/config.rs` (Konsistenz-Ort mit der Embedding-Factory), ALTERNATIV in `crates/memfuse-router/src/` falls die Recherche in Schritt 4 des Kontexts ergibt, dass `memfuse-router` der fachlich passendere, bereits etablierte Ort für LLM-Backend-Instanziierung ist (triff diese Entscheidung nach eigener Recherche, dokumentiere sie):
   ```rust
   pub fn create_llm_text_generator(
       provider_type: &str,
       ollama_url: &str,
       llm_model: &str,
       candle_model_dir: Option<&Path>,
   ) -> Result<Arc<dyn LlmTextGenerator>, MemFuseError> {
       match provider_type.to_lowercase().trim() {
           "ollama" => { /* bestehende OllamaClient-Konstruktion, aus dem Ort übernehmen, an dem sie aktuell hartkodiert erfolgt */ }
           #[cfg(feature = "candle")]
           "candle" => { /* analog zu Schritt 1 */ }
           #[cfg(not(feature = "candle"))]
           "candle" => Err(MemFuseError::CapabilityUnsupported { /* ... */ }),
           other => Err(MemFuseError::InvalidInput(format!("Unknown LLM provider: {other}"))),
       }
   }
   ```
   Ersetze JEDE aktuell hartkodierte `OllamaClient::new(...)`-Instanziierung, die für Produktionscode (nicht Tests) relevant ist, durch einen Aufruf dieser neuen Factory-Funktion — NUR an Stellen, an denen dies ohne größere Signatur-Kaskaden-Änderungen möglich ist; falls eine Ersetzung eine tiefgreifende Refaktorierung nach sich ziehen würde, dokumentiere diese Stelle stattdessen als "identifiziert, aber außerhalb des Scopes dieses Prompts" statt eine riskante Großumstellung vorzunehmen.

3. **Standardverhalten bleibt UNVERÄNDERT**: Der Default-Provider bleibt `"ollama"` an JEDER Stelle, an der aktuell kein expliziter Provider konfiguriert ist — diese Aufgabe fügt eine WAHLMÖGLICHKEIT hinzu, sie vollzieht NICHT den "Ollama-Ausstieg" (das bleibt ein bewusster, separater, späterer Schritt gemäß Roadmap-Priorität P3, nachdem die LongMemEval-Baseline aus Prompt 7 als Qualitäts-Absicherung für einen Backend-Wechsel zur Verfügung steht).

TESTS (`crates/memfuse-mcp/src/config.rs` bzw. neuer Ort aus Schritt 2, Testmodul erweitern):
- `create_embedding_provider("candle", ...)` mit Feature aktiviert und gültigem `candle_model_dir` → liefert erfolgreich einen `Arc<dyn EmbeddingProvider>` (Mock-Modellverzeichnis oder `#[ignore]`-markierter Test mit Hinweis auf benötigtes echtes Modell, falls kein Mock-GGUF für Tests praktikabel ist — verifiziere, ob `memfuse-candle` bereits eine Mock-Implementierung für Tests bereitstellt, wie in seiner eigenen Test-Suite `MockEmbedModel`/`MockCandleModel` erwähnt, und nutze diese wo immer möglich statt echte Modelldateien zu benötigen).
- `create_embedding_provider("candle", ...)` OHNE `candle`-Feature kompiliert → liefert `CapabilityUnsupported`-Fehler, kein Panic, kein Compile-Fehler.
- `create_embedding_provider("candle", ...)` mit Feature aktiviert, aber `candle_model_dir = None` → `InvalidInput`-Fehler mit klarer Meldung.
- Bestehende Tests für `"ollama"`/`"onnx"`-Zweige bleiben unverändert grün (Regressionsschutz — Beweis, dass der Default-Pfad nicht angetastet wurde).
- Analoge Tests für `create_llm_text_generator()`.

AKZEPTANZKRITERIEN:
- `cargo build -p memfuse-mcp` (ohne `candle`-Feature, Standardfall) grün, UNVERÄNDERTES Verhalten.
- `cargo build -p memfuse-mcp --features candle` grün.
- `cargo test -p memfuse-mcp --features candle` grün, inklusive neuer Tests.
- `cargo build --workspace` (Default-Features) bleibt insgesamt grün.
- Kein `unwrap()`/`panic!` im Produktionscode.
- Abschlussbericht dokumentiert explizit: (a) welche hartkodierten `OllamaClient`-Instanziierungsstellen ersetzt wurden, (b) welche identifiziert, aber aus Scope-Gründen NICHT ersetzt wurden (für einen Folge-Prompt), (c) ob die LLM-Factory in `memfuse-mcp` oder `memfuse-router` platziert wurde und warum.

WICHTIG: Diese Aufgabe ändert NICHT das Standardverhalten des Systems (weiterhin Ollama), implementiert KEINE neue Inferenzlogik in `memfuse-candle` selbst (das ist bereits fertig), und vollzieht NICHT den vollständigen "Sovereign Core"-Ollama-Ausstieg — das bleibt bewusst ein späterer, von der LongMemEval-Baseline (Prompt 7) abhängiger Schritt.
```

---

## Abhängigkeits- und Ausführungshinweise

| Prompt | Dateien (Kernbereich) | Harte Abhängigkeit | Parallel zu allen anderen? |
|---|---|---|---|
| 1 | `memfuse-db/src/replicator.rs` (löschen), `lib.rs` | keine | Ja |
| 2 | `memfuse-index/src/diskann.rs` (nur Kommentar) | keine | Ja |
| 3 | `docs/decisions/`, ggf. CI-YAML | keine | Ja |
| 4 | `memfuse-db/src/physio_scheduler.rs`, `physio_config.rs` (neu), `reaper.rs` (Deprecation) | Funktioniert unabhängig von 5 (Platzhalter-Hook), nutzt aber dieselbe `ReplicatorState`-Instanz wie Prompt 1 bestätigt — **nach Prompt 1 mergen empfohlen, nicht zwingend** | Bedingt (siehe Hinweis) |
| 5 | `memfuse-graph/src/synaptic_buffer.rs` (neu), `synaptic.rs` (Erweiterung), `memfuse-db/src/fusion.rs`+`search.rs` (additiv) | keine harte Abhängigkeit zu 4 (Platzhalter-Hook-Muster in 4 kompatibel) | Ja |
| 6 | `memfuse-db/src/sleep_cycle.rs` (additiv), `sleep_cycle_executor.rs` | keine | Ja |
| 7 | `.github/workflows/`, `benchmarks/memfuse-bench/` | keine | Ja |
| 8 | `memfuse-mcp/src/config.rs`, `Cargo.toml` | keine | Ja |

**Empfehlung bei voller Parallelität:** Alle 8 Prompts können gleichzeitig an Jules gesendet werden. Die einzige weiche Kopplung (Prompt 1 ↔ Prompt 4, beide berühren den `ReplicatorState`-Konsum-Pfad, aber unterschiedliche Dateien) führt im schlimmsten Fall zu einem trivialen Merge-Konflikt in `crates/memfuse-db/src/lib.rs` (Modul-Registrierungszeilen), nicht zu einem funktionalen Konflikt.
