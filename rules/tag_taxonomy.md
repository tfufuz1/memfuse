# Tag-Taxonomie — MemFuse Kommentarsystem
> Einzige kanonische Definition aller Tag-Typen. Verstöße = SMELL[CRITICAL].

## Übersicht der drei Systeme

| System | Zweck | Format |
|---|---|---|
| `AI-TAG` | Aktuelle Probleme/Risiken | `AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung (ID: AGT-<CRATE>-<hash>) (TS: <ISO-8601-UTC>) (SESSION: <hash>)` |
| `ANCHOR` | Geplante/laufende Arbeit | `ANCHOR[TYP:ID] STATUS:X (TS: <ISO-8601-UTC>) (SESSION: <hash>)` |
| `REVIEW-PASS` | Unabhängiges Mehrfach-Session-Review | `REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL (ID: AGT-<CRATE>-<hash>) (TS: <ISO-8601-UTC>) (SESSION: <hash>)` |
| `FILE-CONTEXT` | Datei-Kontext für Agenten | `// FILE-CONTEXT` Block mit `// STAND: <TS> (SESSION: <hash>)` |

## Sekundengenaue Zeitstempel-Pflicht (TS:<ISO-8601-UTC>), SESSION-Token & Hash-IDs

Alle Tag-Typen (`AI-TAG`, `ANCHOR`, `REVIEW-PASS`) sowie der `FILE-CONTEXT`-Header haben drei VERPFLICHTENDE maschinenlesbare Pflichtfelder:
1. **`TS:`-Zeitstempel**: Sekundengenaues Format `YYYY-MM-DDTHH:MM:SSZ` (z.B. `2026-08-29T09:14:07Z`), ermittelt via `date -u +%Y-%m-%dT%H:%M:%SZ`.
2. **`SESSION:`-Token**: 8-stelliger Hex-Hash der Jules-Sitzung (z.B. `SESSION:a3f29c1d`), bereitgestellt durch das Environment-Setup-Skript (`[10/10] Session identity`).
3. **Hash-basierte ID**: `AGT-<CRATE>-<8-hex-hash>` für ALLE NEUEN Tags (z.B. `AGT-STORE-a3f29c1d`). Der Hash entspricht den ersten 8 Zeichen von `sha256(crate + dateipfad + zeile_bei_erstellung + ts)`.
   - *Bestandsschutz Schnittpunkt*: Bestehende `AGT-<CRATE>-NNN`-IDs (erstellt vor dem 2026-08-29 / Prompt 06) werden aus Kompatibilität mit ADR-Referenzen in `DECISIONS.md` NICHT rückwirkend migriert.

Ein Tag ohne `TS:`- oder `SESSION:`-Feld gilt als Grammatikverstoß (wird von CI Gate 7 durchgesetzt).

## AI-TAG

Kanonisches Pflichtformat:
```rust
// AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung (ID: AGT-STORE-a3f29c1d) (TS: 2026-08-29T09:14:07Z) (SESSION: a3f29c1d)
// BEFUND: Detaillierte Analyse des Ist-Zustands im Code.
// RISIKO: Detaillierte Risikobewertung.
// EMPFEHLUNG: Konkrete Handlungsempfehlung.
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
// ANCHOR[TYP:ID] STATUS:OPEN (TS: 2026-08-29T09:14:07Z) (SESSION: a3f29c1d)
// AUFGABE : <Was zu implementieren ist>
// GATE    : cargo test -p <crate> --test <testname>
```

Typen: `INTEGRATION` | `DEBT` | `REFACTOR` | `TEST` | `ALG-FIX` | `PERF` | `SECURITY`
```

Status-Werte:
- `OPEN` — nicht begonnen
- `IN-PROGRESS AGENT:N` — aktuell in Bearbeitung
- `DONE` — abgeschlossen
- `BLOCKED REASON:<...>` — blockiert

Bei jedem Status-Wechsel (`IN-PROGRESS`, `DONE`, `BLOCKED`, `RESOLVED`) MUSS ein neuer, aktueller `TS:`-Wert sowie der `SESSION:`-Hash der jeweiligen Sitzung gesetzt werden.

## REVIEW-PASS (Unabhängiges Mehrfach-Session-Review)

Grammatik:
```rust
// REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL (ID: AGT-STORE-a3f29c1d) (TS: 2026-08-29T10:15:00Z) (SESSION: b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: <Was diese Prüfsitzung gefunden/bestätigt hat>
// ABWEICHEND-VON-VORGÄNGER: <Pflichtfeld bei Widerspruch zu einem vorherigen Pass>
```

- `PRÜFER-KONTEXT: FRESH` ist Pflicht (Sitzung hatte keine vorherige Historiensicht auf diesen Diff).
- Jede `STATUS:DONE`-Markierung eines `ANCHOR` erfordert 2 (Standard) bzw. 3 (`ASK`/security/unsafe) `REVIEW-PASS` Einträge von unterschiedlichen `SESSION:`-Hashes.

## FILE-CONTEXT Header

Format für nicht-triviale `.rs` Dateien (> 50 Zeilen, mit bekannten Fallstricken):

```rust
// FILE-CONTEXT
// STAND: 2026-08-29T09:14:07Z (SESSION: a3f29c1d)
// ZWECK: <Ein Satz — was diese Datei tut>
// INVARIANTEN: <Was bei jeder Änderung gelten MUSS>
// NICHT-OFFENSICHTLICH: <Entscheidungen, die ohne dieses Wissen zu falschem Code führen>
// SIEHE AUCH: <Pfade zu ADRs/rules/*.md>
// AGENT-NOTIZ: <Optional, max. 1 Satz. Was ein Agent dem NÄCHSTEN Agenten mitteilen will>
```

Maximale Länge: 8 Zeilen. Kein Ersatz für Rustdoc.

> **Bestandsschutz**: Bestehende FILE-CONTEXT-Header ohne SESSION:-Token
> (angelegt vor 2026-08-29) behalten Gültigkeit. SESSION: ist nur für
> NEU angelegte oder aktualisierte FILE-CONTEXT-Header verpflichtend.

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
