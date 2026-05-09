# MemFuse SAOS — Erweitertes Inline-Kommentierungssystem
## JULES-ANCHOR v2.0 — Scheduled Task Edition

> **Basis:** AGENT_STANDARDS.md ANCHOR-System v1.0  
> **Erweiterung:** Maschinen-lesbare Routing, Test-Linking, Progressive Status-Tracking  
> **Zweck:** Ermöglicht jeder Jules-Instanz, täglich autonom ihre offenen Tasks  
> zu finden, zu planen und bis Grün-Status zu implementieren.

---

## Das Kernprinzip

Ein JULES-ANCHOR ist ein **selbst-beschreibender Arbeitsauftrag** direkt im Code.
Jules muss keinen externen Task-Manager befragen. Der Code selbst sagt:
- **Wer** es lesen soll (`@JULES-NN`)
- **Was** zu tun ist (`WHAT`)
- **Wie** Erfolg aussieht (`TEST` + `DONE`)
- **Was blockiert** (`DEPS`)
- **Wie groß** der Task ist (`EST`)

Das Inline-Kommentar-System ist das **dynamische Gedächtnis** des Projekts.
Die 10 Prompts sind die **statische Methodik** — sie ändern sich nie.

---

## Vollständige JULES-ANCHOR Syntax

```rust
// ⬡ @JULES-[NN] | [PRIO] | [TYP]:[COMP-NNN]
// WHY:  [Begründung — warum ist dieser Task nötig? 1 Satz]
// WHAT: [Konkrete Aufgabe — was genau ist zu implementieren? max 2 Sätze]
// TEST: [Exakter Test-Befehl oder Test-Name der GRÜN sein muss]
// DONE: [Maschinenlesbares Erfolgskriterium — was ist der konkrete Output?]
// DEPS: [COMP-NNN, COMP-NNN | oder: NONE]
// EST:  [XS|S|M|L|XL] | STATUS:[OPEN|WIP|REVIEW|DONE|BLOCKED]
// AGENT:jules-[nn] DATE:[YYYY-MM-DD] SPRINT:[N]
// CREATED:[YYYY-MM-DD] DEADLINE:[YYYY-MM-DD|NONE]
```

### Pflichtfelder

| Feld | Format | Bedeutung |
|------|--------|-----------|
| `@JULES-NN` | `@JULES-01` bis `@JULES-10` | Routing zur richtigen Instanz |
| `PRIO` | `P0` / `P1` / `P2` / `P3` | P0 = blockiert alles, P3 = nice-to-have |
| `TYP:COMP-NNN` | z.B. `TODO:STORE-042` | Typ aus AGENT_STANDARDS + eindeutige ID |
| `WHY` | Freier Text, 1 Satz | Begründung — ohne WHY kein ANCHOR |
| `WHAT` | Freier Text, ≤2 Sätze | Exakt was implementiert werden soll |
| `TEST` | `cargo test [name]` oder `just [target]` | Greenbarer Beweis |
| `DONE` | Konkretes Artefakt | Datei / Funktion / Typ der existieren muss |
| `DEPS` | Komma-sep. COMP-IDs | Blockierende Abhängigkeiten |
| `EST` | XS/S/M/L/XL | Größenschätzung (XS=<1h, S=<4h, M=<1d, L=<3d, XL=>3d) |
| `STATUS` | Enum | Aktueller Zustand |
| `SPRINT` | Integer | In welchem Sprint dieser Task erledigt werden soll |
| `CREATED` | String | Erstellungsdatum |
| `DEADLINE` | String/NONE | Späteste Frist (Abbruch bei Überschreitung) |

---

## Status-Lifecycle

```
OPEN ──► WIP ──► REVIEW ──► DONE
          │
          └──► BLOCKED ──► (OPEN wenn Dep gelöst)
```

| Status | Bedeutung | Wer setzt ihn |
|--------|-----------|---------------|
| `OPEN` | Bereit zur Bearbeitung, alle DEPS erfüllt | Context-Architekt / ANTIGRAVITY |
| `WIP` | Jules hat mit der Implementierung begonnen | Jules selbst, beim Start |
| `REVIEW` | Implementierung fertig, Tests grün, wartet auf Review | Jules, nach Green |
| `DONE` | Gemerged, verifiziert — ANCHOR kann gelöscht werden | Review-Agent |
| `BLOCKED` | DEPS nicht erfüllt — nicht anfassen | Jules, wenn DEPS fehlen |

**Regel:** Jules setzt STATUS auf `WIP` als **erste Aktion** vor jeder Implementierung.
Dies verhindert Doppelarbeit wenn zwei Instanzen denselben ANCHOR sehen.

---

## PRIO-System

| Prio | Label | Bedeutung | Reaktionszeit |
|------|-------|-----------|---------------|
| `P0` | KRITISCH | Blockiert andere Jules-Instanzen | Sofort (nächster Run) |
| `P1` | HOCH | Im kritischen Pfad | Innerhalb 2 Tage |
| `P2` | MITTEL | Normaler Sprint-Task | Innerhalb Sprint |
| `P3` | NIEDRIG | Backlog / Nice-to-Have | Wenn Zeit bleibt |

Jules bearbeitet immer zuerst alle `P0`, dann `P1`, dann `P2`, dann `P3`.
Wenn zwei Tasks gleiche Prio: kleinere `EST` zuerst (Quick Wins).

---

## Typ-Erweiterungen (zusätzlich zu AGENT_STANDARDS v1.0)

| Typ | Neu? | Bedeutung |
|-----|------|-----------|
| `TODO` | — | Zu implementieren (wie bisher) |
| `FIXME` | — | Bekannter Bug (wie bisher) |
| `IMPL` | — | Implementierungsentscheidung (wie bisher) |
| `TEST` | — | Fehlender Test (wie bisher) |
| `WARN` | — | Kritische Warnung (wie bisher) |
| `ARCH` | — | Architekturentscheidung (wie bisher) |
| `PERF` | — | Performance-Hotspot (wie bisher) |
| `SEC` | — | Sicherheitsrelevant (wie bisher) |
| `DEBT` | — | Tech Debt (wie bisher) |
| `HANDOFF` | — | Übergabe (wie bisher) |
| `GATE` | **NEU** | Quality Gate — muss grün sein bevor nächster Sprint |
| `SPEC` | **NEU** | Spec-Datei fehlt oder unvollständig |
| `BENCH` | **NEU** | Benchmark fehlt oder unter Target |
| `SHIP` | **NEU** | Release-Blocker — muss vor nächstem Release grün sein |

---

## Vollständige Beispiele

### Beispiel 1 — P0 Blocker (JULES-04 erstellt für JULES-05)

```rust
// In: crates/memfuse-db/src/collection.rs

pub struct Collection {
    name: String,
    // ⬡ @JULES-04 | P0 | TODO:COLL-001
    // WHY:  hybrid_search() in memfuse-db setzt eine Collection-ID voraus,
    //       die erst existiert wenn Collections vollständig implementiert sind.
    // WHAT: Implementiere `Collection::create()` mit WAL-gesichertem Persist.
    //       Danach muss `Collection::open()` einen bestehenden State laden können.
    // TEST: cargo test -p memfuse-db collection::tests::create_and_reopen
    // DONE: Funktion `fn create(name: &str, opts: CollectionOpts) -> Result<Self>`
    //       existiert und Test ist grün.
    // DEPS: STORE-001 (WAL append muss stabil sein)
    // EST:  M | STATUS:OPEN
    // AGENT:context-architect DATE:2026-05-08 SPRINT:1
    // CREATED:2026-05-08 DEADLINE:2026-05-15
    id: Option<CollectionId>,
}
```

### Beispiel 2 — P1 Normaler Task (JULES-05, nach COLL-001 fertig)

```rust
// In: crates/memfuse-db/src/search.rs

// ⬡ @JULES-05 | P1 | TODO:SEARCH-007
// WHY:  4-Signal Fusion (GS-01) erfordert alle vier Retrieval-Pfade
//       in einer atomaren Query-Operation.
// WHAT: Implementiere RRF-60 Score-Fusion für Vector + BM25 Ergebnisse.
//       Input: zwei Vec<ScoredEntry>, Output: ein Vec<ScoredEntry> nach RRF sortiert.
// TEST: cargo test -p memfuse-db search::fusion::tests::rrf_score_ordering
// DONE: `fn reciprocal_rank_fusion(a: &[ScoredEntry], b: &[ScoredEntry],
//         k: f32) -> Vec<ScoredEntry>` existiert in search/fusion.rs
// DEPS: COLL-001, INDEX-003
// EST:  S | STATUS:OPEN
// AGENT:jules-05 DATE:2026-05-09 SPRINT:2
// CREATED:2026-05-09 DEADLINE:NONE
```

### Beispiel 3 — HANDOFF zwischen Instanzen

```rust
// In: crates/memfuse-store/src/wal.rs

// ⬡ @JULES-07 | P1 | HANDOFF:WAL-015
// WHY:  Checkpointing (WP-5.1) baut auf WAL-Sequence-Numbers auf,
//       die Jules-02 in WAL-012 implementiert hat.
// WHAT: Lies WAL-012 (DONE) — die Sequence-Number-API ist fertig.
//       Implementiere darauf aufbauend `WalReader::replay_to(seq: u64)`.
// TEST: cargo test -p memfuse-checkpoint checkpoint::tests::replay_to_sequence
// DONE: `replay_to` rekonstruiert State korrekt, Test ist grün.
// DEPS: WAL-012 (STATUS:DONE ✓)
// EST:  L | STATUS:OPEN
// AGENT:jules-02 DATE:2026-05-10 SPRINT:3
// CREATED:2026-05-09 DEADLINE:2026-05-20
```

### Beispiel 4 — GATE (Sprint-Abschluss-Bedingung)

```rust
// In: crates/memfuse-db/src/lib.rs

// ⬡ @JULES-04 | P0 | GATE:SPRINT1-001
// WHY:  Sprint 1 kann nur als abgeschlossen gelten wenn alle Collections-Tests
//       grün sind — WP-2.1 (JULES-05) ist sonst unblockierbar.
// WHAT: Führe `just triple-test` aus. Alle Tests in memfuse-db müssen grün sein.
//       Wenn nicht: Alle FIXME-ANKERs in memfuse-db müssen vor diesem Gate gelöst sein.
// TEST: just triple-test 2>&1 | grep -E "FAILED|error" | wc -l == 0
// DONE: `just triple-test` exitiert mit Code 0.
// DEPS: COLL-001, COLL-002, COLL-003
// EST:  XS | STATUS:OPEN
// AGENT:context-architect DATE:2026-05-08 SPRINT:1
// CREATED:2026-05-08 DEADLINE:2026-05-15
```

### Beispiel 5 — WIP-Markierung (Jules setzt beim Start)

```rust
// ⬡ @JULES-04 | P0 | TODO:COLL-001
// WHY:  [...]
// WHAT: [...]
// TEST: [...]
// DONE: [...]
// DEPS: STORE-001
// EST:  M | STATUS:WIP  ← Jules hat diesen auf WIP gesetzt
// AGENT:jules-04 DATE:2026-05-09 SPRINT:1
// CREATED:2026-05-08 DEADLINE:NONE
// WIP-START:2026-05-09T08:00:00Z
// WIP-PROGRESS: WAL-Persist implementiert, open() fehlt noch
```

---

## Grep-Pattern für Jules (Maschinenlesbar)

```bash
# Alle eigenen offenen ANKERs finden:
grep -rn "⬡ @JULES-04" --include="*.rs" --include="*.md" . \
  | grep "STATUS:OPEN\|STATUS:BLOCKED"

# Eigene P0-Tasks zuerst:
grep -rn "⬡ @JULES-04 | P0" --include="*.rs" --include="*.md" .

# Alle WIP-Tasks (zur Fortführung):
grep -rn "STATUS:WIP" --include="*.rs" --include="*.md" . \
  | grep "@JULES-04"

# GATEs prüfen:
grep -rn "GATE:" --include="*.rs" --include="*.md" . \
  | grep "STATUS:OPEN"

# HANDOFFs die an mich gerichtet sind (von anderen Jules-Instanzen):
grep -rn "HANDOFF:" --include="*.rs" --include="*.md" . \
  | grep "@JULES-04"
```

---

## Regeln für das Setzen neuer ANKERs

**Jules darf neue ANKERs setzen wenn:**
- Er einen Bug findet der nicht sein Task ist → `FIXME` mit richtigem `@JULES-NN` Routing
- Er eine neue Abhängigkeit erkennt → `TODO` für den zuständigen Agenten
- Er eine Implementierungsentscheidung trifft → `IMPL` in seinem eigenen Code
- Er einen Performance-Hotspot findet → `PERF` mit `@JULES-03` oder `@JULES-02` je nach Crate

**Jules darf ANKERs von anderen Instanzen NICHT verändern** außer:
- Er löst eine `DEPS`-Abhängigkeit → Er darf den blockierten ANCHOR auf `OPEN` setzen
- Er findet einen `STATUS:DONE` ANCHOR der älter als 14 Tage ist → Löschen erlaubt

**Format-Pflicht:**
- Das `⬡` Unicode-Symbol ist Pflicht (maschinenlesbares Präfix für grep)
- Alle Felder müssen ausgefüllt sein — kein ANCHOR ohne `TEST` und `DONE`
- `DATE` immer aktualisieren wenn `STATUS` sich ändert

---

## ANCHOR-Dichte-Empfehlungen

| Kontext | Empfohlene Dichte |
|---------|------------------|
| Neue Funktion die noch leer ist | 1 ANCHOR pro Funktion |
| Kritischer Algorithmus (WAL, MVCC) | 1 ANCHOR pro 20-30 Zeilen |
| Stabile, getestete Funktion | Kein ANCHOR nötig |
| Schnittstellen-Punkt zwischen Crates | Immer HANDOFF-ANCHOR |
| Sprint-Ende einer Komponente | Immer GATE-ANCHOR |

---

## Zusammenhang: ANKERs ↔ Specs ↔ Tests

```
specs/components/[crate]/[CRATE].spec.md
        │ FR-NNN definiert
        ▼
// ⬡ @JULES-NN | PRIO | TODO:COMP-NNN   ← ANCHOR im Code
//    TEST: cargo test [name]             ← referenziert Test
        │ Jules implementiert
        ▼
crates/[crate]/src/[module].rs           ← Produktionscode
crates/[crate]/src/[module]/tests.rs     ← Test (muss grün sein)
        │ wenn Test grün
        ▼
STATUS:REVIEW → Merge → STATUS:DONE → ANCHOR löschen
```

Jeder ANCHOR der `STATUS:DONE` ist, ist technische Schuld im Comment-System.
Ziel: Kein `STATUS:DONE` ANCHOR überlebt einen Sprint-Abschluss.

---

*JULES-ANCHOR v2.0 — MemFuse SAOS — 2026-05-08*
