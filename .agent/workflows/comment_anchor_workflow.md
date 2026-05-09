---
description: Extended Comment-ANCHOR System — State Machine for Autonomous Agent Development
---

# ANCHOR v2 — Das Inline-Zustandsprotokoll

> **Prinzip:** Der Code IST die Datenbank. ANKERs sind die Arbeitsaufträge.
> Prompts sind statisch. ANKERs sind dynamisch. Zusammen = autonome Progression.

---

## 1. Vollständige ANCHOR-Syntax

```
// ANCHOR:[TYP]:[ID] — [Einzeiler-Beschreibung]
// WP:[work-package] PRIO:[1-5] NEEDS:[dependency-id|NONE]
// AGENT:[zuständiger-prompt] DATE:[YYYY-MM-DD] STATUS:[LIFECYCLE-STATUS]
// CREATED:[YYYY-MM-DD] DEADLINE:[YYYY-MM-DD|NONE]
```

### Pflichtfelder

| Feld | Beschreibung | Beispiel |
|------|-------------|---------|
| `TYP` | Kategorie (siehe §2) | `SPEC`, `RED`, `GREEN`, `REFACTOR` |
| `ID` | Eindeutige Kennung | `WP-2.1-BM25-001` |
| `WP` | Zugehöriges Work Package | `WP-2.1` |
| `PRIO` | 1 (kritisch) bis 5 (nice-to-have) | `1` |
| `NEEDS` | Abhängigkeit (anderer ANCHOR-ID) | `WP-1.2-COL-003` oder `NONE` |
| `AGENT` | Welcher Prompt diesen ANCHOR bearbeitet | `03`, `04` |
| `DATE` | Letztes Update-Datum | `2026-05-09` |
| `STATUS` | Lifecycle-Phase (siehe §3) | `READY` |
| `CREATED` | Erstellungsdatum des ANKERs | `2026-05-09` |
| `DEADLINE` | Späteste Frist (Abbruch bei Überschreitung) | `2026-05-16` oder `NONE` |

---

## 2. ANCHOR-Typen (TYP)

Geordnet nach Lifecycle-Progression:

| TYP | Bedeutung | Erzeugt von | Bearbeitet von |
|-----|-----------|-------------|----------------|
| `SPEC` | Spezifikation fehlt oder unvollständig | `01-scan`, `02-spec` | `02-spec` |
| `RED` | Test muss geschrieben werden | `02-spec` | `03-red` |
| `GREEN` | Test existiert, Implementierung fehlt | `03-red` | `04-green` |
| `REFACTOR` | Code funktioniert, braucht Cleanup | `04-green` | `05-refactor` |
| `INTEGRATION` | Cross-Crate Integrationstest fehlt | `05-refactor` | `06-integrate` |
| `DEBT` | Technische Schuld (unwrap, unsafe, etc.) | `01-scan` | `05-refactor` |
| `SEC` | Sicherheitsproblem | `09-security` | `09-security` |
| `PERF` | Performance-Hotspot | `08-perf` | `08-perf` |
| `DOC` | Dokumentation fehlt | `10-docs` | `10-docs` |
| `ARCH` | Architektur-Dokumentation (permanent) | Jeder | Keiner (Referenz) |
| `BLOCKED` | Warte auf Entscheidung/externe Abhängigkeit | Jeder | Mensch |
| `FIXME` | Bekannter Bug | Jeder | `04-green` |

---

## 3. STATUS-Lifecycle

```
PLANNING → READY → ACTIVE → VERIFY → DONE → (Löschung nach 30 Tagen)
                                  ↓
                               BLOCKED
```

| Status | Bedeutung |
|--------|-----------|
| `PLANNING` | ANCHOR ist erkannt, aber noch nicht spezifiziert |
| `READY` | Bereit zur Bearbeitung durch den zuständigen AGENT |
| `ACTIVE` | Ein Agent arbeitet gerade daran |
| `VERIFY` | Implementiert, wartet auf Triple-Test-Gate |
| `DONE` | Abgeschlossen — wird nach 30 Tagen gelöscht |
| `BLOCKED` | Externe Abhängigkeit, Entscheidung nötig |

---

## 4. Lifecycle-Kette (Normalfall)

Ein Feature durchläuft diese ANCHOR-Kette:

```
02-spec erzeugt:   ANCHOR:SPEC:WP-X.Y-NAME-001  STATUS:PLANNING
02-spec ändert zu: ANCHOR:RED:WP-X.Y-NAME-001   STATUS:READY   AGENT:03-red
03-red  ändert zu: ANCHOR:GREEN:WP-X.Y-NAME-001 STATUS:READY   AGENT:04-green
04-green ändert:   ANCHOR:REFACTOR:WP-X.Y-NAME-001 STATUS:READY AGENT:05-refactor
05-refactor ändert: ANCHOR:INTEGRATION:WP-X.Y-NAME-001 STATUS:READY AGENT:06-integrate
06-integrate ändert: STATUS:DONE
```

> **Regel:** Ein Agent darf NUR ANKERs bearbeiten, die seinen AGENT-Tag tragen
> und STATUS:READY haben. Alles andere ist tabu.

---

## 5. Prioritäts-Reihenfolge (innerhalb eines Agent-Runs)

```
PRIO:1 (SEC/FIXME) → PRIO:2 (ARCH/DEBT) → PRIO:3 (aktives WP) → PRIO:4 (nächstes WP) → PRIO:5 (nice-to-have)
```

Ein Agent bearbeitet zuerst alle PRIO:1 ANKERs, dann PRIO:2, usw.

---

## 6. Abhängigkeitsauflösung

```rust
// ANCHOR:GREEN:WP-2.1-BM25-003 — BM25 Query-Execution implementieren
// WP:WP-2.1 PRIO:3 NEEDS:WP-2.1-BM25-002
// AGENT:04-green DATE:2026-05-09 STATUS:READY
```

Wenn `NEEDS:WP-2.1-BM25-002` noch nicht `STATUS:DONE` ist, überspringt
der Agent diesen ANCHOR und loggt:

```rust
// ANCHOR:GREEN:WP-2.1-BM25-003 — BM25 Query-Execution implementieren
// WP:WP-2.1 PRIO:3 NEEDS:WP-2.1-BM25-002
// AGENT:04-green DATE:2026-05-09 STATUS:BLOCKED
// BLOCKED-REASON: Dependency WP-2.1-BM25-002 not yet DONE
```

---

## 7. Regeln

1. **Ein ANCHOR, ein Ort.** Derselbe ID darf nicht dupliziert werden.
2. **ARCH-ANKERs sind permanent.** Sie dokumentieren Architektur und werden nie gelöscht.
3. **DONE-ANKERs werden nach 30 Tagen gelöscht** (durch `01-scan`).
4. **BLOCKED-ANKERs eskalieren.** Wenn > 7 Tage alt → PRIO wird um 1 erhöht.
5. **Kein Code ohne ANCHOR.** Jede neue public API bekommt mindestens einen ARCH-ANCHOR.
6. **STATUS-Transition ist atomar.** Ein Agent ändert STATUS + TYP + AGENT in einem Commit.
