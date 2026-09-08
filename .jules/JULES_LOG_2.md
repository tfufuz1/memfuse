# Jules Entwicklungsumgebung & System-Kontext Analyse

Dieses Dokument dient als umfassende Protokollierung und Analyse der Funktionsweise, Kontextrepräsentation, Datei-Einlesemechanismen und Qualitätsstandards von Jules (Google-Jules) im Repository **MemFuse**.

---

## 1. Chronologie des Dateieinlesens in dieser Session

Gemäß der strikten Anforderung ist hier die exakte chronologische Abfolge aller Werkzeuge und Dateizugriffe seit dem Start dieser Sitzung (von zuerst bis zuletzt) aufgeführt:

1. **Start der Session & System-Init**:
   - Laden der System-Instruktionen, Sicherheitsrichtlinien, Tool-Deklarationen und der Memory-Bank (Injected Memories aus früheren Durchläufen).
   - Empfang der Initialen Benutzernachricht.

2. **Tool-Call 1: `list_files`** (Root `./`)
   - *Zweck*: Erster Überblick über die Ordner- und Dateistruktur des Repositories im Root.

3. **Tool-Call 2: `list_files`** (Pfad `.jules`)
   - *Zweck*: Inspektion des spezialisierten Jules-Kontextordners `.jules/`.

4. **Tool-Call 3: `run_in_bash_session`** (`git log -n 15 --oneline`)
   - *Zweck*: Prüfung des aktuellen GitHub/Git-Verlaufs und der jüngsten Commits.

5. **Tool-Call 4: `read_file`** (`AGENTS.md`)
   - *Zweck*: Einlesen der primären Instruktionsdatei im Root-Verzeichnis (`AGENTS.md`), um Systemarchitektur, Crate-Topologie, Regeln und unvollständige Features zu erfassen.

6. **Tool-Call 5: `run_in_bash_session`** (`git log -n 5 --stat`)
   - *Zweck*: Tiefere Prüfung der zuletzt geänderten Dateien im Git-Repository.

7. **Tool-Call 6: `read_file`** (`.jules/SESSION_BOOTSTRAP.md`)
   - *Zweck*: Einlesen der maschinenausführbaren Bootstrap-Checkliste für Jules-Sitzungen.

8. **Tool-Call 7: `read_file`** (`.jules/JULES_CONTEXT.md`)
   - *Zweck*: Einlesen des permanenten Ambient Context Snapshots.

9. **Tool-Call 8: `list_files`** (Pfad `docs`)
   - *Zweck*: Inspektion der Dokumentationsstruktur im Ordner `docs/` zur Platzierung von `JULES_LOG.md`.

---

## 2. Funktionsweise des Systems & Kontext-Präsentation

### Wie wird Jules der Kontext präsentiert?
Der Kontext für das Modell besteht aus mehreren klar getrennten Abschnitten:
1. **System Prompt / Directives**:
   - Allgemeine Verhaltensregeln, Sicherheitsrichtlinien (Safety Boundaries), Tool-Definitionen, Planungs-Regeln (`set_plan`, `request_plan_review`) und Richtlinien für Git Merge Diffs.
2. **Memory Block**:
   - Automatisch injizierte Erkenntnisse und Invarianten aus früheren Iterationen (z. B. Lock-Hierarchien, `TxId`-Regeln, `unwrap`-Baseline, Crate-Layering).
3. **User Input / Prompt**:
   - Die explizite Benutzernachricht. Sie wird dem Modell als aktueller User-Turn übergeben. In der Benutzeroberfläche erscheint die Nachricht als Startpunkt der Unterhaltung.
4. **Tool Execution Outputs**:
   - Die Rückmeldungen der aufgerufenen Werkzeuge (z. B. Dateiinhalte von `read_file`, Bash-Ausgaben von `run_in_bash_session`).

### Wird die gesamte Codebasis in den Kontext geladen?
- **Nein, keineswegs!** Die Codebasis wird **nicht** automatisch beim Start als Ganzes in den Modellkontext geladen.
- Das Modell besitzt zu Beginn lediglich die Grundstruktur und die im System-Prompt bzw. Memory hinterlegten Informationen.
- Alle weiteren Dateien werden **dynamisch und selektiv** über Tool-Aufrufe (`read_file`, `list_files`, `run_in_bash_session`) in den Kontext geladen.

### Gibt es eine feste Reihenfolge beim Einlesen von Dateien?
- Es gibt keine starre technische Reihenfolge der Umgebung, sondern eine **methodische Standard-Reihenfolge**:
  1. **Root-Level Exploration**: `list_files` im Root, Lesen von `README.md` oder `AGENTS.md`.
  2. **Session Bootstrap & Context**: Einlesen von `.jules/SESSION_BOOTSTRAP.md` und `.jules/JULES_CONTEXT.md` (falls vorhanden).
  3. **Aufgabenspezifische Sub-Crates**: Wenn Code in `crates/memfuse-db` bearbeitet wird, liest Jules die dateinahe `crates/memfuse-db/AGENTS.md`.
  4. **Quellcodedateien**: Punktuelles Einlesen der betroffenen `.rs`-Dateien vor der Bearbeitung.

### Wird `.jules/SESSION_BOOTSTRAP.md` automatisch geladen?
- **Nein!** Das System-Framework lädt `.jules/SESSION_BOOTSTRAP.md` **nicht automatisch** unsichtbar im Hintergrund in den Workspace-Kontext.
- Es ist jedoch in den Richtlinien (`AGENTS.md` / Memory) festgelegt, dass Jules zu Beginn der Arbeit diese Datei oder `JULES_CONTEXT.md` selbstständig per Tool-Call einlesen soll, um die Arbeitsumgebung zu initialisieren.

### Welche Markdown-Dateien liest Jules typischerweise?
1. `AGENTS.md` (Root und Crate-spezifisch in `crates/*/AGENTS.md`): Höchste Priorität für Entwicklungsstandards.
2. `README.md` & `DEVELOPERS.md`: Projektüberblick und Setup-Anleitungen.
3. `.jules/SESSION_BOOTSTRAP.md` & `.jules/JULES_CONTEXT.md`: Session-Protokolle und Umgebungszustand.
4. `WORKING_STATE.md`: Autogenerierter, tagesaktueller Projektstatus.
5. `CONSTITUTION.md` & `VETOES.md`: Verfassungsregeln und abgelehnte/eingeschränkte Features.
6. `docs/decisions/ADR-*.md`: Architectural Decision Records bei Architektur-Änderungen.

---

## 3. Analyse des GitHub-Verlaufs & Lerneffekte zur Fehlervermeidung

Aus der Analyse der bisherigen Commits, Audits und PRs lassen sich folgende historische Fehlerquellen und deren Vermeidung identifizieren:

1. **Unsynchronisierte Kontext-Dateien (`JULES_CONTEXT.md` vs. `WORKING_STATE.md`)**:
   - *Problem*: Statische Dokumente veralten schnell, wenn Commits ohne Doku-Update gemerged werden.
   - *Lösung*: Einführung des CI Gate 10 (`cargo run -p xtask -- check-jules-context-freshness`) und automatische Generierung via `just sync-docs`.
2. **Panic-Risiken durch `.unwrap()` / `.expect()`**:
   - *Problem*: In Production-Code führten `.unwrap()` Aufrufe zu FFI- oder Thread-Panics.
   - *Lösung*: Die `.unwrap-baseline.json` Überwachung stellt sicher, dass kein neuer Production-Code `.unwrap()` verwendet. Fehler werden strikt über `MemFuseError` und `?` propagiert.
3. **Kollisionen bei ADR-Nummern durch parallele Agenten-Sessions**:
   - *Problem*: Mehrmaliges Vergeben derselben ADR-Nummer bei paralleler Bearbeitung.
   - *Lösung*: Dynamisches Ermitteln der höchsten vergebenen ADR-Nummer via `ls docs/decisions/ | grep -oP '(?<=ADR-)\d+' | sort -n | tail -1` vor Neuerstellung.
4. **Schichtenverletzungen im Crate-DAG (Layer 0–6)**:
   - *Problem*: Niedrige Layer (z. B. `memfuse-core`) importierten versehentlich Typen aus höheren Layern.
   - *Lösung*: Automatischer DAG-Check via `just dag-check` und `check_type_registry`.

---

## 4. Gestaltungsrichtlinien für die ultimative Jules-Entwicklungsumgebung

Um künftig doppelte Arbeiten, Prompt-Thrashing und unnötige Kontext-Aufblähung zu vermeiden, sollte die Arbeitsumgebung wie folgt strukturiert sein:

### A. Single Source of Truth (SSOT) & Redundanzvermeidung
- **Modulare AGENTS.md**: Statt einer riesigen monolitischen Datei sollten Crate-spezifische Regeln ausschließlich in den jeweiligen Subdirectories liegen (z. B. `crates/memfuse-db/AGENTS.md`). Jules liest nur die `AGENTS.md`, die für die aktuelle Aufgabe relevant ist.
- **Autogenerierte Zustandsdateien**: `WORKING_STATE.md` und `CHANGELOG.md` sollten immer per `just sync-docs` generiert werden, um manuelle Doku-Abweichungen zu verhindern.

### B. Klare, maschinenlesbare CI-Gates & Commands
- **Xtask-Harness**: Bündelung aller Prüfungen in `cargo xtask` (z. B. `jules-preflight`, `check-duplicate-symbols`, `check-vetoes`).
- **Justfile als Einstieg**: Alle wiederkehrenden Befehle (`just check`, `just test`, `just dag-check`, `just sync-docs`) bieten eine einheitliche Schnittstelle, die Jules über `run_in_bash_session` ausführen kann.

### C. Standardisiertes In-File Tagging
- **Verwendung von `AI-TAG` und `ANCHOR`**:
  - `AI-TAG[CRITICAL](TS: ISO-8601)(SESSION: hash)` für kritische Bedenken.
  - Dadurch kann Jules mit einem gezielten `grep` in Sekunden alle offenen Problemstellen finden, ohne hunderte Dateien einlesen zu müssen.

### D. Optimale Interaktions-Pipeline
- **Erst Exploration & Plan, dann Mutation**: Vor jeder Code-Änderung prüft Jules Signaturen (`grep -n "pub fn ..."`), liest relevante `AGENTS.md`, fordert ein Plan-Review an (`request_plan_review`), setzt den Plan (`set_plan`) und führt erst danach Modifikationen durch.
- **Verifikation nach jeder Modifikation**: Nach jedem Schreiben wird die Datei erneut eingelesen oder getestet, um festzustellen, dass keine Artefakte oder Syntaxfehler entstanden sind.

---

*Dokument erstellt von Jules im Rahmen der Umgebungsevaluierung.*
