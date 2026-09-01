# Implementierungsplan: GitHub Gates Optimierung & Google Jules CI Fixer Integration

> **Dokumenttyp:** Architektur & Implementierungsplan
> **Status:** Genehmigt (Approved)
> **Zielsystem:** Google Jules Cloud Agent, GitHub Actions CI/CD Pipeline (`context-gates.yml`, `rust-ci.yml`, `dag-check.yml`), `xtask` Automation
> **Erstellungsdatum:** September 2026

---

## 1. Executive Summary & Aufgabenstellung

Eine detaillierte Analyse der GitHub Commit-Historie, PR-Aktivitäten und CI-Runner-Protokolle von MemFuse Brain hat gezeigt, dass automatisierte Qualitätsschranken (Gates) regelmäßig Fehlschläge (CI Failures) verursachten. Während diese Gates essenziell für die Code-Qualität, Systemarchitektur und Governance sind, führten unpräzise Fehlermeldungen, spröde Regular Expressions (Regex), unvollständige Task-Isolierung und fehlende Handlungsanweisungen dazu, dass autonome Agents wie **Google Jules** und dessen **CI Fixer** unnötige Reparaturiterationen benötigten oder an unklaren CI-Fehlern scheiterten.

Dieser Implementierungsplan analysiert präzise die historischen Ursachen für das Versagen jedes einzelnen Gates, definiert die Integrationsmechanik des **Google Jules CI Fixer** und entwirft eine optimierte, aufgabenbezogene (task-scoped) Gate-Architektur.

---

## 2. Historische Analyse der GitHub Gate Fehlschläge

Anhand des Commit-Verlaufs (`docs/GITHUB_HISTORY.md`) und der CI-Workflows wurden die Ursachen für Gate-Fehlschläge identifiziert und kategorisiert:

### 2.1 Gate 1: Kritische & Blockierende AI-TAGs (`AI-TAG[CRITICAL|BLOCKER]`)
* **Historische Ursache:** Commits wie `bd56c88` und `68070e1` schlagfehlten, weil das Shell-Skript in `context-gates.yml` starr nach `AI-TAG[...][CRITICAL]` suchte, ohne neue Sub-Kategorien (z. B. `AI-TAG[SMELL][CRITICAL]`, `AI-TAG[SECURITY][BLOCKER]`) korrekt zu matchen oder weil Tags im Übergang nicht als `RESOLVED` markiert waren.
* **CI Fixer Auswirkung:** Jules erhielt lediglich ein generisches `exit 1` ohne Zeilennummern oder Pfadkontext, was zu Fehlraten bei der automatischen Fehlerbehebung führte.

### 2.2 Gate 2: Paniksicherheit Baseline (`.unwrap()` / `.expect()`)
* **Historische Ursache:** Refactorings und Neu-Implementierungen (z. B. Commits `fb50159`, `cf36f2b`, `354217a`) führten neue `.unwrap()` oder `.expect()` Aufrufe im Produktionscode (`crates/*/src/`) ein, wodurch `CURRENT` von `.unwrap-baseline.txt` abwich.
* **CI Fixer Auswirkung:** Wenn der Entwickler/Agent vergaß, `.unwrap-baseline.txt` zu aktualisieren oder einen expliziten Safety-Proof (`// unwrap allowed`) hinzuzufügen, schlug Gate 2 fehl. Jules fehlte oft die Anweisung, ob der `.unwrap()`-Aufruf durch Error-Handling (`?`) ersetzt oder in die Baseline aufgenommen werden sollte.

### 2.3 Gate 3: Silent I/O Fehler (`let _ = ...`)
* **Historische Ursache:** Entwickler ignorierten Rückgabewerte von Synchronisations- und Schreibaufrufen (`let _ = file.sync_all()`).
* **CI Fixer Auswirkung:** Das Skript schlug korrekterweise an, jedoch fehlte im CI-Output das präzise Muster, um dem CI Fixer direkt die betroffene Funktion und Zeile zu übermitteln.

### 2.4 Gate 5: Dokumentations-Synchronisation (`sync-docs`)
* **Historische Ursache:** Commits wie `0e3d7bf`, `9274dc6`, `b084da5` scheiterten an Gate 5, weil Quellcode-Anderung (z. B. Hinzufügen/Löschen von `AI-TAG`s) durchgeführt wurde, ohne danach `cargo xtask sync-docs` auszuführen. Dadurch entstand ein Drift zwischen Inline-Tags und `WORKING_STATE.md` / `docs/ARCHITECTURE.md`.
* **CI Fixer Auswirkung:** Jules erkannte nicht sofort, dass ein einfacher Befehl (`just sync-docs` bzw. `cargo run -p xtask -- sync-docs`) gefolgt von einem Commit den Build sofort reparieren würde.

### 2.5 Gate 7: Datum & Session-Hash Validierung (`TS:` & `SESSION:`)
* **Historische Ursache:** Mehrmalige CI-Blockaden (Commits `9be9f3a`, `a1568c5`, `f4b5611`, `bc7385a`) entstanden durch starre, hartkodierte Datums-Regexes (z. B. `TS:2026-08-(29|30|31)`). Sobald ein neues Datum erreicht wurde (z. B. `2026-09-01`), schlug Gate 7 fehl, obwohl die Tags syntaktisch korrekt waren.
* **CI Fixer Auswirkung:** Der CI Fixer versuchte fälschlicherweise, den Quellcode-Tag zu ändern, obwohl der Fehler in der fehlerhaften Regex-Logik des CI-Workflows selbst lag.

### 2.6 Gate 8: Mehrfach-Session Review Abdeckung (`check-review-coverage`)
* **Historische Ursache:** Neue `ANCHOR`-Tags für kritische Komponenten verlangten vor dem Status `DONE` mindestens zwei bzw. drei unabhängige Review-Pässe aus unterschiedlichen Session-Hashes (`REVIEW-PASS` mit `PRÜFER-KONTEXT: FRESH`). Fehlte dieser Eintrag (z. B. in Commits `781c4f3`, `45d37cd`), verweigerte Gate 8 das Mergen.
* **CI Fixer Auswirkung:** Jules konnte diesen Fehler ohne explizite Strukturhinweise schwer beheben, da nicht klar war, welcher ANCHOR unvollständig war.

### 2.7 Rust-CI (Clippy, Format, Cross-Platform) & DAG-Check
* **Historische Ursache:**
  - `cargo fmt` Prüfungen scheiterten bei fehlenden Formatierungen (`f637e3a`).
  - Feature-Matrix Isolation (`memfuse-embed --no-default-features` vs. `--all-features`) schlug fehl, wenn optionale Features unbeabsichtigt in den Core leckten.
  - DAG-Check (`dag-check.yml`) schlug fehl, wenn unzulässige Abhängigkeiten zwischen Layer-1 Crates importiert wurden (`65322dc`).

---

## 3. Google Jules & CI Fixer Integrationsstrategie

**Google Jules** ist ein autonomer, asynchroner Cloud-Agent, der Aufgaben in isolierten VMs ausführt und Pull Requests erstellt. Ein zentrales Feature von Jules ist der **CI Fixer**: Wenn ein GitHub Actions Workflow auf einem Jules-PR fehlschlägt, analysiert Jules das CI-Log und sendet automatisch korrigierende Commits.

### 3.1 Die Funktionsweise des CI Fixer
1. **Trigger:** GitHub Webhook signalisiert `check_run` / `workflow_run` Failure auf einem PR-Branch.
2. **Log Extraction:** Jules liest die stdout/stderr Streams der gescheiterten CI-Steps.
3. **Reasoning & Fix Generation:** Jules analysiert die Fehlermeldungen, lokalisiert die betroffenen Dateien/Zeilen und generiert Anpassungen.
4. **Push:** Jules committet die Behebung direkt auf den PR.

### 3.2 Design-Prinzipien für CI-Fixer-optimierte Gates

Damit der CI Fixer maximale Erfolgsquoten erzielt, müssen die GitHub Gates folgende Kriterien erfüllen:

1. **Aufgabenbezogene Präzision (Task-Scoped Gates):**
   - Ein Gate darf nur die Änderungen der jeweiligen Aufgabe bzw. der betroffenen Dateien evaluieren, anstatt die gesamte Codebase unnötig zu blockieren.
2. **Maschinenlesbare, deterministische Fehlerausgabe (Actionable Diagnostics):**
   - Jedes Gate muss bei Versagen strukturierte Blöcke nach folgendem Muster ausgeben:
     ```text
     ❌ [GATE_ID]: Kurzbeschreibung des Fehlers
     Betroffene Datei: <PATH>:<LINE>
     Ursache: <URSACHE>
     💡 AUTOMATISCHE BEHEBUNG (CI-FIXER GUIDANCE):
        Ausführen: `<BEFEHL>`
        Oder Ändern: `<KONKRETE_ANWEISUNG>`
     ```
3. **Keine spröden, zeitabhängigen Hardcodierungen:**
   - Ersetzung robuster ISO-8601 Parser anstelle von veraltenden Datums-Regexes.
4. **Fast-Feedback Loop:**
   - Schnelle Gates (z. B. Tag-Checks, Fmt) laufen zuerst, um innerhalb weniger Sekunden Feedback zu geben, bevor zeitintensive Cargo-Tests starten.

---

## 4. Ziel-Architektur: Aufgabenbezogene & Präzise GitHub Gates

### 4.1 Neugestaltung der Gate-Pipeline (`.github/workflows/context-gates.yml`)

Die Pipeline wird in modulare, präzise konfigurierte Schritte aufgeteilt, die spezifische Anweisungen für den CI Fixer enthalten:

| Gate | Name | Aufgabe / Fokus | Optimierung & CI Fixer Guidance |
|---|---|---|---|
| **Gate 1** | Unresolved Critical Tags | Scan auf offene `CRITICAL` / `BLOCKER` AI-Tags | Exakte Pfad- & Zeilenangabe; Auswurf von `AI-TAG[...][CRITICAL]` mit Hinweis, das Problem zu lösen oder auf `RESOLVED` zu setzen. |
| **Gate 2** | Panic Safety Baseline | Schutz vor neuen `.unwrap()` / `.expect()` | Nutzt `git diff` zur Erkennung nur neu hinzugefügter Unwraps. Gibt exakten Diff und den Befehl `cargo xtask update-unwrap-baseline` oder Refactoring auf `?` vor. |
| **Gate 3** | Silent I/O Prevention | Verhindert `let _ = io_op()` | Identifiziert betroffene Stellen und empfiehlt die Propagation mittels `?`. |
| **Gate 4** | MCP Protocol Isolation | Strikte Einhaltung von ADR-010 (kein `axum` in `memfuse-mcp`) | Sofortige Warnung bei unzulässiger Dependency. |
| **Gate 5** | Documentation Drift | Automatische Synchronisation von `WORKING_STATE.md` | Führt `cargo xtask sync-docs --check` aus. Bei Failure lautet die Anweisung: `cargo run -p xtask -- sync-docs` ausführen und Doku-Diff committen. |
| **Gate 6** | TODO Grammar Check | Erzwingt AI-TAG Grammatik für TODOs | Zeigt zeilengenau TODOs ohne TAG-Header. |
| **Gate 7** | Dynamic Tag & Session Enforcement | Gültigkeit von `TS:` und `SESSION:` Feldern | **Entfernung harter Datums-Regexes.** Einbindung von `cargo xtask validate-tags`, das dynamische ISO-8601 Zeitstempel-Vergleiche durchführt. |
| **Gate 8** | Multi-Session Review Coverage | Nachweis unabhängiger Reviews vor `DONE` | Ausgabe der genau fehlenden `ANCHOR`-IDs und Anleitung zum Einfügen von `REVIEW-PASS` Tags. |
| **Gate 9** | Workspace Consistency | Layer-DAG & Metadaten-Check | Prüft Crate-Zuordnungen via `cargo xtask check-consistency`. |
| **Gate 10**| Context Freshness | Jules Context Aktualitätsprüfung | Verhindert veraltete Kontext-Buffer. |

---

## 5. Schritt-für-Schritt Implementierungsplan

### Schritt 1: Refactoring von `xtask` für aufgabenbezogene Diagnosen
- Erweiterung des `xtask`-Crates um detaillierte Fehlerausgaben mit CI Fixer Hinweisen.
- Hinzufügen von `cargo xtask validate-tags` mit dynamischer ISO-8601 Validierung, um veraltende Regexes in YAML-Workflows endgültig zu eliminieren.

### Schritt 2: Überarbeitung von `.github/workflows/context-gates.yml`
- Integration der neuen `xtask`-Kommandos.
- Strukturierte Formatierung aller `echo` Fehlerausgaben in GitHub Actions Logs.

### Schritt 3: Optimierung von `rust-ci.yml` & `dag-check.yml`
- Einführung von differentieller Format- und Clippy-Prüfung für geänderte Crates.
- Exakte Ausgabe bei Feature-Matrix Fehlgeschlagenen Builds (`memfuse-embed`).

### Schritt 4: Verifizierung & Dokumentations-Update
- Ausführen von `cargo xtask sync-docs` und `cargo xtask check-consistency`.
- Verifizierung der Arbeitsfähigkeit lokal und Aktualisierung der System-Dokumentation (`WORKING_STATE.md`).

---

## 6. Zusammenfassung & Erwarteter Nutzen

Durch die Umsetzung dieses Implementierungsplans wird die CI-Pipeline von MemFuse Brain:
1. **Robust gegen Zeitdrift:** Keine CI-Fehler mehr durch Monats- oder Jahrestag-Wechsel in Regexes.
2. **Optimal für Google Jules & CI Fixer:** Deterministische, zeilengenaue Logs ermöglichen dem CI Fixer eine 100% automatische Korrektur von Routine-Fehlern (z. B. Doku-Sync, Formatierung, Tag-Einträge).
3. **Aufgabenbezogen & Präzise:** Entwickler und Agents erhalten sofortigen Kontext zum betroffenen Modul ohne unnötige globale Rauschen-Signale.
