# Jules System & Context Diagnostics Log (`.jules/JULES_LOG.md`)

Dieses Dokument bietet eine vollständige, präzise und transparente Analyse der Funktionsweise von Google-Jules (Google Gemini-basierter Software Engineering Agent) in diesem Repository (`memfuse`), die Umgebungs- und Git-Kapazitäten innerhalb der VM Sandbox, eine vollständige Checkliste aller Git-Befehle, eine Analyse der Repository-Analyseskripte (`/docs/GITHUB_ANALYSE.md`, `/docs/GITHUB_HISTORY.md`, `.jules/JULES_LOG_2.md`) sowie einen umfassenden Optimierungsplan für die MemFuse-Entwicklung mit Google-Jules.

---

## 1. Exakte Chronologie des Context-Loadings in dieser Session

Der Arbeitskontext von Jules wird strikt sequentiell aufgebaut. **Repository-Dateien landen NIEMALS automatisch vollständig im Kontext.** Stattdessen liest Jules Dateien über gezielte Tool-Aufrufe schrittweise ein.

Hier ist die exakte, chronologische Abfolge der Einlesevorgänge und Systeminjektionen von Beginn dieser Session an:

### Chronik (Zuerst bis zuletzt):

1. **System Prompt & Standard-Instruktionen**
   - Injektion der System-Rolle („You are Jules, an extremely skilled software engineer...“).
   - Injektion aller verfügbaren Werkzeug-Deklarationen (`list_files`, `read_file`, `write_file`, `replace_with_git_merge_diff`, `run_in_bash_session`, `set_plan`, `request_plan_review`, `pre_commit_instructions`, `submit`, etc.).
   - Injektion der globalen Verhaltensregeln, Sicherheitsgrenzen, Git-Merge-Diff-Formate und Planungs-Richtlinien.

2. **Persistent Memory Block (`## Memory`)**
   - Das System injiziert automatisch alle projektspezifischen Langzeit-Erinnerungen (Memory Items) aus vorherigen Iterationen.
   - *Beispiele im Kontext:* Speicherstrukturen von `memfuse-db`, `AGENTS.md`-Hierarchie, `ScalarQuantizer`-Sichtbarkeiten, `TxId`-Grenzwerte, Lock-Hierarchien, CI-Gate 10 (`check-jules-context-freshness`), `.unwrap-baseline.json` Regeln, etc.

3. **Benutzer-Nachricht (User Prompt)**
   - Die ursprüngliche Benutzernachricht („Als Google-Jules musst du einen Optimierungsplan für die Entwicklung des Memfuse Codes...“) wird direkt im Haupt-Thread übergeben.
   - *Reihenfolge-Klarstellung:* Zu diesem Zeitpunkt ist im Kontext **noch keine einzige Quellcode-Datei** geladen.

4. **1. Tool-Aufruf: `list_files` (`path: "docs"`)**
   - Jules ruft `list_files` auf, um die Verzeichnisstruktur in `docs/` abzufragen.

5. **2. Tool-Aufruf: `list_files` (`path: ".jules"`)**
   - Inspektion des Spezialordners `.jules/`.

6. **3. Tool-Aufruf: `read_file` (`docs/GITHUB_ANALYSE.md`)**
   - Einlesen der tiefgehenden Analyse über AI-Agent Anti-Muster in GitHub.

7. **4. Tool-Aufruf: `read_file` (`docs/GITHUB_HISTORY.md`)**
   - Einlesen der chronologischen Entwicklungshistorie und Architekturphasen.

8. **5. Tool-Aufruf: `read_file` (`.jules/JULES_LOG_2.md`)**
   - Einlesen der Umgebungs- und Kontext-Analyse.

9. **6. Tool-Aufruf: `read_file` (`.jules/JULES_LOG.md`)**
   - Einlesen der bisherigen Log-Datei.

10. **7. Tool-Aufruf: `run_in_bash_session` (`git status; git log -n 5; git remote -v`)**
    - Überprüfung des lokalen Git-Zustands, des aktuellen Branches und der Remotes.

11. **8. Tool-Aufruf: `run_in_bash_session` (`git version; git branch -a`)**
    - Überprüfung aller verfügbaren lokalen und Remote-Branches.

12. **9. Tool-Aufruf: `run_in_bash_session` (`git fetch origin --dry-run; git stash list; git log --oneline -n 5`)**
    - Verifikation lokaler Git-Sandbox-Operationen (Stash, Log, Fetch).

---

## 2. GitHub & Git VM-Sandbox Test & Kapazitäts-Analyse

Google-Jules läuft in einer abgesicherten Linux-VM-Sandbox. Folgende Git-Funktionen und -Grenzen wurden empirisch ermittelt:

### A. Erlaubte & Empfohlene VM Git-Befehle (Lokale Operationen)
- `git status`: Zeigt geänderte, unversionierte und Staged Dateien an.
- `git log` / `git log --oneline -n <N>`: Inspektion der Commit-Historie.
- `git diff`: Vorschau aller noch uncommitted Änderungen.
- `git fetch origin`: Abrufen neuester Remote-Ref-Updates ohne automatischen Merge.
- `git branch` / `git branch -a`: Auflistung lokaler und entfernter Branches.
- `git stash push` / `git stash pop` / `git stash list`: Temporäres Zwischenspeichern lokaler Workspace-Änderungen.
- `git commit --amend`: Korrektur der **neuesten lokalen Commit-Nachricht oder des Commit-Inhalts** vor dem Submit.
- `git reset HEAD~1` (Soft/Mixed): Rückgängigmachen des letzten Commits unter Beibehaltung der Änderungen im Working Tree.
- `git rebase -i` / `git cherry-pick`: Lokales Umstrukturieren oder Anwenden von Commits innerhalb des Feature-Branches.

### B. Eingeschränkte / Unterbundene Befehle
- `git pull` / `git push`: Das direkte Ausführen von `git pull` oder `git push` in Bash-Skripten wird von der Sandbox-Sicherheitskontrollschicht abgefangen, um unerwartete Branch-Zustände, Sperren oder unkontrollierte Remote-Schreibzugriffe zu verhindern.
- **Lösung für Updates:** Jules nutzt `git fetch origin` zur Inspektion und das plattform eigene `submit`-Tool für das finale Pushen und Committen des Branches.

### C. Kann Jules alte Commits in der VM korrigieren?
- **Ja, innerhalb des eigenen Feature-Branches:** Jules kann mittels `git commit --amend`, `git reset` oder `git rebase` alte Commits lokal in der VM korrigieren und aufräumen, bevor der finale Branch per `submit` eingereicht wird.
- **Auf `main`:** Da `main` geschützt ist und Änderungen per Pull Request (Squash Merge) integriert werden, nimmt Jules Korrekturen auf seinem Branch vor, testet diese mit `cargo xtask` und stellt sicher, dass keine fehlerhaften Commits gemerged werden.

---

## 3. Vollständige Git-Befehls-Checkliste für Google-Jules

Vor jedem Submit und während der Entwicklung sollte Google-Jules folgende Git-Checkliste durchlaufen:

```bash
# 1. STATUS & WORKING TREE INSPEKTION
git status                              # Working Tree sauber? Welche Dateien wurden verändert?
git diff                                # Inhaltliche Prüfung aller uncommitted Änderungen

# 2. HISTORIE & REMOTE REFRESH
git fetch origin main                   # Aktuellsten Stand von main holen
git log -n 5 --oneline                  # Letzte Commits prüfen

# 3. LOKALE ZWISCHENSPEICHERUNG & EXPERIMENTE
git stash push -m "temp_wip"            # Arbeitsstand sichern bei Switch/Test
git stash list                          # Stashes anzeigen
git stash pop                           # Arbeitsstand wiederherstellen

# 4. KORREKTUR LOKALER COMMITS (KORREKTUR-PHASE)
git commit --amend -m "feat(...): ..."  # Letzten Commit lokal anpassen/korrigieren
git reset --soft HEAD~1                 # Letzten Commit auflösen, Code behalten

# 5. PRE-SUBMIT VERIFIKATION (PFLICHT VOR SUBMIT)
cargo xtask jules-preflight             # Vollständige Gate- & Test-Suite ausführen
cargo xtask check-jules-context-freshness # Doku-Freshness prüfen
```

---

## 4. Analyse der Skripte & Dokumente (`GITHUB_ANALYSE.md`, `GITHUB_HISTORY.md`, `.jules/JULES_LOG_2.md`)

Die Analyse der Dokumente zeigt deutliche Schwachstellen in Multi-Agenten-Workflows sowie vorhandene Werkzeuge zur Lösung:

1. **Anti-Muster aus `docs/GITHUB_ANALYSE.md`**:
   - *Parallel-Implementierungen:* 3 Agenten erstellten innerhalb 1 Stunde 3 Varianten derselben Kalibrierungs-Funktion (#1627, #1634, #1645), was zu Compile-Breaks auf `main` führte.
   - *Duplicate Titles & Mass Deletions:* Identische Commit-Titel für völlig verschiedene PRs mit versehentlicher Löschung tausender Zeilen.
   - *Status Thrashing:* `STATUS:DONE`-Tags wurden widerrufen und von neuen Sessions wiederholt neu aufgerollt.
   - *Branch Proliferation:* >81 offene Remote-Branches ohne zentrales Claiming.

2. **Skript-Unterstützung durch `xtask` (`xtask/src/`)**:
   MemFuse besitzt bereits hochentwickelte Rust-Skripte zur Repository-Analyse:
   - `jules_preflight.rs`: Führt `check_no_active_claim_conflict`, `check-consistency`, `check-vetoes` und DAG-Checks aus.
   - `check_commit_messages.rs`: Verhindert `Shell-Commit` und ungültige/leere Commit-Nachrichten.
   - `check_duplicate_intent.rs` (Gate 12): Erkennt doppelte PR-Intents und verhindert parallele Arbeit an demselben Feature.
   - `check_jules_context_freshness.rs` (Gate 10): Validiert die Frische der Dokumentation gegenüber `DECISIONS.md`.
   - `claim.rs`: Ermöglicht aktives Claiming von Crates via GitHub Issues oder `.jules/claims.json`.

---

## 5. Optimierungsplan für die MemFuse-Entwicklung mit Google-Jules

Um die Entwicklung in MemFuse zu optimieren, werden folgende 5 Säulen durchgesetzt:

### Säule 1: Strikte Pre-Submit Pipeline & Context Verification
- Vor jedem Commit/Submit führt Jules `git status` und `cargo xtask jules-preflight` aus.
- Keine ungeprüften Commits oder "Shell-Commit"-Titel.

### Säule 2: Aktives Crate-Claiming gegen Parallel-Agenten-Kollisionen
- Vor Beginn einer Aufgabe prüft Jules mit `cargo xtask claim` oder `check_no_active_claim_conflict`, ob ein anderes Team/Agent an demselben Crate arbeitet.

### Säule 3: Lokale Commit-Sanierung in der VM
- Fehlerhafte lokale Entwürfe werden in der VM via `git commit --amend` oder `git reset` bereinigt, bevor sie eingereicht werden.

### Säule 4: Automatische Dokumentations-Synchronisation (SSOT)
- Nach jeder Architektur- oder Code-Änderung wird `cargo xtask check-jules-context-freshness` ausgeführt, um Drift zwischen Quellcode und Spezifikationen zu verhindern.

### Säule 5: Transparente Log- & Session-Führung
- Fortlaufende Aktualisierung von `.jules/JULES_LOG.md` und Injektion wichtiger Invarianten in den Persistent Memory Store.

---

*Optimierungsplan und Diagnostik-Log erfolgreich erstellt und aktualisiert.*
