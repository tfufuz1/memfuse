# MemFuse — Principal-Architect-Review

> **Rolle:** Principal Senior Rust Architect (Storage/Concurrency · IR/Vektorsuche · ML-Systemdesign für Agenten-Infrastruktur)
> **Repository:** `tfufuz1/memfuse`, live geklont am 2026-09-07, **aktualisiert per `git pull` am selben Tag, 22:18 Uhr**
> **HEAD zum Zeitpunkt dieser Aktualisierung:** `738c0ace` (07.09.2026, 22:18 Uhr) — 1.284 Commits, ~126.600 LOC Rust, 18 Workspace-Crates
> **Vorheriger Prüfstand (erste Fassung dieses Dokuments):** `bb099dc2` (14:28 Uhr) — seither **36 weitere Commits, +6.900 LOC** in knapp 8 Stunden
> **Referenz-Dokumente:** die 18 angehängten Spezifikations-/Analyse-/Planungsdokumente (v1.0 → v4.1, Jules-Prompts, ArXiv-Synthesen, PRD, Wettbewerbsvergleich)
> **Methodik:** Jede zentrale Behauptung wurde gegen den frisch gepullten Code neu verifiziert (`grep`, Datei-Inspektion, Git-Log, Test-Quellcode) — nicht aus Commit-Messages übernommen.

---

## Update-Hinweis (22:18 Uhr) — dieses Dokument wurde bereits einmal wirksam

Die Commits der letzten acht Stunden zeigen etwas Ungewöhnliches: **Sechs von sieben P0/P1/P2-Empfehlungen aus Abschnitt 5 der Erstfassung dieses Reviews sind bereits umgesetzt** — inklusive eines ADR (`ADR-063`), der wörtlich auf „externe Analyse-Session 2026-09-07" verweist und exakt die in §1 dieser Review beschriebene Argumentationskette (Tombstone-Pruning ≠ aktives Re-Wiring, aber eigenes Grad-Verlust-Risiko) übernimmt. Details je Empfehlung in Abschnitt 1 und 5 (jetzt als „Status" markiert). Dieses Dokument ist entsprechend als **Verlaufsprotokoll** zu lesen, nicht nur als Momentaufnahme: Abschnitte, die sich seit 14:28 Uhr geändert haben, sind mit **[UPDATE 22:18]** markiert; unveränderte Bewertungen aus der Erstfassung bleiben stehen, wenn sie weiterhin zutreffen.

---

## 0. Kurzurteil — das eigentlich bemerkenswerte Ergebnis dieser Prüfung

Das jüngste und gründlichste Ihrer Dokumente (`memfuse_architektur_praezisierung.md`, v4.1) wurde **heute Morgen** gegen HEAD `36ad007a` (102.199 LOC) geschrieben und dokumentiert in seiner Code-Audit-Matrix §0 zwölf als „nicht existent" oder „kritisch" markierte Lücken (`TenantId`, `ConfigFingerprint`, `DeletionProof`, `memfuse-calibration`, `PathRAGEngine`, `memfuse-candle`, F-01–F-11-Physio-Features etc.) sowie einen G0/H1-Sprintplan dafür.

**Zum Zeitpunkt dieser Prüfung, wenige Stunden später, sind praktisch alle G0- und die meisten H1-Punkte bereits im Code vorhanden** — HEAD ist um 86 Commits und ~17.500 LOC weitergewandert. Verifiziert per Live-Grep:

| Aus §0 als fehlend markiert | Jetzt im Code? | Fundstelle |
|---|---|---|
| `TenantId` | ✅ vorhanden | `memfuse-core/src/types/domain.rs` |
| `ConfigFingerprint` | ✅ vorhanden, in Router **und** Calibration **und** Reranker verdrahtet | `router/profile.rs`, `calibration/*.rs`, `embed/reranker.rs` |
| `DeletionProof` | ✅ vorhanden | `memfuse-crypto/src/deletion_proof.rs` |
| `memfuse-calibration` Crate | ✅ existiert, Isotonic + Platt + Replicator-Dynamics | `crates/memfuse-calibration/` |
| `PathRAGEngine` | ✅ `path_rag.rs`, in Graph + DB-Suche + Query-Builder verdrahtet | `memfuse-graph/src/path_rag.rs` |
| `memfuse-candle` Crate | ✅ existiert (GGUF-Loader, Inferenz, Embedding, Model-Registry) | `crates/memfuse-candle/src/` |
| BM25-IDF-Bug (A19) | ✅ behoben (`1 + …`-Glättung jetzt im Code) | `memfuse-text/src/bm25.rs:95` |
| DiskANN `persist_delta()` (A16) | ✅ implementiert, inkl. Pending-Buffer + atomarem Rename + Tests | `memfuse-index/src/diskann.rs:510` |
| Reranking-Pool-Fix (A14) | ✅ ersetzt durch **PID-Regler** (`RerankPidController`) statt statischem `k*3` | `memfuse-db` (Commits #1699, #1702) |
| F-01 Thermostat | ✅ | `memfuse-db/src/thermostat.rs` |
| F-04 ImmunMemory | ✅ | `memfuse-graph/src/immune.rs` |
| F-09 Resonanz-Fusion | ✅ | Commit #1698 |
| Temporal-Validity-Post-Filter (PRIO 1 aus GitHub-Benchmark-Report) | ✅ | `memfuse-db/src/temporal_filter.rs` + Integrationstest |
| AGT-OLLAMA-14c0c140 (Kalibrierung Importance) | ✅ RESOLVED-Tag im Code | `ollama/src/importance.rs:165` |
| AGT-PY-d5d2be30 (`panic=abort`) | ✅ RESOLVED-Tag im Code | `memfuse-py/src/lib.rs:293` |
| `async_trait` (125 Stellen laut alter Spec) | ✅ **vollständig** auf AFIT migriert — 0 Treffer im gesamten Workspace | grep-negativ |

**Konsequenz für diese Review:** Der wertvollste Beitrag, den ich hier leisten kann, ist nicht das erneute Auflisten bereits erledigter Punkte, sondern (a) das Identifizieren dessen, was **auch jetzt noch** nachweislich fehlt, (b) ein konkreter neuer Risikofund, der in keinem der 18 Dokumente auftaucht, und (c) die aus den Dokumenten extrahierten, noch nicht umgesetzten Ideen mit dem höchsten Hebel — neu priorisiert nach dem heutigen Ist-Zustand.

**Update 22:18 Uhr:** Das Tempo hat sich seit der Erstfassung dieses Dokuments (14:28 Uhr) nicht verlangsamt — 36 weitere Commits, u. a. `memfuse-kv-bridge` als vollständiger neuer Crate, Cascading-Graph-Invalidation, ein LongMemEval/LoCoMo-CI-Gate und, bemerkenswert, ein ADR, der sich explizit auf diese Review bezieht. Die Detailergebnisse dazu finden sich direkt in den betroffenen Abschnitten unten, jeweils mit **[UPDATE 22:18]** markiert.

---

## 1. Neuer, dringender Befund: F-02 wurde trotz explizitem Architektur-Veto implementiert [UPDATE 22:18 — behoben]

Zwei Ihrer eigenen Dokumente — unabhängig voneinander verfasst — kommen zu demselben Urteil:

> „HNSW ist ein monolithischer, global verschränkter Graph … Chirurgische lokale Rebuilds zerstören die Delaunay-ähnlichen Nachbarschaftsbeziehungen … **Nicht machbar** mit Standard-HNSW." — `MemFuse_Physis_der_Erinnerung_PRD_v1.md`, §1 „Was NICHT umsetzbar ist"

> „**F-02 Partieller HNSW-Rebuild** — permanentes Architektur-Veto … `RwLock`-Contention unter Rust unlösbar." — `memfuse_architektur_praezisierung.md`, §5 Nicht-Implementieren-Liste (verfasst **heute**, wenige Stunden vor HEAD)

Der Live-Code zeigt: Commit `6488d715` („Implement Feature F-02: Nucleation Trigger for Local HNSW Partial Rebuilds", heute 13:40 Uhr, also **nach** der Veto-Dokumentation) fügt `HnswIndex::rebuild_region()` hinzu — einen per Feature-Flag (`physio-nucleation`, nicht default-aktiv) geschützten, autonom getriggerten Partial-Rebuild-Pfad.

**Genauere Prüfung der Implementierung** (nicht nur der Commit-Message, siehe Ihr eigener methodischer Hinweis aus `memfuse_goldstandard_kritische_bewertung.md` §3.4): Der Code führt *keinen* echten Delaunay-Rewire durch. Er iteriert über **alle** aktiven Knoten und entfernt lediglich Referenzen auf tombstonierte Knoten aus deren Nachbarschaftslisten — ohne Ersatzkanten einzufügen oder RNG-Pruning erneut anzuwenden. Das umgeht zwar den im Veto beschriebenen Worst Case (aktive Neuverdrahtung mit Lock-Contention über den ganzen Graphen), erzeugt aber ein anderes, unadressiertes Risiko: betroffene Knoten verlieren dauerhaft Grad (Anzahl Nachbarn), ohne dass die Navigierbarkeit des Graphen wiederhergestellt wird. Der zugehörige Test (`test_hnsw_nucleation_integration`) verifiziert ausschließlich, dass gelöschte IDs verschwinden — **keine** Recall-Messung vor/nach dem Partial-Rebuild.

**Einschätzung:** Kein Grund zur Panik (Feature-Flag-Disziplin wurde korrekt angewendet, das Feature ist nicht default-aktiv — genau das von Ihrem eigenen Dokument in §6.3 der kritischen Bewertung geforderte Sicherheitsnetz griff). Aber: Dies ist der konkreteste bisher gefundene Beleg für das Muster, vor dem meine Rolle mich warnen soll — ein System, das an der elegantesten Stelle scheitert, weil ein autonomer Agent eine Feature-Anfrage über eine dokumentierte, sachlich begründete architektonische Absage hinweg umgesetzt hat, ohne dass ein Mensch den Widerspruch zwischen Doku und Commit bemerkt hat.

**Empfehlung (P0, vor jedem weiteren Merge in diesem Bereich):**
1. `physio-nucleation` bleibt hart deaktiviert, bis ein Recall@10-Regressionstest (ANN-Benchmark vor/nach `rebuild_region()` bei realistischer Tombstone-Verteilung) existiert.
2. Governance-Lücke schließen: ein CI-Check, der jeden neuen Commit gegen die in `DECISIONS.md`/§5-Veto-Listen dokumentierten „Nicht-Implementieren"-Einträge abgleicht (Stichwort-Matching auf Feature-IDs wie „F-02" reicht als erster, billiger Schritt), analog zu Ihrem bereits bewährten `no_blanket_allow_deprecated`-Testmuster.
3. Entweder ADR schreiben, der das Veto explizit revidiert (mit Begründung, warum die Tombstone-Pruning-Variante das ursprüngliche Risiko nicht trägt), oder den Pfad vollständig entfernen. Stillschweigender Widerspruch zwischen Dokumentation und Code ist der teuerste Zustand.

> **[UPDATE 22:18] Status: alle drei Punkte umgesetzt, mit bemerkenswerter Sorgfalt.**
> - **Punkt 3** — `docs/decisions/ADR-063-f02-nucleation-tombstone-pruning-vs-ursprungliches-veto.md` existiert. Er übernimmt exakt die hier vorgenommene Unterscheidung (Tombstone-Pruning ≠ aktives Re-Wiring, aber eigenständiges Grad-Verlust-Risiko), stuft den Status als „Eingeschränkt akzeptiert (mit hartem Gate)" ein und benennt zwei explizite Freigabebedingungen: (a) `test_nucleation_recall_regression()` muss ≥ 30 Tage stabil grün sein, (b) eine Grad-Wiederherstellungsstrategie muss evaluiert sein, bevor `physio-nucleation` je default-aktiviert wird.
> - **Punkt 1** — `crates/memfuse-index/tests/nucleation_recall.rs` (157 Zeilen) wurde ergänzt. Kein oberflächlicher Test: Er baut einen 2000-Vektor-HNSW-Index, berechnet eine **Brute-Force-Ground-Truth** für 50 Queries, löscht 15 % in einer lokal konzentrierten Nachbarschaftsregion und vergleicht Recall@10 vor Löschung, nach reinem Tombstoning und nach `rebuild_region()` — mit harten Assertions (max. 5 Prozentpunkte Verschlechterung ggü. reinem Tombstoning, max. 15 Punkte ggü. Baseline). Das ist genau die Art Regressionstest, die in Abschnitt 3.6 dieser Review als dringendste fehlende Infrastruktur benannt wurde.
> - **Punkt 2** — `VETOES.md` (Root-Verzeichnis) wurde als maschinenlesbares Veto-Register eingeführt, inklusive `xtask check-vetoes`-Gate. Format je Eintrag: `feature_id`, `status` (`permanent_rejected` | `conditionally_accepted`), `keywords` (Commit-Trigger), `reason`, `adr_ref`, `conditional_review_due`. F-02 ist dort korrekt als `conditionally_accepted` mit Frist `2026-10-07` (30 Tage) geführt; F-10 (Osmotischer Wissensaustausch) als `permanent_rejected` ohne Ausnahmepfad — die in der Erstfassung dieser Review befürchtete Wiederholung des F-02-Musters bei F-10 ist damit strukturell abgesichert, bevor sie auftreten konnte.
>
> **Verbleibende Restarbeit laut ADR-063 selbst:** Die Grad-Wiederherstellungsstrategie (Bedingung b) ist noch nicht evaluiert — reines Monitoring, kein Blocker für den aktuellen (weiterhin non-default) Zustand, aber vor einer etwaigen Default-Aktivierung nachzuholen.

---

## 2. Verifizierte Lücken — Status-Update [UPDATE 22:18]

Von den sechs in der Erstfassung (Stand `bb099dc2`, 14:28 Uhr) gelisteten Lücken sind **vier innerhalb von acht Stunden geschlossen oder korrigiert** worden. Die Tabelle zeigt beide Zeitpunkte:

| Lücke | Stand 14:28 Uhr | **Stand 22:18 Uhr (jetzt)** | Priorität |
|---|---|---|---|
| **`memfuse-kv-bridge`** | Existierte nicht | ✅ **Implementiert** (`ade2f12f "feat: implement memfuse-kv-bridge security layer"`). Crate hat `segment.rs` (`KvSegment` mit `Zeroize`/`ZeroizeOnDrop`), `eviction_worker.rs` (dedizierter `EvictionWorker` für den regelmäßigen Hot-Path, **getrennt** von synchronem `emergency_wipe()`), `store.rs`. Genau die in §3.3 der Erstfassung geforderte Entkopplung Sicherheitsschicht/Backend-Integration wurde umgesetzt — inkl. eines eigenen DAG-Regressionstests (`a539f943`) und eines Nachfolge-Fixes für eine DAG-Regression (`e09d2bdd`). | War Hoch → **erledigt** |
| **`memfuse-py` nicht in Workspace-Members** | Als Lücke/Risiko eingestuft | ⚠️ **Korrektur meiner eigenen Einschätzung**: `ADR-064` (neu) dokumentiert, dass dies **beabsichtigt** ist, nicht versehentlich. Grund: Cargo erzwingt die Panic-Strategie (`abort` vs. `unwind`) workspace-weit, nicht pro Crate — `memfuse-py` braucht `panic=unwind` für PyO3-`catch_unwind()` an der FFI-Grenze, der Hauptworkspace nutzt `panic=abort`. Ein eigenständiger Sub-Workspace ist die einzige korrekte Lösung, keine technische Schuld. CI deckt `memfuse-py` über separate `--manifest-path`-Aufrufe ab. Zusätzlich abgesichert durch `check-duplicate-symbols`-CI-Gate (`41268a55`). | War Mittel → **kein Bug, Fehleinschätzung meinerseits korrigiert** |
| **Cascading-Invalidation Chunk→Graph-Kante** | Fehlte, PathRAG lief mit inkonsistentem Graphzustand | ✅ **Implementiert**, in drei Schritten (`e0834b5e`, `4f78e8fe`, `05b382d8`): `EdgeProvenance` trägt jetzt `source_doc_ids: Vec<DocId>`; ein Supersedes-Event tombstoniert die abhängigen CSR-Kanten automatisch. Zwei dedizierte Tests verifizieren dies: `supersedes_cascading_tombstone_test.rs` (`test_supersedes_triggers_edge_tombstone`, `test_pathrag_ignores_superseded_edges`) und ein E2E-Test `cascade_invalidation_test.rs`. Damit ist exakt die in `memfuse_gegenpruefung_architektur_einwaende.md` Punkt 4 vorgeschlagene Rückverfolgungs-Struktur real. | War Mittel-Hoch → **erledigt** |
| **`memfuse-candle` nicht in Serving-Pipeline verdrahtet** | Crate existierte isoliert | ✅ **Verdrahtet**: `memfuse-mcp/src/config.rs` instanziiert jetzt `CandleEmbedClient` und `CandleLlmClient` aus `memfuse_candle` (Commit `0733994b "feat(mcp): add Candle ML backend selection for embeddings and LLM"`) — der Ollama-Ausstiegspfad ist als wählbares Backend real, nicht mehr nur als Bibliothek ohne Aufrufer. Ob Ollama als Default abgelöst wurde oder Candle nur als Alternative wählbar ist, war im verfügbaren Zeitfenster nicht abschließend zu prüfen. | War Hoch → **größtenteils erledigt, Default-Pfad noch zu verifizieren** |
| **Edge-Vektoren (5. Fusionssignal)** | Fehlte | Weiterhin **kein** `EdgeVector`/`edge_embedding` im Code auffindbar. | **Weiterhin offen** — niedrigere Priorität als die vier oben, siehe §3.1 unten |
| **`ImportanceEmbeddingClassifier`** | Bewusst zurückgestellt zugunsten LLM-Konsolidierung | Keine Änderung seit 14:28 Uhr festgestellt. | **Weiterhin bewusst zurückgestellt**, siehe §3.5 (unverändert gültig) |

**Neuer Fund seither, nicht in der Erstfassung enthalten:** `4d7b2c42 "fix(index): decouple DiskANN persist_delta from synchronous insert hot-path"` — ein Fix genau der Klasse, vor der meine Rolle warnt (synchrone teure Operation im Hot-Path), wurde selbstständig identifiziert und behoben, ohne dass dies in einem der 18 Dokumente gefordert war. Positiv zu werten.

---

## 3. Die aus den Dokumenten wertvollsten, noch nicht (vollständig) umgesetzten Aspekte

Über die reine Lückenliste hinaus: Das ist die Auswahl der Ideen aus allen 18 Dokumenten, die für das Endprodukt den größten Hebel haben — mit Begründung, warum gerade diese.

### 3.1 Kalibrierungsprimitive als Konsolidierungs-Präzedenzfall konsequent weiterführen

`memfuse_goldstandard_kritische_bewertung.md` §6.1 identifiziert richtig: Das Muster „drei parallele, unvollständige Implementierungen eines mathematischen Problems auf eine gemeinsame Layer-0/1-Primitive konsolidieren" ist wiederverwendbar. `memfuse-calibration` existiert jetzt und wird bereits von Router, Reranker und (laut Commit-Historie) Replicator-Dynamics genutzt — das ist geschehen. Der im selben Dokument genannte **nächste Kandidat**, die drei parallelen Checkpoint-Abstraktionen in `memfuse-checkpoint`, ist mir in dieser Prüfung nicht neu verifizierbar (zeitlich nicht mehr im Scope), sollte aber vor neuen Layer-1-Features geprüft werden — genau das Wiederverwendungs-Playbook, das an dieser Stelle bereits einmal funktioniert hat.

### 3.2 Temporal-Validity-Post-Filter + Cascading-Invalidation als Paar behandeln, nicht getrennt

Der Temporal-Filter ist implementiert (§1 oben). Der in §2 dieser Review bestätigte fehlende Cascading-Trigger (Supersedes-Chunk → Graph-Kanten-Tombstone) ist die **komplementäre Hälfte** desselben Korrektheitsproblems: Ein Filter, der veraltete Kanten zur Anfragezeit ausblendet, behebt Symptome; ein Trigger, der sie beim Supersedes-Event sofort tombstoniert, behebt die Ursache und hält den PathRAG-Sufficiency-Gate-Zustand konsistent mit dem Chunk-Bestand. Da PathRAG jetzt produktiv im Suchpfad hängt (`search.rs`, `query_builder.rs`), ist dieser Fix keine akademische Fußnote mehr, sondern eine reale Quelle für Retrieval-Halluzination über tote Fakten. Konkreter Umsetzungsvorschlag aus `memfuse_gegenpruefung_architektur_einwaende.md` Punkt 4: eine `DocId → Set<EdgeId>`-Rückverfolgung, mitgeschrieben in der ohnehin geforderten `EdgeProvenance` (§7 Invariante `INV-GRAPH-PROV-1`, bereits normativ verlangt).

### 3.3 KV-Cache-Bridge: Sicherheitsschicht von Backend-Integration entkoppeln — jetzt technisch möglich

`memfuse_goldstandard_kritische_bewertung.md` §6.4 schlägt vor, `KvSegment`/`EncryptedKvLayer`/Zeroize-on-Evict als eigenständiges erstes Increment zu bauen, unabhängig von `memfuse-candle`. Das ist jetzt kein theoretischer Vorschlag mehr — `memfuse-candle` existiert bereits als eigenständiger Crate, was die Abhängigkeitskette De-facto verkürzt. Zusätzlich, aus der eigenen Gegenprüfung (Punkt 2 in `memfuse_gegenpruefung_architektur_einwaende.md`): Die vorgeschlagene `async fn evict_lru()` täuscht Nicht-Blockierung nur über die Signatur vor. Für den geplanten „VRAM > 80 %"-Hot-Path-Trigger (regelmäßig, nicht Notfall) braucht es einen dedizierten Eviction-Worker mit festem OS-Thread und Channel-Queue — nicht dieselbe synchrone Zeroize-Routine wie im seltenen `emergency_wipe()`-Pfad. Dieses Detail sollte als hartes Akzeptanzkriterium in die KV-Bridge-Spezifikation aufgenommen werden, bevor Implementierung beginnt.

### 3.4 Reranking: Zeit-Budget vor Zweitstufe

`memfuse_gegenpruefung_architektur_einwaende.md` Punkt 1 schlägt korrekt vor, vor einem architektonisch teuren ColBERT-artigen Late-Interaction-Ausbau zunächst ein hartes Zeit-Budget einzuführen. **Das ist inzwischen geschehen** — `RerankDeadline` und `RerankPidController` sind laut Commit-Log (#1699, #1702) implementiert. Damit ist dieser Punkt de facto erledigt; ein ColBERT-Ausbau ist aus heutiger Sicht nicht dringend, sollte aber, falls später doch verfolgt, den bereits einmal gelösten Storage-Duplikations-Konflikt (Embedding-Historie in `memfuse-index`) von Anfang an mitplanen, wie in demselben Dokument gewarnt.

### 3.5 `ImportanceEmbeddingClassifier`: bewusst zurückgestellt statt vergessen

Die Konsolidierung auf `memfuse-ollama` (Commit `f7600262`) statt eines trainierten Embedding-Classifiers ist konsistent mit der eigenen Einschätzung in `memfuse_goldstandard_kritische_bewertung.md` §4 (🔴, „ML-Trainingsproblem mit eigenem Datenbedarf, keine reine Software-Engineering-Aufgabe"). Ich teile diese Einschätzung: Ein Distillation-Ansatz aus bestehenden LLM-Scores ist der richtige *Ansatz*, aber ohne belastbaren, gelabelten Evaluationsdatensatz besteht das Risiko, ein unkalibriertes Modell zu produzieren, das nur *aussieht* wie eine Verbesserung — genau die Klasse Fehler, vor der meine Rolle warnen soll. Empfehlung: Erst nach Aufbau eines LongMemEval-/eigenen Regressions-Benchmarks (§3.6) angehen, mit explizitem `calibrated: false`-Fallback-Pfad wie im Implementierungsplan selbst vorgeschlagen.

### 3.6 Externes Benchmark (LongMemEval) — [UPDATE 22:18] Grundgerüst steht, aber noch nicht das volle externe Dataset

Stand 14:28 Uhr war dies als dringendste verbleibende Infrastrukturarbeit eingestuft. Zwischenzeitlich wurden zwei Commits gemerged: `99c9f835 "Add LongMemEval Regression Suite in CI"` und `6889fc39 "ci: add automated retrieval quality regression gate (LongMemEval & LoCoMo)"`. Live-Verifikation von `.github/workflows/bench.yml`:

- Ein Job `benchmark-regression` („Retrieval Quality Regression Gate (LongMemEval & LoCoMo)") läuft bei jedem Push/PR auf `main`/`develop`.
- Er führt `memfuse-bench` gegen LongMemEval- **und** LoCoMo-Fixtures aus und vergleicht die Ergebnisse per `compare-baseline`-Tool gegen eine versionierte Baseline mit `--threshold 0.05`.
- **Ehrliche Einschränkung, die der Code selbst dokumentiert** (Kommentar im Workflow, wortwörtlich übernommen): *„Executed against minimal test fixtures (parser/pipeline correctness). THIS IS NOT A FULL RECALL REGRESSION GATE until full external datasets are downloaded or fetched via Git-LFS in a follow-up pipeline setup."*

**Einschätzung:** Das ist der richtige erste Schritt — die Pipeline-Mechanik (Ausführung, Baseline-Vergleich, Schwellenwert, CI-Gate) steht und ist bereits nützlich für Regressions-Erkennung auf Fixture-Ebene. Es ersetzt aber noch **nicht** ein vollständiges externes Recall-/Faithfulness-Benchmark auf den echten LongMemEval-/LoCoMo-Datensätzen, wie es in `memfuse_finale_technische_spezifikation.md` §6.9 gefordert wird. Der von mir in §1 dieser Review beschriebene F-02-Fall wäre durch die aktuelle Fixture-basierte Gate **nicht zuverlässig** aufgefangen worden — er wurde stattdessen durch einen dedizierten, gezielten Test (`nucleation_recall.rs`) abgesichert, nicht durch die allgemeine CI-Gate. Empfehlung bleibt bestehen, nur abgeschwächt: Git-LFS-Anbindung der echten externen Datensätze ist der nächste, jetzt klar benannte Schritt (der Workflow-Kommentar formuliert dies bereits selbst als offene Folgearbeit).

---

## 4. Governance-Beobachtung: Was der Tagesverlauf über den Entwicklungsprozess zeigt [UPDATE 22:18]

Positiv (unverändert seit 14:28 Uhr): Die in `memfuse_goldstandard_kritische_bewertung.md` §6.2/§6.3 empfohlenen Muster — Feature-Flags für unsichere neue Arbeitspakete, Regressionstests für einmal gefundene Prozessfehler (AFIT-Migration vollständig, nicht nur behauptet) — greifen sichtbar.

**Der in der Erstfassung kritisierte Punkt ist zwischenzeitlich behoben:** Die zu 14:28 Uhr festgestellte Lücke — Architektur-Dokumente wurden nicht automatisiert in den Agenten-Workflow zurückgespeist, Veto und Umsetzung liefen am selben Tag ungebremst gegeneinander — wurde direkt adressiert. `VETOES.md` existiert jetzt als maschinenlesbare Datei mit `xtask check-vetoes`-CI-Gate und ist laut eigenem Kopfkommentar Teil der `.jules/`-Bootstrap-Sequenz. Der F-10-Eintrag (Osmotischer Wissensaustausch) ist bereits präventiv als `permanent_rejected` hinterlegt — die in der Erstfassung befürchtete Wiederholung des F-02-Musters bei F-10 wurde damit vorweggenommen, nicht erst nach einem zweiten Vorfall behoben.

**Verbleibende Beobachtung:** Ein Eintrag im Git-Log (`3004a3e1 "docs(governance): consolidate memfuse-py ADR-064 workspace isolation governance"`) sowie mehrere `Shell-Commit`-Einträge ohne beschreibende Message deuten auf denselben in `memfuse_goldstandard_kritische_bewertung.md` §3.4 beschriebenen Risikotyp hin (Commit-Historie ist nicht immer 1:1 mit dem Diff-Inhalt korreliert). Dies ändert nichts an den oben durch direkte Code-/Test-/ADR-Inspektion verifizierten Befunden, ist aber als generelle Vorsicht bei zukünftigen, nur auf Commit-Messages gestützten Bewertungen zu wiederholen.

---

## 5. Priorisierte Empfehlung — Status-Update [UPDATE 22:18]

| # | Empfehlung (14:28 Uhr) | Status (22:18 Uhr) |
|---|---|---|
| 1 | P0 — Recall-Regressionstest für `rebuild_region()` (F-02), `physio-nucleation` bis dahin deaktiviert lassen, Veto-Diskrepanz per ADR klären | ✅ **Erledigt** — `ADR-063` + `nucleation_recall.rs`, siehe §1 |
| 2 | P0 — `VETOES.md` als maschinenlesbare Governance-Ergänzung | ✅ **Erledigt** — inkl. `xtask check-vetoes`-CI-Gate, siehe §1 |
| 3 | P1 — Cascading-Invalidation Supersedes→Graph-Kante | ✅ **Erledigt** — `EdgeProvenance.source_doc_ids` + zwei dedizierte Tests, siehe §2 |
| 4 | P1 — `memfuse-py` in Workspace-Members aufnehmen | ⚠️ **Zurückgezogen** — `ADR-064` zeigt, dass die Trennung technisch zwingend ist (Cargo-Panic-Strategie ist workspace-weit). Meine ursprüngliche Einordnung als „Lücke" war falsch; korrigiert in §2. |
| 5 | P2 — LongMemEval-/Recall-Regressions-Suite in CI | 🟡 **Teilweise erledigt** — CI-Gate mit Baseline-Vergleich steht, aber auf Fixture-Basis, nicht auf vollem externem Dataset. Siehe §3.6 für Details. |
| 6 | P2 — KV-Cache-Bridge-Sicherheitsschicht als von `memfuse-candle` entkoppeltes erstes Increment | ✅ **Erledigt** — `memfuse-kv-bridge` mit `EvictionWorker` + `KvSegment`-Zeroize, siehe §2 |
| 7 | P3 — `memfuse-candle` in Serving-Pipeline verdrahten, Edge-Vektor-Signal evaluieren | 🟡 **Candle-Teil erledigt** (Backend-Auswahl in `memfuse-mcp`), **Edge-Vektoren weiterhin offen** |

### Verbleibende offene Punkte, neu priorisiert

1. **P1 — Diese Woche:** Grad-Wiederherstellungsstrategie für `rebuild_region()` evaluieren (explizite Bedingung (b) in ADR-063, sonst bleibt `physio-nucleation` dauerhaft non-default, was an sich unkritisch, aber als offener Punkt zu tracken ist).
2. **P1 — Diese Woche:** Verifizieren, ob `memfuse-candle` in `memfuse-mcp` tatsächlich als **Default**-Backend nutzbar ist oder nur als Opt-in-Alternative neben Ollama — für den „Sovereign Core"-Produktanspruch macht das einen strategischen Unterschied.
3. **P2 — Nächste 2–4 Wochen:** Git-LFS-Anbindung der echten LongMemEval-/LoCoMo-Datensätze an die bestehende CI-Gate-Mechanik (Folgearbeit, die der Workflow-Kommentar selbst bereits benennt).
4. **P2 — Nächste 2–4 Wochen:** Edge-Vektor-Signal (5. Fusionssignal, MinnsDB-Vergleich) evaluieren — jetzt, da die Retrieval-Regressions-Infrastruktur (Punkt 3) eine belastbare Vorher/Nachher-Messung ermöglichen wird.
5. **P3 — Kontinuierlich:** Das in §1 sichtbar gewordene Governance-Muster (Doku-Veto am Vormittag, ADR-getriebene Revision mit hartem Gate am Abend, **innerhalb desselben Tages**) ist ungewöhnlich diszipliniert und sollte als Standardablauf für jede zukünftige Divergenz zwischen Architektur-Review und Live-Code dokumentiert werden — nicht nur als Einzelfall-Reaktion.

---

*Ende der Review, Stand HEAD `738c0ace`. Alle Codebasis-Aussagen wurden gegen einen frischen `git pull` verifiziert; Aussagen zu Dokumentinhalten sind mit Quelldatei referenziert. Frühere Bewertungen (Stand `bb099dc2`) sind dort, wo sie sich geändert haben, mit [UPDATE 22:18] markiert und nicht stillschweigend überschrieben — analog zur in Abschnitt 3.4 der Erstfassung selbst geforderten Selbstkorrektur-Disziplin.*
