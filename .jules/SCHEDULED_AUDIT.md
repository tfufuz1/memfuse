# Scheduled Proactive Audit & Mutation Testing Rotation

This document describes the automated weekly proactive audit pipeline and mutation testing rotation configured in `.github/workflows/scheduled-audit.yml`.

---

## 1. Setup & Requirements

### Einmaliges Erstellen des Labels `jules-audit`
Falls das Label `jules-audit` im Repository noch nicht existiert, erstelle es einmalig per `gh`-CLI:

```bash
gh label create jules-audit --description "Automatisch generierter Jules-Audit-Auftrag" --color FBCA04
```

---

## 2. Funktionsweise des Workflows (`.github/workflows/scheduled-audit.yml`)

Der Workflow wird automatisch jeden **Freitag um 22:00 UTC** (`0 22 * * 5`) sowie manuell via `workflow_dispatch` ausgeführt.

Er besteht aus zwei unabhängigen Jobs:

### Job 1: `prepare-audit-context`
- Sammelt alle Commit-Logs der letzten 7 Tage (`git log --since="7 days ago" --oneline --no-merges`).
- Erstellt automatisch ein GitHub-Issue mit dem Titel `Proaktiv-Audit YYYY-MM-DD` und den Labels `jules-audit`, `automated`.
- Der Issue-Text verweist strikt auf `.jules/AUDIT_INTAKE_PROTOCOL.md` und fordert die Prüfung auf:
  1. Race Conditions / TOCTOU-Fehler
  2. Neue `.unwrap()` / `.expect()` außerhalb der Baseline (`.unwrap-baseline.txt`)
  3. Silent-Failure-Pattern (`let _ = ...` bei I/O-Operationen)
  4. DAG-Grenzverletzungen in `Cargo.toml`-Dependencies

### Job 2: `trigger-mutation-testing`
- Berechnet die ISO-Kalenderwoche (`KW % 4`).
- Rotiert wöchentlich durch die 4 Fokus-Crates:
  - **Woche 0**: `memfuse-graph`
  - **Woche 1**: `memfuse-index`
  - **Woche 2**: `memfuse-agent`
  - **Woche 3**: `memfuse-db`
- Löst für den berechneten Fokus-Crate den `mutation-testing.yml`-Workflow via `workflow_dispatch` aus.

---

## 3. Workflow für die Bearbeitung (ChatOps / Escalation)

Da kein direkter Jules-GitHub-App-Webhook automatisch ohne Zuweisung reagiert:
1. Mensch/Operator erhält Benachrichtigung über das neue Issue mit Label `jules-audit`.
2. Zuweisung an Jules via Issue-Kommentar: `@google-jules löse das Problem` (oder Übergabe des Issue-Inhalts in den Jules-Prompt).
3. Jules arbeitet den Auftrag gemäß `.jules/AUDIT_INTAKE_PROTOCOL.md` ab.

---

## Audit-Workflow v2 (nach Review 2026-09)

1. Automatisches Audit-Issue wird wie bisher erstellt.
2. Agent führt statische Analyse durch und erstellt Draft-Report.
3. **NEU: Draft-Report darf NICHT selbst als VERIFIED markiert werden.**
   Der Status bleibt auf DRAFT, bis Schritt 4 abgeschlossen ist.
4. **NEU: Stichproben-Pflicht.** Jeder Audit-Zyklus muss mindestens
   EINEN der folgenden menschlichen Review-Schritte enthalten:
   a) Manuelle Code-Review des Reports durch einen Menschen, ODER
   b) Cross-Crate-Invarianten-Test durch einen zweiten Agenten mit
      separatem System-Prompt (anderer Kontext, kein Zugriff auf
      den Draft-Report des ersten Agenten), ODER
   c) Externes Fuzzing-/Property-Test-Ergebnis als Gate.
5. Status "VERIFIED" darf nur gesetzt werden wenn Schritt 4 belegt ist.
