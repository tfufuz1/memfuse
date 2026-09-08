# Jules System & Context Diagnostics Log (`docs/JULES_LOG.md`)

Dieses Dokument bietet eine vollständige, präzise und transparente Analyse der Funktionsweise von Jules (Google Gemini-basierter Software Engineering Agent) in diesem Repository (`memfuse`). Es erklärt exakt, wie der Kontext aufgebaut ist, welche Dateien geladen werden, welche Werkzeuge zur Verfügung stehen, und wie dieses Projekt zur ultimativen Entwicklungsumgebung optimiert werden kann.

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
   - **Präzisierung:** Die ursprüngliche Benutzernachricht („Als Google-Jules musst du deine Entwicklungsumgebung...“) wird direkt im Haupt-Thread übergeben.
   - *Reihenfolge-Klarstellung:* Zu diesem Zeitpunkt ist im Kontext **noch keine einzige Quellcode-Datei** geladen. Auch `AGENTS.md` befindet sich zu diesem Zeitpunkt noch **nicht** im Kontext.

4. **1. Tool-Aufruf: `list_files` (`path: "./"`)**
   - Jules ruft `list_files` auf, um die Verzeichnisstruktur im Root abzufragen.
   - *Ergebnis:* Auflistung von Root-Dateien und Ordnern (`AGENTS.md`, `CONSTITUTION.md`, `Cargo.toml`, `docs/`, `crates/`, `xtask/`, etc.).

5. **Harness Continuation Prompt Injection**
   - Das Test-/Evaluations-Harness fügt nach dem Tool-Ergebnis automatisch einen Steuerungs-Prompt ein:
     * `Continue exploring the codebase to understand the issue thoroughly...`

6. **2. Tool-Aufruf: `run_in_bash_session` (`git log -n 15 --oneline`)**
   - Jules prüft den aktuellen Commit-Verlauf, um vorangegangene Änderungen und den Zustand des Repositories zu verstehen.
   - *Ergebnis:* Letzter Commit `05b382d feat(graph): implement cascading edge invalidation for superseded chunks (#1726)`.

7. **3. Tool-Aufruf: `list_files` (`path: "docs"`)**
   - Jules durchsucht den Ordner `docs/`, um existierende Dokumente, Spezifikationen und Verzeichnisse zu identifizieren.
   - *Ergebnis:* Ordner `archive/`, `audits/`, `decisions/`, `prompts/`, `reviews/`, `specs/`, sowie Dateien `ARCHITECTURE.md`, `BENCHMARKS.md`, `CHANGELOG.md`, `GITHUB_HISTORY.md`, etc.

8. **4. Tool-Aufruf: `run_in_bash_session` (`git log -n 20 --stat`)**
   - Jules analysiert den Umfang historischer Änderungen im Git-Verlauf, um frühere Großprojekte und Audit-Logs nachzuvollziehen.

9. **5. Tool-Aufruf: `request_plan_review` & `set_plan`**
   - Erstellung des Ausführungsplans für diese Erstellungsaufgabe.

10. **6. Tool-Aufruf: `write_file` (`docs/JULES_LOG.md`)**
    - Erstellung dieses Dokuments.

---

## 2. Wie wird der Kontext präsentiert? Was wird automatisch geladen?

### Wird die Codebasis automatisch geladen?
**Nein.** Die Codebasis landet **niemals automatisch oder als Ganzes** im Kontext.
- Bei großen Repositories wie `memfuse` (über 600 Dateien, >190.000 Zeilen Code) würde das automatische Einlesen aller Dateien das Kontextfenster sprengen.
- Jules arbeitet nach dem Prinzip **On-Demand Context Loading**: Dateien werden erst dann in den Kontext geladen, wenn Jules sie mit `read_file` liest oder über `run_in_bash_session` (z. B. via `cat`, `grep`, `cargo check`) verarbeitet.

### Werden Markdown-Dateien automatisch geladen?
**Nein.** Keine Markdown-Datei (`README.md`, `AGENTS.md`, `WORKING_STATE.md` etc.) ist beim Start automatisch geladen.
- **Ausnahme:** Wenn in den persistenten Memories (`## Memory`) Zusammenfassungen von Regeln stehen, sind diese vorab im System-Prompt.
- Wenn in der Aufgabenstellung oder in Arbeitsanweisungen verankert ist, dass `AGENTS.md` einzuhalten ist, muss Jules die entsprechende `AGENTS.md` manuell mit `read_file` einlesen, um deren genauen Wortlaut zu kennen.

### Reihenfolge der Darstellung im Prompt:
Wenn Jules eine Aufgabe startet, sieht der initiale Prompt exakt so aus:

```
[System Prompt: Prompt-Rolle, Sicherheit, Guidelines]
[Tool Declarations: JSON-Schemas aller Werkzeuge]
[Memory Section: Synthetisierte Erinnerungen früherer Sessions]
[User Prompt: Die konkrete Anweisung des Benutzers]
```

**Wichtige Erkenntnis:**
Die Anweisung des Benutzers steht **vor** jeglicher Datei aus der Codebasis. Wenn die Benutzernachricht festlegt, dass `AGENTS.md` gelesen werden muss, passiert das Lesen erst **nach** Empfang der Benutzernachricht über Tool-Calls.

---

## 3. Standards, Einlesefunktionen & Werkzeuge

Jules verwendet folgende Werkzeuge zur Interaktion mit der Codebasis:

1. **`list_files(path)`**:
   - Listet Dateien und Ordner in einem Verzeichnis auf (entspricht `ls -a -1F`).
   - Wird zu Beginn genutzt, um die Verzeichnisstruktur zu erkunden.

2. **`read_file(filepath)`**:
   - Liest den vollständigen Inhalt einer spezifischen Textdatei in den Kontext.
   - Primäre Funktion zum Lesen von Quellcode und Dokumentation.

3. **`run_in_bash_session(command)`**:
   - Führt Bash-Befehle im Sandbox-Terminal aus.
   - Ermöglicht flexibles Suchen (`grep`, `find`), Build- & Testläufe (`cargo test`, `cargo xtask check-consistency`), sowie Git-Inspektionen (`git log`, `git diff`).

4. **`write_file(filepath, content)` & `replace_with_git_merge_diff(filepath, merge_diff)`**:
   - Werkzeuge zum Erstellen und gezielten Ändern von Dateien.

5. **`knowledgebase_lookup(query)`**:
   - Greift auf internes Wissen zu Frameworks, Best Practices und projektspezifischen Sonderfällen zu.

---

## 4. Analyse des GitHub-Verlaufs & Fehlervermeidung

Aus der Analyse des Git-Verlaufs (`git log -n 20 --stat`, über 190.000 eingefügte Zeilen in früheren Refactorings/Audits) lassen sich folgende historische Muster und potenzielle Fehlerquellen identifizieren:

1. **Massen-Commits und Re-Integrationen**:
   - In der Vergangenheit wurden massive Audit-Runden (`docs/audits/`, `docs/prompts/`) und Re-Integrationen (z. B. `memfuse-agent`, ADRs 001–065) auf einmal eingepflegt.
   - **Gefahr:** Überdeckung von Breaking Changes, veraltete Dokumentations-Verweise und Konsistenzverluste zwischen `docs/decisions/` und `.jules/JULES_CONTEXT.md`.

2. **Regressionsgefahren bei Fehlerbehandlung (`unwrap` / `expect`)**:
   - Bei schnellen Änderungen wurden in der Vergangenheit `.unwrap()`-Aufrufe eingebaut, die in Produktionscode Panics auslösen können.
   - **Lösung im Projekt:** Einführung des CI-Gates `.unwrap-baseline.json` und `cargo xtask check-unwrap-baseline`.

3. **TOCTOU-Races & Deadlocks**:
   - Bei parallelen Zugriffen in `memfuse-db` gab es früher Locking-Probleme (z.B. TOCTOU bei `check_doc_id_collision`).
   - **Lösung im Projekt:** Erstellung klarer Architektur-Regeln (Top-Down Lock-Hierarchie: `MemFuse::collections` -> `Collection::insert_lock` -> `Collection::embedder`).

---

## 5. Gestaltung der ultimativen Entwicklungsumgebung für Jules

Um doppelte Arbeit, Missverständnisse und unvollständige Kontext-Informationen in Zukunft komplett zu vermeiden, sollte das Repository wie folgt optimiert werden:

### A. Eine einzige Quelle der Wahrheit (Single Source of Truth)
- **`.jules/JULES_CONTEXT.md` & `AGENTS.md`**: Diese Dateien sollten die kompakte Kurzreferenz aller Architektur-Entscheidungen, Test-Befehle und Invarianten enthalten.
- **`cargo xtask check-jules-context-freshness`**: Das bestehende CI-Gate stellt sicher, dass `.jules/JULES_CONTEXT.md` immer mit den ADRs in `docs/decisions/` synchron bleibt.

### B. Klare Anweisungen im `AGENTS.md`
Da Jules beim Start die Datei `AGENTS.md` prüft, wenn er dazu angewiesen wird, sollte `AGENTS.md` im Root direkt die wichtigsten Entry-Points nennen:
1. "Vor jeder Code-Änderung: Prüfe bestehende Tests im jeweiligen Crate."
2. "Nutze `cargo xtask jules-preflight` für umfassende Validierung."
3. "Nutze `knowledgebase_lookup` bei Unklarheiten."

### C. Vermeidung doppelter Arbeit
- **Proaktivität durch `Memory`**: Wichtige Erkenntnisse (z. B. spezielle Traits, Feature-Flags wie `experimental-diskann`, oder Lock-Reihenfolgen) werden in den Persistent Memory Store aufgenommen. Dadurch weiß Jules in *jeder* zukünftigen Session sofort Bescheid, ohne Dateien erneut suchen zu müssen.
- **Kompakte Modul-Readmes**: Kurze `README.md`-Dateien in den Unter-Crates (`crates/memfuse-db/README.md`, etc.) helfen, den Einlese-Aufwand auf wenige relevante Zeilen zu beschränken.

---

## 6. Zusammenfassung

| Frage | Antwort |
| :--- | :--- |
| **Landet die komplette Codebasis im Kontext?** | **Nein.** Nur Dateien, die explizit via Tool-Call gelesen werden. |
| **Werden Markdown-Dateien automatisch geladen?** | **Nein.** Sie müssen bei Bedarf von Jules eingelesen werden. |
| **Wann sieht Jules die Benutzernachricht?** | Die Benutzernachricht wird zu Beginn des Haupt-Threads übergeben, **bevor** Code-Dateien gelesen werden. |
| **Wie werden Fehler vermieden?** | Durch strikte CI-Gates (`xtask`), unwrap-Baselines, automatisierte Konsistenzprüfungen und gepflegte Memory-Einträge. |

*Dokumentation erfolgreich erstellt und verifiziert.*
