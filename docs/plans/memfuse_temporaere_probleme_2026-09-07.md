# MemFuse — Temporäre Probleme & Zwischenstand
## Live-Audit-Nachtrag zu den drei Vorgänger-Dokumenten · Stand HEAD `ade2f12f`

> **Dokument-Typ:** Vergängliches Arbeitsdokument. Kein Ersatz für `DECISIONS.md` (P6). Hält den *aktuellen* Bearbeitungsstand fest — wird bei jedem substanziellen HEAD-Fortschritt neu geschrieben, nicht fortlaufend gepflegt.
> **Repository:** `tfufuz1/memfuse`, frisch geklont, HEAD `ade2f12f9ac574ded7abb16522ddf6d77c4b7a87` (07.09.2026, 16:35:17 +02:00)
> **Vorgänger-HEADs der drei Eingangsdokumente:** `36ad007a` (Spec v4.0 / Präzisierung v4.1) → `bb099dc2` (Principal-Review) → `ade2f12f` (dieses Dokument). Zeitdifferenz zum letzten Audit: < 3 Stunden, 6 Commits.
> **Methodik:** Jeder Befund ist gegen den frisch geklonten Code neu verifiziert (`grep`, `git log -p`, Dateiinspektion). Keine Übernahme unverifizierter Aussagen aus den Eingangsdokumenten.
> **Kernbefund dieser Prüfung:** Zwei der drei Eingangsdokumente sind **innerhalb von Stunden ihrer Erstellung bereits überholt** — praktisch alle G0/H1-Maßnahmen aus v4.1 sind umgesetzt, inklusive der im Principal-Review selbst noch als offen geführten KV-Bridge-Sicherheitsschicht. Gleichzeitig wurden bei dieser Prüfung **zwei neue, bislang in keinem Dokument erfasste, CI/Build-brechende Regressionen** gefunden, die exakt aus der hohen Commit-Taktung entstehen, vor der Dokument 3 warnt.

---

## §0 Disposition dieses Dokuments

| § | Inhalt |
|---|---|
| §1 | **P0 — Build-brechend:** Duplikat-Konstanten in `diskann.rs` (Merge-Kollision zweier Parallel-PRs) |
| §2 | **P0 — CI-brechend:** DAG-Layer-Regressionstest kennt `memfuse-kv-bridge` nicht |
| §3 | Bestätigt behoben seit `36ad007a`/`bb099dc2` (Kurzbeleg, keine Wiederholung der Herleitung) |
| §4 | Falsch-positive bzw. veraltete Befunde in den Eingangsdokumenten (Korrektur) |
| §5 | Tatsächlich noch offene Punkte (nach Neuprüfung) |
| §6 | Neue Detailbefunde ohne Build-Bruch (Latenzbudget, stale Kommentare) |
| §7 | Priorisierte Maßnahmenliste |
| §8 | Governance-Konsequenz: Duplicate-Symbol-Gate als neues CI-Gate |

---

## §1 P0 — Build-brechend: Doppelte Top-Level-Konstanten in `diskann.rs`

**Fundstelle:** `crates/memfuse-index/src/diskann.rs:26–36`

```rust
const DISKANN_MAGIC: &[u8; 4] = b"DANN";
const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"DFTR";              // Zeile 27
const DISKANN_INTEGRITY_KEY: &[u8; 32] = b"MEMFUSE_DISKANN_INTEGRITY_KEY___";  // Zeile 28
const DISKANN_VERSION: u16 = 1;
const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"FOOT";               // Zeile 30 — DUPLIKAT
const DISKANN_INTEGRITY_KEY: &[u8; 32] = b"memfuse-diskann-integrity-key-32"; // Zeile 31 — DUPLIKAT
```

**Diagnose:** `rustc` gibt hierfür zwingend `E0428: the name 'DISKANN_FOOTER_MAGIC' is defined multiple times` — kein Sonderfall, kein Feature-Flag-Pfad, kein `cfg`-Gate trennt die beiden Definitionen. `crates/memfuse-index` kompiliert an HEAD `ade2f12f` **nicht**. Da `memfuse-db`, `memfuse-router`, `memfuse-bench` transitiv von `memfuse-index` abhängen, ist `cargo build --workspace` vollständig blockiert.

**Ursachenanalyse (git-forensisch verifiziert):**

| Commit | Zeit | Inhalt |
|---|---|---|
| `307df50` | 16:31:01 | fügt `DISKANN_FOOTER_MAGIC = b"DFTR"` + `DISKANN_INTEGRITY_KEY = ...KEY___` ein (Integritäts-Footer, Variante A) |
| `eb0e3ef` | 16:31:24 | *„feat(index): diskann integrity check and hnsw fallback policy (#1707)"* — fügt **erneut** `DISKANN_FOOTER_MAGIC = b"FOOT"` + `DISKANN_INTEGRITY_KEY = ...key-32` ein (Integritäts-Footer, Variante B) |

Beide Commits implementieren **dieselbe fachliche Anforderung** (HMAC-Integritäts-Footer für DiskANN-Persistenzdateien) unabhängig voneinander, 23 Sekunden auseinander. Da die Diffs textuell nicht überlappen (reine Zeileneinfügung, kein gemeinsamer Kontext-Hunk), erkennt der Merge sie nicht als Konflikt — beide Textblöcke werden anstandslos übernommen. Das Ergebnis ist ein semantisch redundantes, syntaktisch ungültiges Modul. Beide Varianten sind an vier Stellen verdrahtet (Zeilen 184, 195, 1096, 1144, 1984 nutzen `DISKANN_FOOTER_MAGIC`/`DISKANN_INTEGRITY_KEY` — welche der beiden Definitionen tatsächlich gemeint war, ist aus dem Diff allein nicht mehr rekonstruierbar).

**Fix (P0, < 15 Minuten, aber Entscheidung erforderlich):**

1. Git-Historie beider Commits inhaltlich vergleichen: `git show 307df50 -- crates/memfuse-index/src/diskann.rs` vs. `git show eb0e3ef -- crates/memfuse-index/src/diskann.rs` — prüfen, ob eine der beiden Varianten den vollständigeren Footer-Schreib-/Lesepfad implementiert (Magic-Bytes allein sind kosmetisch, die HMAC-Verifikationslogik dahinter kann divergieren).
2. Eine Variante behalten, die andere vollständig entfernen (nicht nur die Konstante — auch etwaige zugehörige Testfälle beider Commits deduplizieren).
3. Falls beide Varianten inhaltlich gleichwertig sind: Variante aus `eb0e3ef` behalten (jüngerer Commit, vermutlich informierter über den Zwischenstand), da `magic: &[u8;4] = b"FOOT"` selbsterklärender ist als `b"DFTR"`.
4. Regressionstest ergänzen: Rundtrip-Test schreibt Footer, lädt Footer, verifiziert HMAC — deckt beide vormals unentdeckt gebliebenen Implementierungen ab.
5. **Governance-Konsequenz siehe §8** — dieser Fehlerklasse muss strukturell vorgebeugt werden, ein Einzel-Fix reicht nicht.

---

## §2 P0 — CI-brechend: DAG-Layer-Regressionstest kennt `memfuse-kv-bridge` nicht

**Fundstelle:** `xtask/src/main.rs`, `test_workspace_crate_layers_regression`

Der exakte HEAD-Commit `ade2f12f` selbst (*„feat: implement memfuse-kv-bridge security layer (#1713)"*) fügt `crates/memfuse-kv-bridge` als neues Workspace-Mitglied hinzu, aktualisiert aber **nicht** die `expected_layers`-HashMap im DAG-Regressionstest:

```rust
assert_eq!(crates.len(), 18, "Expected 18 workspace crates");
let expected_layers: HashMap<&str, u8> = [
    ("memfuse-core", 0), ("memfuse-calibration", 1), ("memfuse-candle", 1),
    ("memfuse-checkpoint", 1), ("memfuse-crypto", 1), ("memfuse-graph", 1),
    ("memfuse-text", 1), ("memfuse-embed", 2), ("memfuse-index", 2),
    ("memfuse-ollama", 2), ("memfuse-store", 2), ("memfuse-db", 3),
    ("memfuse-bench", 4), ("memfuse-router", 4), ("memfuse-tauri", 4),
    ("memfuse-agent", 5), ("memfuse-mcp", 6),
    // memfuse-kv-bridge FEHLT — 17 Einträge, aber 18 Crates werden erwartet
].into_iter().collect();
```

`get_workspace_crates()` liefert nach Hinzunahme von `memfuse-kv-bridge` 18 Crates (`assert_eq!` bliebe grün), aber die Schleife `expected_layers.get(c.name.as_str()).unwrap_or_else(|| panic!("Unexpected workspace crate: {}", c.name))` **panict** deterministisch für `memfuse-kv-bridge`, sobald `cargo test -p xtask` läuft. `xtask check-dag` selbst (Produktionslogik, getrennt vom Test) ist davon nicht betroffen — nur der Regressionstest.

**Fix (P0, 2 Minuten):** Eintrag `("memfuse-kv-bridge", 1)` ergänzen. Layer-Herleitung: `memfuse-kv-bridge/Cargo.toml` hat als einzige Workspace-Abhängigkeit `memfuse-core` (Layer 0) → `1 + max(0) = 1`, konsistent mit `memfuse-calibration`, `memfuse-candle`, `memfuse-checkpoint`, `memfuse-crypto`, `memfuse-graph`, `memfuse-text`.

**Bewertung:** Dieselbe Fehlerklasse wie §1 — eine Änderung (neuer Crate) hat eine entfernte, nicht offensichtlich gekoppelte Stelle (hartkodierte Erwartungstabelle in einem Test) nicht mitgezogen. Kein Compiler-Fehler, aber ein deterministisch fehlschlagender Test blockiert denselben CI-Pfad wie §1.

---

## §3 Bestätigt behoben seit `36ad007a` / `bb099dc2` (Kurzverifikation, keine Wiederholung)

Alle folgenden Punkte wurden gegen HEAD `ade2f12f` per `grep`/Dateiinspektion neu bestätigt — Herleitung siehe Vorgänger-Dokumente, hier nur der aktuelle Fundort:

| Punkt | Fundort HEAD `ade2f12f` |
|---|---|
| BM25 `+1`-Glättung (A19) | `memfuse-text/src/bm25.rs:91-96`, inkl. Proptest `prop_bm25_idf_non_negative_for_high_df` |
| Rerank-Kandidatenfenster (A14, statisch `k*3`) | ersetzt durch `RerankPidController` (`memfuse-db/src/homeostat.rs`), `k_min=50` hart über `arXiv:2604.01733`-Recall-Knee begründet |
| DiskANN `persist_delta()` (A16) | `diskann.rs:570`, inkl. WAL-rückgestütztem `pending.wal` + `recover_pending_delta()` — Datenverlustrisiko aus früherer Iteration behoben (siehe aber §6.2) |
| `TenantId` / `ConfigFingerprint` / `DeletionProof` (A4–A7) | `memfuse-core/src/types/domain.rs`, `memfuse-crypto/src/deletion_proof.rs` |
| `memfuse-calibration` Crate | im Workspace, Layer 1, Isotonic + Platt + Conformal |
| F-02 Nucleation-Trigger | **nicht** stillschweigend gegen Veto gemergt — `VETOES.md` + ADR-063 klassifizieren es explizit als `conditionally_accepted` (Tombstone-Pruning-Variante), `physio-nucleation` bleibt non-default, `tests/nucleation_recall.rs` misst Recall-Regression >5pp als harte Assertion |
| KV-Cache-Bridge-Sicherheitsschicht | `crates/memfuse-kv-bridge/` existiert (`segment.rs`, `eviction_worker.rs`, `store.rs`) — **inklusive** des im Principal-Review (§3.3) geforderten dedizierten `EvictionWorker` mit eigenem Thread, getrennt von `emergency_wipe()` |
| Governance-Lücke „Veto ↔ Commit" (Review §4) | `VETOES.md` + `xtask check-vetoes` existieren bereits |
| `async_trait`-Migration | 0 Treffer im Workspace, vollständig auf RPITIT/`BoxFuture` migriert |
| LongMemEval-Integration | `benchmarks/memfuse-bench/src/long_mem_eval.rs` + `tests/external_benchmarks_test.rs` |

---

## §4 Korrektur veralteter/falsch-positiver Befunde aus den Eingangsdokumenten

### §4.1 „`memfuse-py` fehlt im Workspace" (A20, Review §2) — **kein Defekt, bewusste Architektur**

`crates/memfuse-py/Cargo.toml` enthält ein eigenständiges `[workspace]`-Manifest (Zeile 35), macht es also *absichtlich* zu einem eigenen Cargo-Workspace, nicht zu einem Mitglied des Haupt-Workspace. Grund, im Code selbst dokumentiert (`lib.rs:293-296`): Nur so lässt sich für die PyO3-Bindings `panic = "unwind"` setzen, während der Haupt-Workspace `panic = "abort"` im Release-Profil fährt — sonst würde `catch_unwind()` an der FFI-Grenze wirkungslos, ein Rust-Panic würde den CPython-Interpreter per SIGABRT mitreißen. `.github/workflows/rust-ci.yml` deckt `memfuse-py` explizit über separate `--manifest-path`-Aufrufe ab (`maturin build`, `cargo clippy`, vermutlich `cargo test`) — der Crate ist CI-geprüft, nur eben nicht über den Haupt-Workspace-Build. **Kein Merge-Ausschluss, sondern eine sauber begründete Panic-Strategie-Isolation.** Anhang A der Spec sollte diesen Punkt streichen bzw. explizit als „ADR-würdig dokumentierte Ausnahme" statt als technische Schuld führen.

### §4.2 DiskANN „read-only, kein inkrementeller Pfad" — veraltet

`insert()` (`diskann.rs:1626`) schreibt heute WAL-first in `pending.wal`, aktualisiert den In-Memory-Puffer und triggert bei Schwellwert `persist_delta()`. Nur `delete()` bleibt ohne HNSW-Fallback ein `Err` — das ist eine bewusste, im Code dokumentierte Fachentscheidung (Löschung erfordert Graph-Rewiring, das DiskANN architektonisch nicht leistet), keine offene Lücke.

---

## §5 Tatsächlich noch offene Punkte (nach Neuprüfung, Stand `ade2f12f`)

| Lücke | Live-Verifikation | Priorität |
|---|---|---|
| `memfuse-candle` nicht in Serving-Pipeline verdrahtet | `grep -rl memfuse_candle crates/memfuse-db/src crates/memfuse-ollama/src crates/memfuse-router/src` → **kein Treffer** | Hoch — Fundament für Säule I (Sovereign Core) steht, Ollama-Ausstieg nicht vollzogen |
| Cascading-Invalidation Supersedes → Graph-Kante | `grep -rl "tombstone_edges_for_doc\|cascade_invalidat"` → **kein Treffer** | Mittel-Hoch — PathRAG ist produktiv im Suchpfad, tote Kanten sind eine reale Halluzinationsquelle, nicht mehr akademisch |
| Edge-Vektoren als 5. Fusionssignal | kein `EdgeVector`/`edge_embedding` im Code | Niedrig — kein Wettbewerbsdruck, der eine Sofort-Umsetzung rechtfertigt |
| `ImportanceEmbeddingClassifier` (k-NN-Distillation) | bewusst zurückgestellt zugunsten LLM-Konsolidierung (`memfuse-ollama/src/importance.rs`) | Niedrig — richtige Reihenfolge: erst Benchmark-Fundament (LongMemEval, jetzt vorhanden), dann Distillation |
| Checkpoint-Konsolidierung (3 Abstraktionen → 1 Fassade) | nicht neu verifiziert (außerhalb des heutigen Scopes) | Muss vor nächstem Layer-1-Feature geprüft werden (P10-Pflicht) |

---

## §6 Neue Detailbefunde ohne Build-Bruch

### §6.1 `persist_delta()` blockiert synchron im Hot-Path bei Schwellwert-Überschreitung

`diskann.rs:1638-1641`: `insert()` ruft bei `count >= PENDING_FLUSH_THRESHOLD` (50) **synchron** `self.persist_delta().await?` auf — nicht als `tokio::spawn`-Hintergrundaufgabe, wie in der Präzisierung v4.1 (§1.4) als Zieldesign vorgeschlagen. Bei `pending_ratio > 10%` löst `persist_delta()` einen vollständigen Vamana-Rebuild aus (`load_all_vectors_from_mmap()` + `build()`). Der 50. `insert()`-Aufruf in jeder Charge trägt damit potenziell die volle Rebuild-Latenz — ein P11-Verstoß (Latenzbudget-Pflicht im Hot-Path, kein hartes Deadline-Abbruchpfad für diesen spezifischen Pfad vorhanden).

**Empfehlung:** `persist_delta()`-Trigger analog zum bereits vorhandenen `EvictionWorker`-Muster (kv-bridge) als dedizierten Hintergrund-Task mit Channel-Queue ausführen, `insert()` gibt sofort zurück. Aufwand: klein, da das Muster im selben HEAD bereits einmal korrekt implementiert wurde (Wiederverwendung, P10).

### §6.2 Stale Risiko-Kommentar in `diskann.rs:32-35`

Der Kommentar über `PENDING_FLUSH_THRESHOLD` warnt noch vor Datenverlust bei Absturz innerhalb des 50-Insert-Fensters — dieses Risiko wurde durch den WAL-rückgestützten `pending.wal`-Pfad (Commit `72243f2`, „Fix DiskANN data loss with WAL-backed pending buffer and recovery") bereits behoben. Der Kommentar wurde beim Fix nicht aktualisiert. Kein funktionaler Fehler, aber ein Dokumentations-Drift, der bei zukünftiger Bearbeitung zu falschen Annahmen führen kann. Korrektur: Kommentar durch Verweis auf `recover_pending_delta()` und die WAL-Garantie ersetzen.

### §6.3 F-02: 30-Tage-Beobachtungsfrist läuft — Wiedervorlage terminieren

`VETOES.md` klassifiziert F-02 als `conditionally_accepted … bis Recall-Regressionstest 30 Tage stabil`. `last_verified: 2026-09-07`. Es existiert aktuell kein Tracking-Mechanismus (Issue, Kalendereintrag, CI-Datumsprüfung), der diese Frist automatisch überwacht. Empfehlung: `xtask check-vetoes` um eine Datumsprüfung erweitern, die nach Ablauf der Frist eine explizite Re-Evaluation erzwingt (Warnung statt Hard-Fail, um CI nicht ungeplant zu blockieren).

---

## §7 Priorisierte Maßnahmenliste

1. **P0 — sofort, vor jedem weiteren Commit:** `diskann.rs`-Duplikat auflösen (§1). Workspace kompiliert aktuell nicht.
2. **P0 — sofort:** `expected_layers`-Map um `memfuse-kv-bridge` ergänzen (§2). 2-Minuten-Fix.
3. **P0 — diese Woche:** Duplicate-Symbol-CI-Gate einführen (§8) — verhindert Wiederholung von §1/§2-Fehlerklasse.
4. **P1 — diese Woche:** `persist_delta()`-Hot-Path-Blockierung entkoppeln (§6.1), Wiederverwendung des `EvictionWorker`-Musters.
5. **P1 — diese Woche:** Cascading-Invalidation Supersedes→Graph-Kante (§5) — PathRAG-Korrektheit ist jetzt produktiv betroffen.
6. **P2 — nächste 2 Wochen:** `memfuse-candle` in Serving-Pipeline verdrahten (Ollama-Ausstieg vollziehen).
7. **P2 — nächste 2 Wochen:** `VETOES.md`-Fristüberwachung für F-02 automatisieren (§6.3).
8. **P3 — mittelfristig:** Checkpoint-Konsolidierung (3→1 Fassade) prüfen, bevor neue Layer-1-Arbeit beginnt (P10-Pflicht).
9. **Dokumentationspflicht (laufend):** Anhang A der Spec v4.0 um §4.1/§4.2 dieses Dokuments korrigieren (`memfuse-py`, DiskANN-Insert-Status sind keine technischen Schulden mehr).

---

## §8 Governance-Konsequenz: Duplicate-Symbol-Gate

Beide P0-Befunde (§1, §2) haben dieselbe Struktur: zwei zeitlich nah beieinanderliegende, textuell nicht-überlappende Änderungen an derselben Datei, die von Git anstandslos gemergt werden, obwohl sie semantisch kollidieren. Ein Merge-Konflikt-Marker entsteht dabei nie, weil Git nur auf Zeilenebene, nicht auf Symbolebene prüft. Bei einer Commit-Taktung von 76 Commits/Tag (siehe Principal-Review §3.6) ist dies kein Einzelfall, sondern ein strukturelles Risiko.

**Empfehlung:** `xtask check-duplicate-symbols` als neues CI-Gate, das für jede geänderte `.rs`-Datei im PR-Diff prüft, ob `rustc --edition 2021 -Z parse-only` (oder einfacher: ein Regex-basierter Vorab-Scan auf doppelte `^(pub )?(const|struct|fn|enum) NAME`-Deklarationen pro Modul) fehlschlägt, **bevor** der eigentliche Compile-Schritt in CI läuft — das verkürzt die Fehlerdiagnose von „ganzer Workspace-Build rot" auf „eine Datei, ein Symbol, eine Zeile". Ergänzend: Ein leichtgewichtiger `cargo check --workspace`-Lauf unmittelbar nach jedem Merge in den Hauptzweig (nicht nur bei PR-Erstellung) hätte beide P0-Funde noch am selben Nachmittag automatisch gemeldet, statt sie einer externen Prüfung zu überlassen.

---

*Dieses Dokument ersetzt keinen ADR. Es dokumentiert einen Zwischenstand und wird mit dem nächsten substanziellen HEAD-Fortschritt neu erstellt, nicht fortgeschrieben. Normative Wahrheitsquelle für Architektur bleibt `DECISIONS.md`; normative Schnittstellenwahrheit ist das begleitende Dokument `memfuse_schnittstellenspezifikation_v5.md`.*
