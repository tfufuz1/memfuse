# Jules Dashboard Setup & Progressive Activation Guide

> **WICHTIG:** Das Event-Driven "Conveyor Belt" Architekturkonzept ist operativ nicht umsetzbar. Google Jules Scheduled Tasks müssen zwingend **manuell** im Jules Dashboard ([jules.google.com](https://jules.google.com)) eingerichtet werden. Es gibt keine programmatische API dafür.

Die Orchestrierung erfolgt jetzt über ein **manuelles Scheduled-Task-System**, in dem jeder Agent einmal am Tag aktiv wird, seine `READY` Anchors bearbeitet und die `SUCCESSOR` Tags als "Warteschlangen-Element" für den nächsten Agenten setzt, der dann bei seinem nächsten manuellen Schedule-Run die Aufgabe aufnimmt.

---

## 1. Wie man einen Scheduled Task im Jules Dashboard einrichtet

Für jeden der 14 Accounts musst du dich in den entsprechenden Google Account einloggen, Jules aufrufen, und folgende Schritte durchführen:

1. Gehe in das Haupt-Input-Feld in Jules.
2. Klicke unten rechts auf das "Planning" Dropdown-Menü.
3. Wähle **"Scheduled Task"**.
4. Setze die **Frequenz** laut der Tabelle unten (meistens "Daily" zu einer bestimmten Uhrzeit, oder wöchentlich).
5. **Prompt-Erstellung:** Dein Prompt besteht IMMER aus zwei Teilen:
   - Die komplette `00-PREAMBLE.md` (befindet sich in `.agent/jules/prompts/`)
   - Gefolgt von dem accountspezifischen Prompt (z.B. `.agent/jules/prompts/accounts/01-core-guardian.md`)
6. Kopiere diesen fusionierten Textblock in das Eingabefeld und klicke auf **Submit**.

_Hinweis: Man kann fertige Scheduled Tasks nach der Erstellung pausieren/löschen. Die Dashboard Tab "Scheduled" (unter dem Textfeld) listet anstehende Jobs._

---

## 2. Progressive Activation Strategy (Rollout Plan)

Um das Free Tier Kontingent zu schonen (max. 15 Tasks/Tag pro Account) und Deadlocks zu vermeiden, aktiviere nicht alle 14 Accounts auf einmal. Aktiviere sie progressiv pro Woche:

### 🟢 Woche 1: Core Stabilität & Maintenance
*Die fundamentalen Agenten, um das System stabil zu halten und technische Schulden abzubauen.*
- **Account 13 (Debt Hunter):** Daily, 05:00 UTC
- **Account 00 (Watchdog):** Daily, 05:30 UTC
- **Account 01 (Core Guardian):** Daily, 06:00 UTC
- **Account 02 (Store Engineer):** Daily, 07:00 UTC
- **Account 07 (QA Cross-Crate):** Daily, 20:00 UTC

### 🟡 Woche 2: Search Engine & Database
*Nachdem der Core steht, werden die Crate-Schichten `store`, `index` und `db` ausgebaut.*
- **Account 03 (Index Engineer):** Daily, 08:00 UTC
- **Account 04 (DB Orchestrator):** Daily, 09:00 UTC
- **Account 10 (Security):** Daily, 12:00 UTC

### 🟠 Woche 3: Features & Integration
*Erweiterte Funktionalitäten und Belastungstests starten hier.*
- **Account 05 (Text Engine):** Daily, 10:00 UTC
- **Account 12 (Integration Tester):** Daily, 21:00 UTC

### 🔵 Woche 4: Peripherie & API
*Python-Bindings, Benchmarks, und Code-Dokumentation komplettieren das System.*
- **Account 06 (Python Bindings):** Daily, 11:00 UTC
- **Account 08 (Docs & Specs):** Weekly (Montags), 08:00 UTC
- **Account 09 (Benchmarks):** Weekly (Freitags), 22:00 UTC
- **Account 11 (CI/DevOps):** Weekly (Mittwochs), 10:00 UTC

---

## 3. Trouble-Shooting im Dashboard

- **Agent blockiert?** Falls CI-Tests fehlschlagen, repariert **Account 13 (Debt Hunter)** das in seinem nächsten morgendlichen Zyklus automatisch, indem er den Code säubert (z.B. verbotene `.unwrap()` Aufrufe korrigiert).
- **Scheduled Limits:** Achte darauf, nicht mehr als 1-2 Runs pro Tag pro Agent anzusetzen, da du pro Google-Account ein Limit hast.
- **Workflow-Fehler in Actions:** Das `auto-merge-jules.yml` CI-Skript wurde so repariert, das `unwrap` in Tests erlaubt ist. Agenten können gefahrlos Tests schreiben.
