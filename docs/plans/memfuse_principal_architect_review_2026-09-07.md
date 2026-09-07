# MemFuse — Principal-Architect-Review

> **Rolle:** Principal Senior Rust Architect (Storage/Concurrency · IR/Vektorsuche · ML-Systemdesign für Agenten-Infrastruktur)
> **Repository:** `tfufuz1/memfuse`, live geklont am 2026-09-07
> **HEAD zum Zeitpunkt dieser Prüfung:** `bb099dc2` (07.09.2026, 14:28 Uhr) — 1.248 Commits, ~119.700 LOC Rust, 17 Workspace-Crates
> **Referenz-Dokumente:** die 18 angehängten Spezifikations-/Analyse-/Planungsdokumente (v1.0 → v4.1, Jules-Prompts, ArXiv-Synthesen, PRD, Wettbewerbsvergleich)
> **Methodik:** Jede zentrale Behauptung wurde gegen den frisch geklonten Code neu verifiziert (`grep`, Datei-Inspektion, Git-Log) — nicht aus den Dokumenten übernommen.

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

---

## 1. Neuer, dringender Befund: F-02 wurde trotz explizitem Architektur-Veto implementiert

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

---

## 2. Verifizierte, tatsächlich noch offene Lücken (Stand HEAD `bb099dc2`)

| Lücke | Quelle in Dokumenten | Live-Verifikation | Priorität |
|---|---|---|---|
| **`memfuse-kv-bridge` existiert nicht** (weder als Crate noch als `KvSegment`/`EncryptedKvLayer`-Typ) | Kern-Alleinstellungsmerkmal in `spec_v21.md` §6.2, `memfuse_finale_technische_spezifikation.md` §6.2 | `find … -iname "*kv*bridge*"` → kein Treffer | Hoch, aber bewusst spät — siehe §3.4 unten |
| **`memfuse-py` weiterhin nicht in `Cargo.toml`-Workspace-Members** | A20 (v4.1-Audit), mehrfach in Jules-Prompts als „KRITISCH: Build- und Sicherheitsregression" markiert | `grep memfuse-py Cargo.toml` → kein Treffer in `members` | Mittel — Verzeichnis existiert, Sicherheits-Fix (`panic=unwind`) ist im Quelltext markiert RESOLVED, aber der Workspace bindet den Crate nicht ein, d.h. der Fix wird von `cargo build --workspace` nicht einmal geprüft |
| **Edge-Vektoren fürs Retrieval-Fusion (5. Signal)** | `MemFuse_Architektur_Analyse_2026.md` §3.1 (MinnsDB-Vergleich), `MemFuse_vs_Competitors_Detailed.md` | kein `EdgeVector`/`edge_embedding` im Code | Mittel |
| **Cascading-Invalidation Chunk→Graph-Kante** (Supersedes-Displacement löst keine Graph-Tombstones aus) | `memfuse_gegenpruefung_architektur_einwaende.md` Punkt 4, unabhängig bestätigt | kein `tombstone_edges_for_doc`/vergleichbarer Trigger auffindbar; CSR-Tombstone-Mechanismus (`csr.rs`) bleibt isoliert vom `memfuse-db`-Supersedes-Pfad | Mittel-Hoch — betrifft Korrektheit des PathRAG-Sufficiency-Gates, das jetzt produktiv ist |
| **`memfuse-candle` ist noch nicht in die Serving-Pipeline verdrahtet** | — (neuer Befund) | Crate existiert (488 LOC: GGUF-Loader, Inferenz, Embedding, Model-Registry), aber **kein** Import von `memfuse_candle` in `memfuse-db`, `memfuse-ollama` oder `memfuse-router` | Hoch für den „Sovereign Core"-Claim — das Fundament steht, der Ollama-Ausstieg ist noch nicht vollzogen |
| **`ImportanceEmbeddingClassifier`** (58ms-Ziel statt LLM-Call) | AP-5 in `memfuse_v2_implementierungsplan.md`, als 🔴 ML-Trainingsaufgabe eingestuft | Nicht gefunden; stattdessen zeigt Commit `f7600262 "Consolidate LLM Importance Scoring on memfuse-ollama"`, dass das Projekt den LLM-basierten Pfad konsolidiert statt abgelöst hat | Niedrig-Mittel — plausible bewusste Entscheidung angesichts des ML-Trainingsaufwands (siehe §3.5) |

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

### 3.6 Externes Benchmark (LongMemEval) fehlt weiterhin als Regressions-Fundament

Mehrere Dokumente (ArXiv-Berichte §7, `memfuse_finale_technische_spezifikation.md` §6.9) nennen die LongMemEval-Integration als „fehlendes Benchmark". Angesichts der jetzt sehr hohen Änderungsgeschwindigkeit (76 Commits an einem einzigen Tag) und des in §1 gefundenen Falls (ein Feature wurde trotz Veto gemergt, ohne dass eine Recall-Metrik das aufgefangen hätte), ist dies aus meiner Sicht **die dringendste verbleibende Infrastrukturarbeit** — dringender als jedes einzelne neue Feature. Ohne einen automatisierten, in CI laufenden Recall-/Faithfulness-Benchmark kann das Projekt bei diesem Tempo nicht zuverlässig unterscheiden zwischen „elegant aussehender Verbesserung" und „stiller Qualitätsregression".

---

## 4. Governance-Beobachtung: Was der heutige Tag über den Entwicklungsprozess zeigt

Positiv: Die in `memfuse_goldstandard_kritische_bewertung.md` §6.2/§6.3 empfohlenen Muster — Feature-Flags für unsichere neue Arbeitspakete (F-02 korrekt hinter `physio-nucleation`), Regressionstests für einmal gefundene Prozessfehler (AFIT-Migration jetzt tatsächlich vollständig, nicht nur per Commit-Message behauptet) — greifen sichtbar.

Kritisch: Die Diskrepanz aus §1 (Veto am Morgen, Umsetzung am Nachmittag) zeigt, dass Ihre Architektur-Dokumente aktuell **nicht automatisiert** in den Agenten-Workflow zurückgespeist werden. Die Governance-Infrastruktur (`.jules/SESSION_BOOTSTRAP.md`, `CONSTITUTION.md`, `.jules/COMMON_LLM_ERRORS.md` laut `MemFuse_Architektur_Analyse_2026.md` §4.2) ist vorbildlich für Code-Anti-Patterns, deckt aber offenbar keine „per-Feature-Veto"-Liste ab, die aus den (menschlich gelesenen) Analyse-Dokumenten stammt. Empfehlung: Die §5-Nicht-Implementieren-Liste aus `memfuse_architektur_praezisierung.md` sollte als maschinenlesbare Datei (z. B. `VETOES.md` mit Feature-IDs) direkt neben `CONSTITUTION.md` liegen und Teil der Jules-Einlese-Sequenz werden — sonst wiederholt sich der F-02-Fall bei jedem zukünftigen Feature mit denselben Buchstaben-Präfixen (F-10 Osmotischer Austausch ist mit identischer Begründung verboten und ein ähnlich verlockendes „Direct Fit"-Missverständnis ist dort ebenso denkbar).

---

## 5. Priorisierte Empfehlung (Update gegenüber allen 18 Dokumenten, Stand heute)

1. **P0 — Sofort:** Recall-Regressionstest für `rebuild_region()` (F-02) schreiben und `physio-nucleation` bis dahin hart deaktiviert lassen; Veto-Diskrepanz per ADR klären (§1).
2. **P0 — Sofort:** `VETOES.md` als maschinenlesbare Governance-Ergänzung neben `CONSTITUTION.md` einführen (§4).
3. **P1 — Diese Woche:** Cascading-Invalidation Supersedes→Graph-Kante schließen, jetzt mit realer Auswirkung auf PathRAG-Korrektheit (§3.2).
4. **P1 — Diese Woche:** `memfuse-py` tatsächlich in `Cargo.toml`-Workspace-Members aufnehmen, damit der bereits im Quelltext markierte Sicherheits-Fix auch von CI erfasst wird (§2).
5. **P2 — Nächste 2–4 Wochen:** LongMemEval-/Recall-Regressions-Suite in CI (§3.6) — Voraussetzung für jedes weitere ML-lastige Feature (Edge-Vektoren, ImportanceEmbeddingClassifier).
6. **P2 — Nächste 2–4 Wochen:** KV-Cache-Bridge-Sicherheitsschicht (`KvSegment`, Zeroize via dediziertem Worker-Thread) als von `memfuse-candle`-Backend-Integration entkoppeltes erstes Increment starten (§3.3).
7. **P3 — Mittelfristig:** `memfuse-candle` tatsächlich in die Serving-Pipeline verdrahten (Ollama-Ausstieg), Edge-Vektor-Signal (5. Fusionssignal) evaluieren, sobald die Benchmark-Infrastruktur aus P2 steht.

---

*Ende der Review. Alle Codebasis-Aussagen wurden gegen HEAD `bb099dc2` (frischer Klon) verifiziert; Aussagen zu Dokumentinhalten sind mit Quelldatei referenziert.*
