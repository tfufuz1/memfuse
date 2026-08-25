# Tag-Taxonomie — MemFuse Kommentarsystem
> Einzige kanonische Definition aller Tag-Typen. Verstöße = SMELL[CRITICAL].

## Übersicht der drei Systeme

| System | Zweck | Format |
|---|---|---|
| `AI-TAG` | Aktuelle Probleme/Risiken | `AI-TAG[KATEGORIE][SEVERITY]` |
| `ANCHOR` | Geplante/laufende Arbeit | `ANCHOR[TYP:ID] STATUS:X` |
| `FILE-CONTEXT` | Datei-Kontext für Agenten | `// FILE-CONTEXT` Block |

## AI-TAG (aus llm_protocol.md §3 — dort ist die primäre Definition)

Siehe `rules/llm_protocol.md §3` für vollständige Spezifikation.
Kurzformat: `AI-TAG[KATEGORIE][SEVERITY] Titel (ID: AGT-NNNN)`

Severity-Stufen und CI-Verhalten:
- `BLOCKER` → CI bricht ab (Gate 1)
- `CRITICAL` → CI bricht ab (Gate 1)
- `MAJOR` → CI warnt
- `MINOR` → nur getrackt

Abschluss: Kommentar mit `RESOLVED: AGT-XXXX — <fix> (YYYY-MM-DD)` versehen.

## ANCHOR (kanonische Definition)

```
// ANCHOR[TYP:ID] STATUS:OPEN
// AUFGABE : <Was zu implementieren ist>
// GATE    : cargo test -p <crate> --test <testname>
```

Typen: `INTEGRATION` | `DEBT` | `REFACTOR` | `TEST` | `ALG-FIX` | `PERF` | `SECURITY`

Status-Werte:
- `OPEN` — nicht begonnen
- `IN-PROGRESS AGENT:N` — aktuell in Bearbeitung
- `DONE DATE:YYYY-MM-DD` — abgeschlossen
- `BLOCKED REASON:<...>` — blockiert

## FILE-CONTEXT Header

Format für nicht-triviale `.rs` Dateien (> 50 Zeilen, mit bekannten Fallstricken):

```rust
// FILE-CONTEXT
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
| `rules/tag_taxonomy.md` | Tag-Definitionen | rust-ci.yml (Gate 1) |
| `AGENTS.md §4` | unsafe-Scope | rust-ci.yml (Gate 4) |
| `AGENTS.md §3` | DAG-Schichten | dag-check.yml |
