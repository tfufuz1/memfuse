# Tag-Taxonomie — MemFuse Kommentarsystem
> Einzige kanonische Definition aller Tag-Typen. Verstöße = SMELL[CRITICAL].

## Übersicht der drei Systeme

| System | Zweck | Format |
|---|---|---|
| `AI-TAG` | Aktuelle Probleme/Risiken | `AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung (ID: AGT-XXX) (TS: <ISO-8601-UTC>)` |
| `ANCHOR` | Geplante/laufende Arbeit | `ANCHOR[TYP:ID] STATUS:X (TS: <ISO-8601-UTC>)` |
| `FILE-CONTEXT` | Datei-Kontext für Agenten | `// FILE-CONTEXT` Block mit `// STAND: <TS>` |

## Zeitstempel-Pflicht (TS:<ISO-8601-UTC>)

Alle Tag-Typen (`AI-TAG`, `ANCHOR`) sowie der `FILE-CONTEXT`-Header haben ein VERPFLICHTENDES, maschinenlesbares ISO-8601-UTC-Zeitstempel-Feld.
- Exaktes Format: `YYYY-MM-DDTHH:MM:SSZ` (z.B. `2026-08-27T14:32:00Z`).
- Ein Tag ohne `TS:`-Feld gilt als Grammatikverstoß (wird von CI Gate 7 durchgesetzt, analog zu Gate 6 für TODOs).

## AI-TAG (aus llm_protocol.md §3 — dort ist die primäre Definition)

Siehe `rules/llm_protocol.md §3` für vollständige Spezifikation.
Pflichtformat:
```rust
// AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung (ID: AGT-XXX) (TS: 2026-08-27T14:32:00Z)
```

Severity-Stufen und CI-Verhalten:
- `BLOCKER` → CI bricht ab (Gate 1)
- `CRITICAL` → CI bricht ab (Gate 1)
- `MAJOR` → CI warnt
- `MINOR` → nur getrackt

Abschluss: Kommentar mit `RESOLVED` und `TS:` versehen:
```rust
// RESOLVED: AGT-XXXX — <fix> (TS: 2026-08-27T15:10:00Z)
```

## ANCHOR (kanonische Definition)

```rust
// ANCHOR[TYP:ID] STATUS:OPEN (TS: 2026-08-27T14:32:00Z)
// AUFGABE : <Was zu implementieren ist>
// GATE    : cargo test -p <crate> --test <testname>
```

Typen: `INTEGRATION` | `DEBT` | `REFACTOR` | `TEST` | `ALG-FIX` | `PERF` | `SECURITY`

Status-Werte:
- `OPEN` — nicht begonnen
- `IN-PROGRESS AGENT:N` — aktuell in Bearbeitung
- `DONE` — abgeschlossen
- `BLOCKED REASON:<...>` — blockiert

Bei jedem Status-Wechsel (`IN-PROGRESS`, `DONE`, `BLOCKED`, `RESOLVED`) MUSS ein neuer, aktueller `TS:`-Wert gesetzt werden — der Zeitstempel spiegelt immer den Zeitpunkt des LETZTEN Status-Wechsels wider, nicht der Erstellung.

## FILE-CONTEXT Header

Format für nicht-triviale `.rs` Dateien (> 50 Zeilen, mit bekannten Fallstricken):

```rust
// FILE-CONTEXT
// STAND: 2026-08-27T14:32:00Z
// ZWECK: <Ein Satz — was diese Datei tut>
// INVARIANTEN: <Was bei jeder Änderung gelten MUSS>
// NICHT-OFFENSICHTLICH: <Entscheidungen, die ohne dieses Wissen zu falschem Code führen>
// SIEHE AUCH: <Pfade zu ADRs/rules/*.md>
```

Maximale Länge: 8 Zeilen. Kein Ersatz für Rustdoc.

## AGENT-Register (WORKING_STATE.md führen)

`AGENT:N` in Kommentaren MUSS einem Eintrag in `WORKING_STATE.md` entsprechen.
Format dort: `| AGENT:N | YYYY-MM-DD | <Session-Beschreibung> |`

## CI-Enforcement-Status

| Datei | Inhalt | Gate |
|---|---|---|
| `rules/llm_protocol.md` | Test-Gate-Pflicht | rust-ci.yml (test job) |
| `rules/tag_taxonomy.md` | Tag-Definitionen | rust-ci.yml / context-gates.yml (Gate 1, Gate 7) |
| `AGENTS.md §4` | unsafe-Scope | rust-ci.yml (Gate 4) |
| `AGENTS.md §3` | DAG-Schichten | dag-check.yml |
