# Branch-Protection-Verifikations-Checkliste

> **DISCLAIMER / HINWEIS:**
> Diese Checkliste wurde durch statische Analyse der Workflow-Dateien unter `.github/workflows/` erstellt. Der tatsächliche Konfigurationsstand der Branch-Protection-Regeln in den Repository-Settings wurde NICHT geprüft (kein API-Zugriff/fehlende Admin-Rechte) und muss von einem Repository-Admin manuell abgeglichen werden.

---

## 1. Übersicht der GitHub Actions Workflow Jobs

Diese Tabelle listet **alle 24 Jobs** aus den 9 GitHub Actions Workflows des Repositories auf. GitHub Required Status Checks benötigen den **exakten Job-Namen** (Wert des `name:`-Feldes in der Quell-YAML oder, falls `name:` nicht explizit gesetzt ist, den `job-id`-Schlüssel).

| Job-Name (exakt) | Quelldatei | Trigger | Empfehlung (Required Y/N) | Begründung |
| :--- | :--- | :--- | :---: | :--- |
| `Compile Check (cargo check)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Stellt sicher, dass das gesamte Workspace kompiliert (`check-compile`), bevor Änderungen gemergt werden. |
| `Format Check` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Erzwingt den `rustfmt`-Standard für den gesamten Rust-Code im Repository. |
| `Clippy (-D warnings)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Verhindert Linter-Warnungen und verstärkende Fehler im Haupt-Workspace. |
| `Feature Matrix Checks` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Prüft Crate-Feature-Kombinationen (`--no-default-features`, `--all-features`, `--features reranking`). |
| `PyPI Wheel Build Check (Maturin Dry-Run)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Validiert vorab, dass `memfuse-py` via Maturin fehlerfrei gebaut werden kann. |
| `Test Suite (cargo-nextest with retries)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Führt Unit- und Integrationstests des Haupt-Workspace sowie Doc-Tests aus. |
| `Clippy (memfuse-py)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Prüft Python-Binding-Code (`memfuse-py`) auf Clippy-Warnungen. |
| `Test Suite (memfuse-py)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Führt die Rust-Tests für das Python-Binding (`memfuse-py`) aus. |
| `Clippy (memfuse-tauri --lib)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Prüft die `memfuse-tauri` Bibliothek auf Clippy-Warnungen. |
| `Test Suite (memfuse-tauri --lib)` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Führt die Unit- und Integrationstests für `memfuse-tauri` aus. |
| `Cross-Platform Core Tests` | `.github/workflows/rust-ci.yml` | `push`, `pull_request` | **Y** | Prüft Kern-Crates auf Windows und macOS auf Plattform-Kompatibilität. |
| `context-gates` | `.github/workflows/context-gates.yml` | `push`, `pull_request` | **Y** | Kanonisches Gate für Governance, Unwraps, DAG, Tag-Grammatik, Review-Coverage und Commit-Qualität. |
| `Fixture-Smoke-Test & Harness Dry-Run` | `.github/workflows/bench.yml` | `push`, `pull_request`, `workflow_dispatch` | **Y** | Schnelltests für Benchmark-Harness & Smoke-Tests bei jedem PR. |
| `Retrieval Accuracy Benchmark (memfuse-bench)` | `.github/workflows/bench.yml` | `push`, `pull_request`, `workflow_dispatch` | **Y** | Misst Retrieval-Genauigkeit und stützt die Regressions-Verifikation bei PRs. |
| `Retrieval Quality Regression Gate (LongMemEval & LoCoMo)` | `.github/workflows/bench.yml` | `push`, `pull_request`, `workflow_dispatch` | **Y** | Prüft auf Retrieval-Qualitäts-Regressionen gegenüber der Baseline. |
| `chaos-matrix` | `.github/workflows/chaos.yml` | `schedule` (`0 2 * * *`), `workflow_dispatch` | **N** | Nightly-Fehler-Injektionstest; läuft nicht bei PRs und würde PRs dauerhaft blockieren. |
| `prepare-audit-context` | `.github/workflows/scheduled-audit.yml` | `schedule` (`0 22 * * 5`), `workflow_dispatch` | **N** | Wöchentlicher Audit-Issue-Ersteller; läuft nicht bei PRs. |
| `trigger-mutation-testing` | `.github/workflows/scheduled-audit.yml` | `schedule` (`0 22 * * 5`), `workflow_dispatch` | **N** | Wöchentlicher Auslöser für Mutation-Testing; läuft nicht bei PRs. |
| `build` | `.github/workflows/tauri-release.yml` | `push` (tags `v*`) | **N** | Release-Build-Workflow für Tagged Releases; läuft nicht bei PRs. |
| `Build Wheels (${{ matrix.os }} - ${{ matrix.target \|\| 'default' }})` | `.github/workflows/publish-pypi.yml` | `push` (tags `memfuse-py-v*`), `workflow_dispatch` | **N** | PyPI-Release Wheel Builder; läuft nur bei Release-Tags oder manuell. |
| `Publish Wheels to PyPI` | `.github/workflows/publish-pypi.yml` | `push` (tags `memfuse-py-v*`), `workflow_dispatch` | **N** | PyPI-Upload-Job; läuft nur bei Release-Tags oder manuell. |
| `measure-and-record-recall` | `.github/workflows/nucleation-recall-history.yml` | `schedule` (`0 3 * * *`), `workflow_dispatch` | **N** | Täglicher Recall-Messextraktions-Job; läuft nicht im PR-Kontext. |
| `prune` | `.github/workflows/prune-branches.yml` | `schedule` (`0 2 * * *`), `workflow_dispatch` | **N** | Remote-Branch-Bereinigungsjob; ist explizit für PRs deaktiviert (`if: github.event_name != 'pull_request'`). |
| `overlap-report` | `.github/workflows/prune-branches.yml` | `schedule`, `workflow_dispatch`, `pull_request` | **N** | Advisory-Only Overlap-Bericht (`continue-on-error: true`); soll Merges nicht blockieren. |

---

## 2. Manuelle Verifikationsschritte für einen Repository-Admin

Um sicherzustellen, dass keine PRs gemergt werden, bei denen kritische Quality Gates fehlschlagen, muss ein Repository-Admin mit entsprechenden Rechten die folgenden Einstellungen manuell auf GitHub vornehmen:

1. Navigation im Webbrowser:
   `Settings` → `Branches` → `Branch protection rules` → Regel für `main` bearbeiten (oder neu anlegen).
2. Option aktivieren: **"Require status checks to pass before merging"**.
3. Option aktivieren: **"Require branches to be up to date before merging"**.
4. In der Suchleiste unter *"Status checks that are required"* jeden der oben mit **Empfehlung (Required Y)** markierten Job-Namen suchen und hinzufügen:
   - `Compile Check (cargo check)`
   - `Format Check`
   - `Clippy (-D warnings)`
   - `Feature Matrix Checks`
   - `PyPI Wheel Build Check (Maturin Dry-Run)`
   - `Test Suite (cargo-nextest with retries)`
   - `Clippy (memfuse-py)`
   - `Test Suite (memfuse-py)`
   - `Clippy (memfuse-tauri --lib)`
   - `Test Suite (memfuse-tauri --lib)`
   - `Cross-Platform Core Tests`
   - `context-gates`
   - `Fixture-Smoke-Test & Harness Dry-Run`
   - `Retrieval Accuracy Benchmark (memfuse-bench)`
   - `Retrieval Quality Regression Gate (LongMemEval & LoCoMo)`
5. Speichern durch Klicken auf **"Save changes"**.

---

## 3. Bekannter historischer Vorfall

**Präzedenzfall (2026-08-24 bis 2026-09-11):**
Über einen Zeitraum von 18 Tagen schlug das `check-compile`-Gate in der CI durchgehend fehl, ohne dass Merges auf den `main`-Branch blockiert wurden. Grund dafür war, dass das Gate zwar in den Workflow-Dateien definiert war, aber in den GitHub-Repository-Einstellungen nicht als "Required Status Check" registriert war (oder Admin-Overrides erlaubt waren). Dieser Vorfall (APM-CI-9: Unverified Merge Gate) zeigt eindrücklich, dass das bloße Vorhandensein von CI-Jobs ohne explizite Branch Protection den Haupt-Branch nicht vor fehlerhaftem Code schützt.
