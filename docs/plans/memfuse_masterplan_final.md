# MemFuse — Konsolidierter Masterplan
> **Zusammenführung von drei Vorgänger-Dokumenten**: Architektur-Review, Strategische Gesamtbewertung, Original-Masterplan
> **Erstellt**: 04. September 2026
> **Methodik**: Jede übernommene Aussage wurde gegen den geklonten Quellcode (`tfufuz1/memfuse`, HEAD) verifiziert. Widersprüche zwischen den Quelldokumenten werden explizit aufgelöst, nicht verschwiegen.
> **Rust-Version**: 1.89 (Workspace) · **Crates**: 15 + xtask · **Umfang**: ~92.000 LOC Rust

---

## 0. Leseanleitung & Herkunft dieses Dokuments

Drei Vorgänger-Dokumente lagen vor:

1. **Architektur-Review** — Crate-für-Crate-Codeanalyse mit konkreten Befunden und Schweregraden (🔴🟠🟡🔵)
2. **Strategische Gesamtbewertung** — Prüft Governance, Positionierung und deckt einen kritischen Fehler im Selbstprüfungsmechanismus des Projekts auf; kalibriert die Velocity-Annahmen des Original-Masterplans neu
3. **Original-Masterplan** — Detaillierter Sprint-/ADR-Plan mit Code-Skizzen, Exit-Kriterien und Zeitachse bis Juni 2027

Diese drei Dokumente überschneiden sich stark, widersprechen sich aber auch an entscheidenden Stellen (v. a. bei der **Geschwindigkeitskalibrierung** und der **Reihenfolge der Arbeitspakete**). Dieses Dokument:

- **Verifiziert** die technischen Kernbehauptungen aller drei Dokumente direkt im Quellcode (Ergebnis: siehe Abschnitt 1)
- **Löst Widersprüche auf** zugunsten der zuletzt und am tiefsten verifizierten Quelle (Dokument 2 für Governance/Velocity, Dokument 1 für Code-Detailbefunde, Dokument 3 für Implementierungsdetails)
- **Ordnet die Reihenfolge neu**, da Dokument 2 überzeugend zeigt, dass Governance-Fixes vor allen Feature-Arbeiten stehen müssen
- Ist als **einziges, vollständiges Arbeitsdokument** gedacht — die drei Ursprungsdokumente werden dadurch abgelöst, nicht ergänzt

---

## 1. Verifikationsprotokoll — was im Code tatsächlich bestätigt wurde

Vor der Zusammenführung wurden die zentralen, strategisch folgenreichsten Behauptungen aller drei Dokumente gegen den Quellcode geprüft. Ergebnis: **Alle geprüften Kernbehauptungen sind korrekt.** Keine der drei Quellen enthält materielle Fehlbehauptungen zu den unten genannten Punkten.

| # | Behauptung | Fundstelle | Status |
|---|---|---|---|
| V1 | `StorageEngine::last_tx_id()` gibt `Result<TxId>` zurück, `VectorIndex`/`TextIndex`/`GraphIndex` geben `Result<u64>` zurück | `crates/memfuse-core/src/traits.rs:192,318,411,629` | ✅ Bestätigt — exakte Zeilen stimmen |
| V2 | `async-trait` wird in 13 Crates als Dependency geführt, obwohl Rust 1.89 native Async-Traits unterstützt | 13 `Cargo.toml`-Dateien, `Cargo.toml:41` (Workspace) | ✅ Bestätigt |
| V3 | `FLUSH_COUNTER` ist ein globales `static AtomicU64`, keine Instanzvariable | `crates/memfuse-store/src/lsm.rs:88` | ✅ Bestätigt |
| V4 | `LEGACY_INTEGRITY_KEY` ist ein menschenlesbarer Byte-String, hartcodiert im Binary | `crates/memfuse-store/src/wal.rs:59` | ✅ Bestätigt (`b"memfuse-integrity-key-v1\0..."`) |
| V5 | `experimental-diskann` ist Teil der `default`-Features von `memfuse-index` | `crates/memfuse-index/Cargo.toml` `[features]`-Block | ✅ Bestätigt (`default = ["experimental-diskann"]`) |
| V6 | `#![allow(deprecated)]` steht auf Crate-Ebene in `memfuse-db/src/lib.rs` | `crates/memfuse-db/src/lib.rs`, Zeile 6 | ✅ Bestätigt |
| V7 | `memfuse-db` (als Layer 2 deklariert) hängt direkt von `memfuse-ollama` (als Layer 3 deklariert) ab | `crates/memfuse-db/Cargo.toml:16` | ✅ Bestätigt |
| V8 | Die Layer-Nummer im `xtask`-Dokumentationsgenerator ist eine **hartcodierte Namens-Lookup-Tabelle**, unabhängig von den tatsächlichen `dependencies` — keine Prüfung `dep.layer < self.layer` existiert | `xtask/src/main.rs:381–389` (`match name.as_str() { "memfuse-core" => 0, ... "memfuse-db" => 2, ... "memfuse-ollama" => 3 ... }`) | ✅ **Bestätigt — dies ist der schwerwiegendste Einzelbefund aller drei Dokumente** |

**Konsequenz von V7 + V8 zusammen**: Das Projekt behauptet in generierter Dokumentation "DAG Integrity: ✅ Erfüllt", während im selben Generator-Lauf ein echter Layer-Verstoß (Layer 2 → Layer 3) vorliegt, der **strukturell nicht erkannt werden kann**, weil die Prüfung selbst nicht existiert — nur eine Textzeile, die immer denselben Status ausgibt. Dies bestätigt Dokument 2 vollständig und ist der Ausgangspunkt für die neue Priorisierung in diesem Masterplan (siehe Abschnitt 3).

---

## 2. Executive Summary

MemFuse ist **keine "LLM-OS"**, sondern eine technisch überdurchschnittlich solide, embedded **Hybrid-Retrieval-Engine** (Vektor + BM25 + Graph + Metadaten-Fusion, MVCC-LSM-Storage mit HMAC-WAL, ACID-Garantien) mit einer noch dünnen Agenten-Orchestrierungsschicht. Die Storage- und Retrieval-Grundlagen sind exzellent gebaut und gut getestet. Der Weg zu einem verteidigbaren "Goldstandard"-Anspruch für lokale SLM/LLM-Systeme führt jedoch **nicht** über mehr Features auf der bestehenden Achse, sondern über die Behebung von fünf ineinandergreifenden Problemklassen:

1. **Governance verifiziert nicht, was sie behauptet zu verifizieren** (DAG-Integritäts-Attrappe, s. o.) — dies muss zuerst gelöst werden, weil jede andere "✅"-Aussage im Projekt sonst nicht vertrauenswürdig ist.
2. **Der "Sovereign Pure-Rust Core"-Anspruch hält der Prüfung nicht stand** — Kern-Kognition (Kontext-Kompression, Konsolidierung, Importance-Scoring) ist compile-time an einen externen Ollama-Prozess gebunden, ohne Trait-Abstraktion.
3. **Reale strukturelle Code-Mängel** in praktisch jedem Crate — von inkonsistenten Rückgabetypen über globale Test-Pollution-Statics bis zu potenziellen Deadlocks durch gemischte Sync/Async-Locks.
4. **Die Entwicklungsgeschwindigkeit des Git-Verlaufs ist irreführend** für Sprintplanung — ein Burst von 40–150 Commits/Tag ab dem 22.08.2026 signalisiert kontinuierlichen Agenten-Einsatz, nicht organisches Team-Wachstum; ein erheblicher Anteil sind Audit-Zyklen ohne Verhaltensänderung.
5. **Ein echtes, unbesetztes Alleinstellungsmerkmal existiert** (KV-Cache-Bridge zwischen Memory-Layer und lokaler Inferenz), ist aber im Original-Masterplan komplett unerwähnt und sollte strategisch priorisiert werden.

---

## 3. Auflösung der Widersprüche zwischen den drei Quelldokumenten

Die drei Dokumente widersprechen sich an vier Stellen materiell. Dieser Abschnitt entscheidet jeden Widerspruch explizit und begründet die Entscheidung — die nachfolgende Roadmap folgt dieser Auflösung.

### 3.1 Widerspruch: Entwicklungsgeschwindigkeit

- **Original-Masterplan** kalibriert auf **19,3 Commits/Tag** im Durchschnitt über 121 Tage und leitet daraus "1 Arbeitstag = 8–12 Commits" ab, mit Phase-2B-Dauer von 9 Wochen.
- **Strategische Bewertung** widerspricht dem direkt: Der Durchschnitt verschleiert, dass Mai–Juli praktisch stillstand (~15 Commits in 3 Monaten) und ab 22.08. auf 40–150 Commits/Tag explodierte — ein klares Signal für Coding-Agenten im Dauerbetrieb, nicht für lineares Team-Wachstum. Realistische Netto-Velocity wird auf **6–10 Netto-Engineering-Tage pro Kalenderwoche** geschätzt, nicht auf naiver Commit-Extrapolation.

**Entscheidung**: Die Analyse der Strategischen Bewertung ist methodisch überlegen (sie differenziert nach Zeiträumen statt einen einzelnen Durchschnitt zu bilden) und wird als Grundlage übernommen. **Alle Zeitschätzungen in diesem Dokument verwenden weiterhin die im Original-Masterplan geschätzten Aufwandstage pro Arbeitspaket** (diese sind detailliert und plausibel hergeleitet), aber die **Kalenderzeit-Hochrechnung wird um 60–80 % gestreckt** gegenüber dem ursprünglichen 9-Wochen-Ziel für Phase 2B, und es wird ein **verpflichtendes menschliches Review-Gate** für sicherheitskritische Änderungen ergänzt (siehe 3.3).

### 3.2 Widerspruch: Reihenfolge der Arbeitspakete

- **Original-Masterplan** beginnt mit ADR-049 (Router-Fix, 1 Tag) und ADR-048 (HNSW-Rebuild) als "Top-3-Sofortmaßnahmen", ADR-051 (Provenance) als dritte Priorität. Governance-Fragen (DAG-Check, ADR-Nummernkollision) kommen im Original-Masterplan gar nicht vor.
- **Strategische Bewertung** fordert explizit: **A8 (echter DAG-Check) und ADR-Governance-Bereinigung müssen zuerst passieren**, weil sonst jedes weitere Arbeitspaket auf einer Dokumentationsbasis aufsetzt, die sich nicht selbst verlässlich prüft.

**Entscheidung**: Die Strategische Bewertung hat hier Vorrang — das ist keine akademische Governance-Frage, sondern in Abschnitt 1 dieses Dokuments direkt im Code verifiziert. **Sprint 0 (Governance-Fix) wird der gesamten Roadmap vorangestellt**, noch vor dem Router-Fix, obwohl Letzterer der geringste Aufwand aller Pakete ist. Der Router-Fix bleibt aber weiterhin die günstigste Verbesserung mit sofortigem Nutzen und folgt direkt danach.

### 3.3 Widerspruch: Fehlende strategische Komponenten im Original-Masterplan

- **Original-Masterplan** enthält keine Trait-Abstraktion für Completion/Reasoning, kein Konzept einer KV-Cache-Bridge, kein Evaluierungs-Framework mit externen Benchmarks, keine formale Crash-Konsistenz-Verifikation, kein Fehlerbudget/SLO für den Agenten-Loop.
- **Strategische Bewertung** identifiziert diese fünf Lücken explizit als das, was zwischen "sehr solide RAG-Engine" und "verteidigbarer Goldstandard" fehlt — und markiert A4 (KV-Cache-Bridge) als **den eigentlichen, aktuell komplett unbesetzten Differenzierungshebel** gegenüber Mem0/Zep/MemGPT.

**Entscheidung**: Alle fünf Ergänzungen werden vollständig in diesen Masterplan integriert (siehe Abschnitt 6, Kategorie A). Sie erhalten eigene Sprints in der Gesamt-Roadmap. Die KV-Cache-Bridge (A4) wird — anders als im Original-Masterplan, wo sie gar nicht vorkommt — explizit als strategischer Leuchtturm-Punkt in Abschnitt 9 hervorgehoben.

### 3.4 Widerspruch: "Sovereign Pure-Rust Core"-Anspruch

- **Original-Masterplan** übernimmt implizit den Anspruch "100 % Pure-Rust / Sovereign Core / Zero-IT-Setup" aus dem README, ohne ihn zu hinterfragen, und plant Enterprise-Features (Phase 4) auf dieser Grundlage.
- **Strategische Bewertung** weist nach, dass dieser Anspruch **nicht zutrifft**: Kern-Kognition ist zwingend an einen extern zu installierenden Ollama-Prozess gebunden, was der "Zero-IT-Setup"-Aussage im selben README widerspricht.

**Entscheidung**: Der Anspruch wird korrigiert. Dieses Dokument übernimmt die ehrlichere Positionierung der Strategischen Bewertung: *"MemFuse ist die Gedächtnis- und Retrieval-Schicht für SLM/LLM-Agenten, backend-agnostisch"* — mit dem `LlmCompletionEngine`-Trait (A1, s. u.) als Weg, diesen Anspruch **nachträglich wahr zu machen**, statt ihn zu behaupten, ohne ihn einzulösen.

---
