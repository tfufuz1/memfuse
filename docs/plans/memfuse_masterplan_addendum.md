# MemFuse — Masterplan-Addendum: Spezifikation & Code-Abgleich
> **Version**: 1.0 — 03. September 2026
> **Zweck**: Konkretisierung und Faktenkorrektur des `memfuse_masterplan.md` gegen den tatsächlichen
> Code-Stand des Repos `tfufuz1/memfuse` (verifiziert per `git pull`, Stand: HEAD, 966 Commits)
> **Methodik**: Jede Aussage unten ist gegen den Quellcode geprüft, nicht gegen Audit-Reports oder ADR-Prosa —
> genau die Unterscheidung, die dieses Projekt strukturell am schwersten fällt.

---

## 0. Kritische Korrektur der Planungsgrundlage

Der Masterplan begründet seine gesamte Velocity-Kalibrierung (19,3 Commits/Tag, Aufwandsschätzung
"1 Woche = 24–60 Commits") auf **2.339 Commits über 121 Tage**. Das ist falsch.

| Metrik | Masterplan-Annahme | Verifizierter Ist-Wert | Abweichung |
|---|---|---|---|
| Gesamt-Commits (05.05.–03.09.2026) | 2.339 | **966** | −59% |
| Reale Velocity | 19,3/Tag | **~8,0/Tag** | Faktor 2,4× überschätzt |
| Crates | 15 | 15 ✅ | korrekt |
| ADRs in DECISIONS.md | 47 | **50** (Stand HEAD) | Plan bereits 3 ADRs hinterher |

**Konsequenz**: Jede Aufwandsschätzung im Plan, die implizit auf "1 Woche = X Commits" beruht, ist um
Faktor ~2,4 zu optimistisch kalibriert. Die expliziten Tage-Schätzungen pro Arbeitspaket (z.B. "4–6 Tage"
für HNSW-Rebuild) sind davon unabhängig plausibel, da sie auf Code-Komplexität statt auf Commit-Statistik
beruhen — aber die **Phasen-Enddaten** (07.11.2026, 06.02.2027, 30.06.2027) wurden wahrscheinlich mit der
falschen Velocity gegengeprüft und sollten neu validiert werden, bevor sie als Commitment kommuniziert werden.

**Empfehlung**: Abschnitt 0 des Masterplans streichen oder mit echten Zahlen neu rechnen. Velocity-Metriken
aus Commit-Zählungen sind ohnehin ein schwaches Signal in einem Projekt, in dem ein einzelner Agent-Lauf
5–10 "Verify & Sync"-Commits pro Sitzung erzeugt (siehe `git log`: viele `Shell-Commit`- und
Audit-Sync-Commits ohne Code-Änderung).

---

## 1. ADR-Nummernkollision — MUSS vor Sprint-Start gelöst werden

Der Plan reserviert ADR-048 bis ADR-065 für neue Entscheidungen. Das Repo hat diese Nummern **bereits
teilweise und mit anderem Inhalt vergeben** — und zwar in zwei parallelen, nicht synchronisierten Systemen:

| ADR-Nr. | Masterplan-Vorschlag | `DECISIONS.md` (bereits vorhanden) | `docs/decisions/` (bereits vorhanden) |
|---|---|---|---|
| 048 | HNSW Copy-on-Write Rebuild | WAL Legacy-Key Feature-Gating | `ADR-048-python-ffi-panic-isolation.md` |
| 049 | Router Conformal Calibration Fix | Audit-Log Append-Only Enforcement | — |
| 050 | Context Compaction OCC Retry | **Router Single-Conformal Calibration & Lock Scope Consolidation** (bereits final, s.u.) | — |
| 052 | Agent Dead-Letter/Timeout/Budget | — | `ADR-052-pinguard-drop-strategy.md` (bereits final, s.u.) |

Das ist kein kosmetisches Problem: `DECISIONS.md` und `docs/decisions/` sind zwei **verschiedene, nicht
gegenseitig referenzierte Nummernkreise** — ein direkter Verstoß gegen die eigene MECE-Doktrin aus
`CONSTITUTION.md` ("Jede Information lebt an genau EINEM Ort"). Der Masterplan hat diese Aufspaltung nicht
bemerkt, weil er offenbar nur `DECISIONS.md` gelesen hat.

**Konkrete Maßnahme (ADR-XXX, vor Sprint 2B-1)**:
1. Governance-Entscheidung: Ein einziger ADR-Nummernkreis. Entweder `docs/decisions/` als Zielformat
   (ein File pro ADR) mit einmaliger Migration der 50 bestehenden `DECISIONS.md`-Einträge, oder Rückbau
   von `docs/decisions/` in die Monolith-Datei.
2. Live-Check vor jeder Nummernvergabe MUSS beide Quellen prüfen:
   ```bash
   { grep -oP '(?<=^## ADR-)\d+' DECISIONS.md; ls docs/decisions/ 2>/dev/null | grep -oP '(?<=ADR-)\d+'; } \
     | sort -n | tail -1
   ```
3. Neuer, kollisionsfreier Nummernplan für den Masterplan: **ADR-051 bis ADR-068** (da 048–050 real belegt
   sind und 052 im Parallelsystem existiert). Tabelle in Abschnitt 9 des Masterplans entsprechend verschieben.

---

## 2. Status-Update der Lücken-Matrix (L-1 bis L-9) — Ist-Stand statt Plan-Annahme

Zwei der neun "kritischen Lücken" wurden zwischen Plan-Erstellung und diesem Review bereits (teilweise)
geschlossen. Eine zusätzliche, im Plan nicht erfasste Lücke wurde entdeckt.

| # | Lücke | Plan-Status | **Verifizierter Ist-Status** |
|---|---|---|---|
| L-1 | HNSW Rebuild blockiert Schreibpfad | 🔴 offen | **Bestätigt offen.** `rebuild()` in `hnsw.rs:1685` hält `write_mutex` für die volle Funktionsdauer inkl. der Re-Insert-Schleife über alle aktiven Nodes. Kein `rebuild_in_progress`-Delta-Mechanismus vorhanden. Plan-Spezifikation (2-Phasen-Lock) ist technisch korrekt und bleibt gültig. |
| L-2 | Router-Kalibrierung oszilliert | 🟡 offen | **Bereits behoben** unter `ADR-050: Router Single-Conformal Calibration & Lock Scope Consolidation` (final, 2026-09-03). Verifiziert in `router.rs`: Profilauswahl + Kalibrierungs-Update laufen jetzt in einem einzigen `self.calibration.write()`-Scope; die konkurrierende `recalibrate()`-Legacy-Methode ist `#[deprecated]` und wird nirgends mehr aufgerufen; die Non-Conformity-Berechnung wurde von der self-referenziellen Ratio (`1/confidence`) auf eine Margin-Formel (`(quantile_threshold − best_score).clamp(0,1)`) umgestellt. **Arbeitspaket 2B-1-B entfällt — aber siehe Abschnitt 3 unten, der methodische Kern des Problems bleibt.** |
| L-3 | Context Compaction ohne OCC-Retry | 🟡 offen | **Teilweise bereits implementiert.** `ConsolidationSession::refresh()` existiert in `context_compaction.rs:323` bereits vollständig (liest aktuelle TxIds neu, gibt geänderte DocIds zurück) — exakt die vom Plan spezifizierte Bausteinfunktion. **Was tatsächlich noch fehlt**: der Retry-Wrapper (`consolidate_with_retry` mit Backoff) und die Startup-Recovery (`cleanup_orphaned_consolidation_intents`) — beide wie im Plan spezifiziert, keine Änderung nötig. Aufwand entsprechend auf **1,5–2 Tage** statt 3–4 Tage reduzieren. |
| L-4 | ProvenanceRecord wird nicht befüllt | 🟡 offen | **Bestätigt offen**, aber Zahl korrigieren: **39×** `provenance: None` in `memfuse-db/src/` (nicht 31× wie im Plan). Spezifikation im Plan ist strukturell sinnvoll, siehe Präzisierung in Abschnitt 4. |
| L-5 | Agent Dead-Letter-Queue fehlt | 🟡 offen | **Bestätigt offen**, keine Spur von `DeadLetter`/`timeout()` in `engine.rs`. Plan-Spezifikation unverändert gültig. |
| L-6 | Batch-Pfade nicht durchgezogen | 🟡 offen | Nicht erneut vollständig verifiziert in diesem Pass — Stichprobe zeigt `insert_many()` existiert, MCP-Tool-Layer nicht geprüft. Plan-Spezifikation übernehmen, aber vor Sprint-Start `grep -rn "memfuse_batch_insert" crates/memfuse-mcp/` als Sanity-Check ausführen. |
| L-7 | DiskANN Production-Lifecycle fehlt | 🟡 offen | Nicht neu verifiziert (Umfang zu groß für diesen Pass). Plan-Aufwand (20–30 Tage) erscheint realistisch. |
| L-8 | PyO3 Bindings unvollständig | 🟢 offen | Nicht neu verifiziert. |
| L-9 | Cluster-Stubs (dead code) | 🟢 offen | **Diagnose im Plan ist sachlich falsch.** Der Plan behauptet: *"Begründung: feature 'cluster' existiert nicht in Cargo.toml → dead code"*. Tatsächlich: `crates/memfuse-db/Cargo.toml:37` enthält `cluster = []` — das Feature existiert und ist explizit deklariert, nur nicht implementiert/aktiviert. Es ist **kein dead code hinter einem nicht-existenten Feature**, sondern **unvollständig ausimplementiertes, aber real gegatetes** experimentelles Feature. Entscheidung nötig: Entweder Cluster-Feature vollständig streichen (Cargo.toml-Eintrag + Code) oder als "Phase 4+, nicht Phase 2B" explizit vertagen — aber nicht als "0,5-Tage-Cleanup" behandeln, ohne vorher zu klären, ob irgendein Konsument (memfuse-tauri?) das Feature bereits aktiviert. |
| **L-10 (neu)** | **Orphan-Checkpoint-Registry ist prozessweiter Singleton ohne Mandanten-Trennung** | *nicht im Plan erfasst* | `crates/memfuse-checkpoint/src/lib.rs:41`: `pub static ORPHAN_REGISTRY: OnceLock<OrphanRegistry>` — ein einziger globaler Singleton für den gesamten Prozess. Der Fix in `ADR-052-pinguard-drop-strategy.md` hat das *Drop-Zeitpunkt-Problem* korrekt gelöst (kein `thread::spawn` mehr, stattdessen persistente Registrierung mit Fehlerlogging), aber **nicht** das Nebenläufigkeits-/Mandanten-Problem. Das Projekt selbst hat das inzwischen erkannt: `lib.rs:1224` enthält einen `AI-TAG[TEST][MAJOR]`-Kommentar, der exakt beschreibt, dass parallele Test-Instanzen sich über den globalen Singleton gegenseitig die Registry leerräumen — aber nur als **Test-Flakiness**, nicht als **Produktions-Datensicherheitsproblem** (mehrere `MemFuse`-Instanzen im selben Prozess, z.B. Multi-Tenant-Server via `memfuse-mcp`, teilen sich eine Orphan-Pin-Datei). **Muss als eigenes Arbeitspaket in Sprint 2B-1 aufgenommen werden, bevor Multi-Tenant-Arbeit in Phase 4 beginnt** — sonst wird RBAC/Tenant-Isolation (Sprint 4-1) auf einem Fundament gebaut, das bereits einen globalen Cross-Tenant-Leak-Pfad hat. |

---

## 3. Der methodische Rest-Fehler in ADR-050 (Router-Kalibrierung)

Der Code-Fix von ADR-050 ist **technisch korrekt und notwendig** — er beseitigt die TOCTOU-Race und die
sich widersprechenden Update-Regeln. Er löst aber nicht das tiefere Problem, das im ursprünglichen
Architektur-Review benannt wurde: Der "Non-Conformity Score", der die Konformal-Kalibrierung antreibt,
wird weiterhin ausschließlich aus `best_score` — also aus der *eigenen* heuristischen Retrieval-Bewertung,
die die Profilwahl überhaupt erst getroffen hat — abgeleitet:

```rust
// router.rs, Stand nach ADR-050
let q_threshold = state.conformal.quantile_threshold;
let non_conformity = (q_threshold - best_score).max(0.0).clamp(0.0, 1.0);
state.recalibrate_conformal(non_conformity);
```

Es gibt weiterhin **kein externes Ground-Truth-Signal** (hat das gewählte SLM die Anfrage tatsächlich
korrekt beantwortet? War eine Eskalation nötig?), das in diese Schleife einfließt. Die Kalibrierung
konvergiert stabil — aber gegen die Verteilung ihrer eigenen Score-Funktion, nicht gegen tatsächliche
Routing-Fehlerraten. Das ist der Unterschied zwischen "die Oszillation ist weg" (ADR-050 ✅) und "die
Kalibrierung misst etwas Reales" (weiterhin offen).

**Konkretisierung für ein neues Arbeitspaket (ersetzt den ursprünglichen 2B-1-B-Vorschlag, da dieser
inzwischen obsolet ist):**

**Arbeitspaket: Outcome-gebundene Kalibrierung (ADR-05X, Sprint 2B-1 oder 2B-2, 3–4 Tage)**

```rust
/// Nachträgliches Feedback-Signal für eine bereits getroffene Routing-Entscheidung.
/// Muss vom Aufrufer (Agent-Loop) geliefert werden, NACHDEM das SLM geantwortet hat.
pub enum RoutingOutcome {
    /// SLM hat die Anfrage ohne Eskalation zufriedenstellend beantwortet.
    Success,
    /// SLM-Antwort war unzureichend, Eskalation zu größerem Modell war nötig.
    Escalated { escalated_to: String },
    /// SLM-Antwort wurde von einem nachgelagerten Judge/Reranker als falsch markiert.
    Rejected,
}

impl RouterEngine {
    /// Muss vom Aufrufer nach Abschluss der SLM-Antwort aufgerufen werden.
    /// Trennt den Kalibrierungs-Update-Zeitpunkt vom Routing-Zeitpunkt.
    pub fn record_outcome(&self, decision_id: DecisionId, outcome: RoutingOutcome) {
        let non_conformity = match outcome {
            RoutingOutcome::Success => 0.0,
            RoutingOutcome::Escalated { .. } => 0.7,
            RoutingOutcome::Rejected => 1.0,
        };
        // ... state.recalibrate_conformal(non_conformity) mit ECHTEM Signal statt Score-Margin
    }
}
```

Das erfordert eine strukturelle Änderung: `route()` liefert eine `DecisionId` statt sofort final zu
kalibrieren; die eigentliche Kalibrierung verschiebt sich in einen zweiten, asynchronen Call
`record_outcome()`, den der Agent-Orchestrator (Layer 3, `memfuse-agent`) nach Abschluss des SLM-Calls
aufruft. Das ist die einzige Konstruktion, die aus "Score, der zufällig mit sich selbst korreliert" ein
echtes Konformal-Verfahren mit Coverage-Garantie macht. Ohne diesen Schritt sollte die Doku (`profile.rs`)
den Begriff "Conformal Prediction" nicht mehr verwenden — er impliziert eine statistische Garantie, die das
System aktuell nicht einlöst.

**Abhängigkeit**: Dieses Arbeitspaket sollte **vor** Arbeitspaket 2B-2-B (Agent Dead-Letter/Budget) laufen,
da `record_outcome()` sinnvollerweise am selben Punkt im Agent-Loop eingehängt wird wie das
Timeout/Budget-Settlement (`reservation.settle(...)`) — beide sind "Nachbereitung nach Tool-Ausführung"
und sollten in einem Arbeitspaket zusammengefasst werden, um nicht zwei separate Hooks in `engine.rs`
einzuziehen.

---

## 4. Präzisierung Arbeitspaket 2B-2-A (ProvenanceRecord)

Der Plan spezifiziert korrekt, wo Provenance einzuhängen ist (`fusion.rs`, RRF-Fusion). Eine Lücke in der
Spezifikation: Bei 39 `None`-Stellen ist zu erwarten, dass nicht alle Call-Sites über den RRF-Pfad laufen
(z.B. reine Vektorsuche ohne Fusion, direkte `get()`-Aufrufe). Vor Implementierung:

```bash
# Vollständige Inventur aller Stellen, die SearchResult konstruieren, nicht nur die RRF-Fusion
grep -rn "SearchResult\s*{" crates/memfuse-db/src/ | grep -v tests
```

Falls dabei Konstruktionsstellen außerhalb von `fusion.rs`/`search.rs` auftauchen (z.B. in
`context_compaction.rs`, wo `consolidate_via_llm()` einen synthetischen `ContextChunk` ohne Provenance
erzeugt — verifiziert vorhanden), muss die `ProvenanceRecord`-Population dort um einen expliziten
`ProvenanceRecord::synthesized_from(source_doc_ids)`-Fall ergänzt werden, sonst bleibt nach Sprint 2B-2
ein Teil der 100%-Exit-Kriterium-Messung (Abschnitt 7 des Plans) unerreichbar, weil konsolidierte Dokumente
strukturell nie durch den Fusion-Pfad laufen.

---

## 5. Zusammenfassung der Korrekturen am Masterplan

| Abschnitt im Original-Plan | Korrektur |
|---|---|
| §0 Velocity-Kalibrierung | Commit-Zahl von 2.339 auf 966 korrigieren; Enddaten der Phasen mit realer Velocity (~8/Tag) neu prüfen |
| §1 Lücken-Matrix, L-2 | Als erledigt markieren (ADR-050), Restaufwand auf methodisches Ground-Truth-Problem umlenken (siehe §3 oben) |
| §1 Lücken-Matrix, L-3 | Aufwand von 3–4 auf 1,5–2 Tage reduzieren (`refresh()` bereits vorhanden) |
| §1 Lücken-Matrix, L-4 | Zahl 31× → 39× korrigieren; Provenance-Population auf Nicht-RRF-Pfade (Konsolidierung) erweitern |
| §1 Lücken-Matrix, L-9 | Diagnose korrigieren: Feature existiert in Cargo.toml, ist aber unvollständig implementiert — kein reiner Löschvorgang |
| §1 Lücken-Matrix | **L-10 neu ergänzen**: Orphan-Registry Multi-Tenant-Leak, vor Phase 4 (RBAC) zu lösen |
| §3, Arbeitspaket 2B-1-B | Ersatzlos streichen (bereits erledigt), durch neues Arbeitspaket "Outcome-gebundene Kalibrierung" ersetzen |
| §9 ADR-Nummernplan | Kompletter Nummernkreis auf 051–068 verschieben; Governance-Entscheidung zu `DECISIONS.md` vs. `docs/decisions/` vorschalten |

*Dieses Addendum ersetzt keine der langfristigen Phase-3/4-Spezifikationen (Sleep-Cycle, PathRAG, RBAC,
OAuth) — deren Code existiert noch nicht, daher gibt es dort nichts gegen die Realität zu prüfen. Es sollte
erneut mit demselben Verfahren geprüft werden, sobald diese Phasen näher rücken.*
