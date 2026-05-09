# MemFuse — Multi-Account Entwicklungssystem
## Free-Tier Orchestrierung · Auto-Merge Pipeline · Agent-Koordination
---

> **Zweck:** Vollständige Betriebsanleitung für das 13-Account-Jules-System  
> **Stand:** Mai 2026  
> **Kritischer Befund:** Jules-PRs müssen manuell gemerged werden — das ist lösbar

---

## 0. Ressourcen-Inventar & Realität

### Was du wirklich hast

| Ressource | Anzahl | Limit pro Einheit | Gesamt |
|-----------|--------|------------------|--------|
| Jules Accounts (Free) | 13 | 15 Tasks/Tag + 3 Concurrent | **195 Tasks/Tag** |
| Jules Scheduled Tasks | 13 Accounts | 15 Schedules/Account | **195 Slots** |
| Gemini-CLI (Browser) | 13 | Manuell, Free Tier | Supervision |
| Gemini-CLI (API Key) | 13 | AI Studio Free Tier | Automatisierung |
| Antigravity (Browser) | 13 | Manuell | Elite-Architektur |
| GitHub Account | 1 | Unbegrenzte Actions-Minuten (public repo) | CI/CD |

### Kritische Wahrheit über Jules Free Tier

Der Free Plan bietet 15 individuelle tägliche Tasks und 3 gleichzeitige Tasks. Das rollierende 24h-Fenster bedeutet: Tasks die um 14:00 Uhr gestartet werden, sind um 14:00 Uhr des nächsten Tages wieder verfügbar — nicht um Mitternacht.

Der `jules-action` ermöglicht es, Jules aus jedem GitHub-Event zu triggern: Issues, Pull Requests, Schedules oder Workflow-Dispatches.

**Das PR-Problem:** Code-Änderungen kommen als PRs in dein Repo — deine eigenen CI/CD-Policies bestimmen was gemerged wird. Jules merged **nie** selbst. Das ist die zentrale Herausforderung.

---

## 1. Das Auto-Merge System (Kern-Lösung)

### Wie Jules PRs erstellt

Jules veröffentlicht den Feature-Branch im originalen Repository und öffnet automatisch einen Pull Request gegen den Main-Branch für das finale Merging. Alle Jules-Branches von allen 13 Accounts landen im selben GitHub-Repo als PRs vom selben GitHub-User.

### GitHub Repo Einstellungen (einmalig, 5 Minuten)

```
GitHub → Dein Repo → Settings → General → Pull Requests
  ✅ Allow auto-merge  ← AKTIVIEREN
  ✅ Automatically delete head branches  ← AKTIVIEREN

GitHub → Settings → Actions → General
  ✅ Allow GitHub Actions to create and approve pull requests  ← AKTIVIEREN
```

### Workflow 1: Jules PR Auto-Merge (`auto-merge-jules.yml`)

```yaml
# .github/workflows/auto-merge-jules.yml
name: Auto-Merge Jules PRs

on:
  pull_request:
    types: [opened, synchronize, reopened]

permissions:
  contents: write
  pull-requests: write

jobs:
  # ── Gate 1: Erkennung ob dieser PR von Jules kommt ──────────────────────
  detect-jules:
    runs-on: ubuntu-latest
    outputs:
      is_jules: ${{ steps.check.outputs.is_jules }}
    steps:
      - name: Jules-PR erkennen
        id: check
        run: |
          BRANCH="${{ github.head_ref }}"
          TITLE="${{ github.event.pull_request.title }}"
          # Jules-Branches beginnen typischerweise mit "jules/" oder enthalten "jules"
          # Zusätzlich: PR-Titel von jules-action folgen Mustern aus AGENTS.md
          if [[ "$BRANCH" == jules/* ]] || \
             echo "$TITLE" | grep -qiE "^(feat|fix|chore|refactor|test|spec)\("; then
            echo "is_jules=true" >> $GITHUB_OUTPUT
            echo "✅ Jules-PR erkannt: Branch=$BRANCH"
          else
            echo "is_jules=false" >> $GITHUB_OUTPUT
            echo "ℹ️ Kein Jules-PR: Branch=$BRANCH"
          fi

  # ── Gate 2: Triple-Test (3x cargo test — das Herzstück der Qualitätssicherung) ─
  triple-test:
    needs: detect-jules
    if: needs.detect-jules.outputs.is_jules == 'true'
    runs-on: ubuntu-latest
    strategy:
      fail-fast: true  # Wenn Lauf 1 fehlschlägt, Läufe 2+3 sofort abbrechen
      matrix:
        run: [1, 2, 3]
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.head_ref }}

      - name: Rust Toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache
        uses: Swatinem/rust-cache@v2
        with:
          key: ${{ github.head_ref }}-${{ matrix.run }}

      - name: Format Check
        run: cargo fmt --all -- --check

      - name: Clippy (Zero-Warnings-Policy)
        run: cargo clippy --workspace --all-features -- -D warnings

      - name: Zero-Unwrap-Scan (INV: Zero-Panic)
        run: |
          UNWRAPS=$(grep -rn "\.unwrap()\|\.expect(" crates/ --include="*.rs" \
            | grep -v "/tests/" | grep -v "#\[cfg(test)\]" | wc -l)
          echo "Unwrap-Count in Produktionscode: $UNWRAPS"
          if [ "$UNWRAPS" -gt 0 ]; then
            grep -rn "\.unwrap()\|\.expect(" crates/ --include="*.rs" \
              | grep -v "/tests/" | grep -v "#\[cfg(test)\]"
            exit 1
          fi

      - name: Zero-Blocking-IO-Scan (INV: Async-Safety)
        run: |
          BLOCKING=$(grep -rn "std::fs::" crates/ --include="*.rs" \
            | grep -v "/tests/" | grep -v "mod tests" | wc -l)
          if [ "$BLOCKING" -gt 0 ]; then
            echo "❌ Blockierendes std::fs:: gefunden:"
            grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "/tests/"
            exit 1
          fi

      - name: Test-Lauf ${{ matrix.run }}/3
        run: cargo test --workspace --all-features
        env:
          RUST_BACKTRACE: 1
          RUST_LOG: debug

      - name: Test-Lauf ${{ matrix.run }} PASS
        run: echo "✅ Lauf ${{ matrix.run }}/3 erfolgreich"

  # ── Gate 3: Spec-Coverage-Check ─────────────────────────────────────────
  spec-check:
    needs: detect-jules
    if: needs.detect-jules.outputs.is_jules == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.head_ref }}

      - name: ANCHOR-Status prüfen (kein neues BLOCKED ohne Erklärung)
        run: |
          BLOCKED=$(grep -rn "STATUS:BLOCKED" crates/ --include="*.rs" | wc -l)
          echo "Offene BLOCKED-ANKERs: $BLOCKED"
          # Neue BLOCKED-ANKERs müssen im PR-Body erklärt sein
          # (manuelle Prüfung, kein automatischer Fail)

      - name: AGENTS.md existiert
        run: test -f AGENTS.md || (echo "❌ AGENTS.md fehlt!" && exit 1)

  # ── Gate 4: Cargo Security Audit ────────────────────────────────────────
  security:
    needs: detect-jules
    if: needs.detect-jules.outputs.is_jules == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.head_ref }}
      - name: cargo audit
        uses: rustsec/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  # ── Merge: Nur wenn ALLE Gates grün ─────────────────────────────────────
  auto-merge:
    needs: [triple-test, spec-check, security]
    if: |
      always() &&
      needs.triple-test.result == 'success' &&
      needs.spec-check.result == 'success' &&
      needs.security.result == 'success'
    runs-on: ubuntu-latest
    steps:
      - name: Auto-Approve (als CI-Bot)
        run: |
          gh pr review "${{ github.event.pull_request.number }}" \
            --approve \
            --body "✅ Triple-Test-Gate (3/3 PASS) · Zero-Unwrap · Zero-Blocking-IO · Security OK — Auto-approved by CI"
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: Auto-Merge (Squash — saubere Git-History)
        run: |
          gh pr merge "${{ github.event.pull_request.number }}" \
            --auto \
            --squash \
            --delete-branch \
            --subject "$(gh pr view ${{ github.event.pull_request.number }} --json title --jq .title)"
        env:
          GH_TOKEN: ${{ secrets.PAT_TOKEN }}
          # PAT_TOKEN: Personal Access Token mit repo-Scope
          # In GitHub Secrets hinterlegen: Settings → Secrets → Actions
```

### Workflow 2: Kontinuierliches Monitoring (`codebase-health.yml`)

```yaml
# .github/workflows/codebase-health.yml
name: Codebase Health Monitor

on:
  schedule:
    - cron: '0 */6 * * *'  # Alle 6 Stunden
  workflow_dispatch:

jobs:
  health-report:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - name: Vollständiger Health-Check
        run: |
          echo "=== MemFuse Codebase Health Report ===" >> $GITHUB_STEP_SUMMARY
          echo "**Zeitpunkt:** $(date -u)" >> $GITHUB_STEP_SUMMARY
          echo "" >> $GITHUB_STEP_SUMMARY

          # Unwrap-Count
          UNWRAPS=$(grep -rn "\.unwrap()" crates/ --include="*.rs" \
            | grep -v "/tests/" | wc -l)
          echo "**Unwrap in Prod-Code:** $UNWRAPS" >> $GITHUB_STEP_SUMMARY

          # Test Count
          TESTS=$(cargo test --workspace -- --list 2>/dev/null | grep "::" | wc -l)
          echo "**Test-Anzahl gesamt:** $TESTS" >> $GITHUB_STEP_SUMMARY

          # Offene ANKERs
          TODO=$(grep -rn "ANCHOR:.*STATUS:OPEN" crates/ --include="*.rs" | wc -l)
          echo "**Offene TODO-ANKERs:** $TODO" >> $GITHUB_STEP_SUMMARY

          BLOCKED=$(grep -rn "ANCHOR:.*STATUS:BLOCKED" crates/ --include="*.rs" | wc -l)
          echo "**Blockierte ANKERs:** $BLOCKED" >> $GITHUB_STEP_SUMMARY

      - name: cargo test (Triple)
        run: |
          cargo test --workspace && \
          cargo test --workspace && \
          cargo test --workspace
        continue-on-error: true
```

### Workflow 3: Jules via GitHub Action triggern (`invoke-jules.yml`)

```yaml
# .github/workflows/invoke-jules.yml
# Ermöglicht Jules-Tasks direkt aus GitHub Issues zu triggern
name: Invoke Jules from Issue

on:
  issues:
    types: [labeled]

jobs:
  invoke-jules:
    runs-on: ubuntu-latest
    # Nur für Issues mit Label "jules" UND von autorisierten Usern
    if: |
      github.event.label.name == 'jules' &&
      contains(fromJSON('["DEIN_GITHUB_USERNAME"]'), github.event.issue.user.login)
    steps:
      - uses: google-labs-code/jules-invoke@v1
        with:
          prompt: |
            Du bist ein MemFuse Coding Agent. Lies zuerst vollständig:
            1. AGENTS.md im Repository-Root
            2. docs/specs/ für das relevante Work Package
            
            Dann bearbeite die folgende Aufgabe aus dem Issue:
            ${{ github.event.issue.body }}
            
            Halte alle Invarianten aus AGENTS.md ein.
            Schreibe Tests BEVOR du implementierst (TDD).
            Schließe betroffene ANKERs mit STATUS:DONE.
          jules_api_key: ${{ secrets.JULES_API_KEY_01 }}
          # Rotiere API-Keys über Accounts (01-13)
```

---

## 2. Account-zu-WorkPackage Zuweisung

### Feste Zuweisung (in `AGENTS.md` dokumentiert)

```
Account 01 (Jules-Key-01)  → WP-0.0  Tech Debt Core        → Scheduled: Täglicher Audit
Account 02 (Jules-Key-02)  → WP-1.1  Compaction/Store       → Scheduled: Compaction-Stress
Account 03 (Jules-Key-03)  → WP-2.2  Quantization/Index     → Scheduled: Recall-Benchmark
Account 04 (Jules-Key-04)  → WP-1.2  Collections/DB         → Scheduled: Integration-Tests
Account 05 (Jules-Key-05)  → WP-2.1  Hybrid Search/Text     → Scheduled: BM25-Eval
Account 06 (Jules-Key-06)  → WP-3.1  Python Bindings        → Scheduled: Python-Tests
Account 07 (Jules-Key-07)  → WP-5.1  Checkpointing          → Scheduled: MVCC-Proofs
Account 08 (Jules-Key-08)  → WP-5.2  WASM Sandbox           → Scheduled: Security-Tests
Account 09 (Jules-Key-09)  → WP-5.3  Agent Orchestration    → Scheduled: Integration
Account 10 (Jules-Key-10)  → WP-3.2  Encryption             → Scheduled: Crypto-Tests
Account 11 (Jules-Key-11)  → WP-4.1  mmap                   → Scheduled: Perf-Bench
Account 12 (Jules-Key-12)  → WP-4.2  Adaptive Filter        → Scheduled: Filter-Tests
Account 13 (Jules-Key-13)  → RESERVE → Bug-Fixing, Reviews   → Scheduled: Workspace-Tests
```

### GitHub Secrets (einmalig anlegen)

```
Settings → Secrets → Actions → New repository secret:

JULES_API_KEY_01   ← API Key Account 01
JULES_API_KEY_02   ← API Key Account 02
...
JULES_API_KEY_13   ← API Key Account 13
PAT_TOKEN          ← GitHub Personal Access Token (repo scope, für auto-merge)
```

---

## 3. Scheduled Tasks Strategie (die 195 Slots)

### Pro Account: Optimale Slot-Nutzung (15 Slots)

```
TÄGLICHE TASKS (automatisch, jeden Tag):
Slot 01 — 06:00 UTC: Dependency Audit (cargo audit + machete)
Slot 02 — 07:00 UTC: Zero-Unwrap-Scan + Fix (falls Treffer → PR)
Slot 03 — 08:00 UTC: WP-spezifische Tests (3× cargo test)
Slot 04 — 09:00 UTC: Spec-Status aktualisieren (FR-Status in AGENTS.md)

FEATURE-SLOTS (für aktives WP, 5 Tage/Woche):
Slot 05 — 10:00 UTC: Feature-Implementierung Teil 1
Slot 06 — 14:00 UTC: Feature-Implementierung Teil 2 (falls Teil 1 fertig)
Slot 07 — 18:00 UTC: Test-Erweiterung + Edge-Cases

QUALITY-SLOTS (3×/Woche):
Slot 08 — Mo 20:00 UTC: Algorithmischer Proof-Test
Slot 09 — Mi 20:00 UTC: Clippy-Fix + Documentation
Slot 10 — Fr 20:00 UTC: Integration-Test mit anderen WPs

RESERVE (5 Slots):
Slot 11-15 — Reserve für Bug-Fixes, Regressions, Reviewer-Feedback
```

### Wie Scheduled Tasks in Jules eingerichtet werden

Jules unterstützt Scheduled-Workflows via GitHub Actions. Hier ein Beispiel für einen täglichen Security-Scan:

```yaml
# .github/workflows/jules-scheduled-account-01.yml
name: Jules Account 01 — WP-0.0 Daily Audit

on:
  schedule:
    - cron: '0 6 * * *'    # Täglich 06:00 UTC — Slot 01
    - cron: '0 7 * * *'    # Täglich 07:00 UTC — Slot 02
    - cron: '0 8 * * *'    # Täglich 08:00 UTC — Slot 03
    - cron: '0 10 * * 1-5' # Mo-Fr 10:00 UTC — Slot 05 (Weekday)
  workflow_dispatch:

jobs:
  # ── Slot 01: Dependency Audit ────────────────────────────────────────────
  dependency-audit:
    if: github.event_name == 'schedule' && contains(github.event.schedule, '0 6')
    runs-on: ubuntu-latest
    steps:
      - uses: google-labs-code/jules-invoke@v1
        with:
          prompt: |
            [ACCOUNT-01 | WP-0.0 | Dependency Audit | $(date -u +%Y-%m-%d)]
            
            Lies AGENTS.md vollständig. Dann:
            
            1. `cargo audit` — behebe alle kritischen CVEs via PR
            2. `cargo machete` — entferne ungenutzte Dependencies
            3. `cargo tree --duplicates` — dokumentiere doppelte Crates in AGENTS.md
            4. Prüfe ob `once_cell` durch `std::sync::OnceLock` ersetzbar
            
            Wenn Probleme gefunden:
            → Behebe sie direkt
            → Öffne PR mit Titel: "chore(deps): WP-0.0 dependency audit $(date +%Y-%m-%d)"
            
            Wenn alles sauber:
            → Update AGENTS.md: Status "Audit OK $(date +%Y-%m-%d)"
            → Kein PR notwendig
            
            Halte Zero-Panic und Zero-Blocking-IO Invarianten.
          jules_api_key: ${{ secrets.JULES_API_KEY_01 }}
          branch: develop

  # ── Slot 02: Zero-Unwrap Fix ─────────────────────────────────────────────
  unwrap-elimination:
    if: github.event_name == 'schedule' && contains(github.event.schedule, '0 7')
    runs-on: ubuntu-latest
    steps:
      - uses: google-labs-code/jules-invoke@v1
        with:
          prompt: |
            [ACCOUNT-01 | WP-0.0 | Zero-Unwrap Scan | $(date -u +%Y-%m-%d)]
            
            Führe aus:
            grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "/tests/"
            
            Für jeden Treffer in Produktionscode:
            1. Ersetze .unwrap() durch ? mit passendem MemFuseError
            2. Schreibe einen Test der den Fehlerfall abdeckt
            3. Verifiziere: cargo test -p [betroffene-crate] muss grün sein
            
            PR-Titel: "fix(core): eliminate unwrap in [module] WP-0.0"
            
            NIEMALS .expect() als Ersatz verwenden.
            NIEMALS mehr als eine Datei pro PR ändern (kleiner Scope = schnelleres Review).
          jules_api_key: ${{ secrets.JULES_API_KEY_01 }}
          branch: develop

  # ── Slot 05: Feature-Implementierung (Mo-Fr) ─────────────────────────────
  feature-implementation:
    if: github.event_name == 'schedule' && contains(github.event.schedule, '0 10')
    runs-on: ubuntu-latest
    steps:
      - uses: google-labs-code/jules-invoke@v1
        with:
          prompt: |
            [ACCOUNT-01 | WP-0.0 | Feature | $(date -u +%Y-%m-%d)]
            
            Lies ZUERST vollständig:
            1. AGENTS.md — deinen aktuellen Status und nächste Aufgabe
            2. docs/specs/SPEC-20260505-WP-0.0-DependencyAudit.md
            
            Bearbeite das nächste OFFENE Item aus der Spec (Status: OPEN).
            Schreibe Tests BEVOR du implementierst (Red → Green → Refactor).
            
            Nach Implementierung:
            → cargo test --workspace (muss 3x grün sein)
            → Aktualisiere Spec: FR-Status auf [v] Getestet
            → Schließe betroffene ANKERs: STATUS:DONE
            → PR öffnen
          jules_api_key: ${{ secrets.JULES_API_KEY_01 }}
          branch: develop
```

---

## 4. AGENTS.md — Das Kommandozentrum

Das ist die wichtigste Datei. Jules sucht automatisch nach einer Datei namens AGENTS.md im Repository-Root. Diese Datei beschreibt Agents und Tools, deren Konventionen und wie Jules damit interagiert.

```markdown
# AGENTS.md — MemFuse Development Command Center
> **Pflichtlektüre für ALLE Coding Agenten vor jeder Aktion**
> **Letzte Aktualisierung:** [Datum] von [Agent/Mensch]

## 🏗️ Projektarchitektur

MemFuse ist "SQLite für KI-Agenten" — Rust-basierte Vektor+Hybrid-DB.
Crate-Hierarchie (DAG, keine zyklischen Abhängigkeiten):

  memfuse-py → memfuse-db → memfuse-store, memfuse-index, memfuse-text → memfuse-core

**EISERNES GESETZ: memfuse-core importiert KEINE anderen internen Crates.**

## ⚖️ Absolute Gesetze (NIEMALS brechen)

1. **Zero-Panic**: Kein `.unwrap()`, `.expect()`, `panic!()` in Produktionscode.
   Propagiere Fehler via `?` in `MemFuseError`.

2. **Zero-Blocking-IO**: Nur `tokio::fs` in async-Kontexten. Kein `std::fs`.

3. **TDD-Pflicht**: Tests werden GESCHRIEBEN bevor die Implementierung beginnt.
   Red → Green → Refactor. Kein Code ohne Test.

4. **Triple-Test-Gate**: Vor jedem PR: `cargo test --workspace` muss 3× grün sein.

5. **ANCHOR-Protokoll**: Jede wichtige Entscheidung, jedes offene Problem bekommt
   einen ANCHOR-Kommentar im Code. Nie stumm entscheiden.

## 👥 Account-Zuweisung & Status

| Account | Zuständig für | WP | Status | Letzter PR |
|---------|-------------|-----|--------|-----------|
| Account-01 | Tech Debt Core | WP-0.0 | IN PROGRESS | #XX |
| Account-02 | LSM/Compaction | WP-1.1 | SPEC READY | — |
| Account-03 | HNSW/SQ8 | WP-2.2 | WAITING | — |
| Account-04 | Collections | WP-1.2 | WAITING | — |
| Account-05 | Hybrid Search | WP-2.1 | WAITING | — |
| Account-06 | Python Bindings | WP-3.1 | WAITING | — |
| Account-07 | Checkpointing | WP-5.1 | WAITING | — |
| Account-08 | WASM Sandbox | WP-5.2 | WAITING | — |
| Account-09 | Agent Orch. | WP-5.3 | WAITING | — |
| Account-10 | Encryption | WP-3.2 | WAITING | — |
| Account-11 | mmap | WP-4.1 | WAITING | — |
| Account-12 | Adaptive Filter | WP-4.2 | WAITING | — |
| Account-13 | Reserve/Bugfix | — | STANDBY | — |

## 📋 Entwicklungs-Reihenfolge (STRIKT)

Phase 0: WP-0.0 (AKTIV — Account 01 arbeitet daran)
Phase 1: WP-1.1, WP-1.2 (startet wenn WP-0.0 DONE)
Phase 2: WP-2.1, WP-2.2 (startet wenn Phase 1 DONE)
Phase 3: WP-3.1, WP-3.2 (startet wenn Phase 2 DONE)
Phase 4: WP-5.1 → WP-5.2 → WP-5.3 (SAOS)
Phase 5: WP-4.1, WP-4.2, WP-4.3 (Hyper-Scale)

**WARTE auf vorherige Phase bevor du anfängst. Prüfe Status oben.**

## 🛠️ Entwicklungsumgebung

```bash
# Vollständiger Check (vor jedem PR)
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace  # Run 1
cargo test --workspace  # Run 2
cargo test --workspace  # Run 3

# Audit
cargo audit
cargo machete
```

## 📂 Spec-Dateien (lese die relevante VOR dem Start)

- `docs/specs/SPEC-20260505-WP-0.0-DependencyAudit.md`
- `docs/specs/SPEC-20260505-WP-1.1-Compaction.md`
- `docs/specs/SPEC-20260505-WP-1.2-Collections.md`
- `docs/specs/SPEC-20260505-WP-2.1-HybridSearch.md`
- `docs/specs/SPEC-20260505-WP-2.2-Quantization.md`
- `docs/specs/SPEC-20260505-WP-3.1-PythonBindings.md`
- `docs/specs/SPEC-20260505-WP-3.2-Encryption.md`

## 🔀 Merge-Strategie

PRs werden automatisch gemerged wenn:
  ✅ Triple-Test-Gate (3× cargo test grün)
  ✅ Zero-Unwrap in Produktionscode
  ✅ Zero-Blocking-IO
  ✅ cargo audit sauber
  
Merge-Ziel: `develop` (NIEMALS direkt in `main`)
```

---

## 5. Die Rolle jedes Tools (Betriebsmodell)

```
┌─────────────────────────────────────────────────────────────────┐
│                    DU (Context-Architekt)                        │
│  Antigravity → Specs schreiben, AGENTS.md pflegen, Audits        │
└──────────────────────────┬──────────────────────────────────────┘
                           │ definiert Specs + AGENTS.md
           ┌───────────────▼───────────────────────────┐
           │          GitHub Repository                  │
           │  develop ← [Auto-Merge Pipeline] ← Jules-PRs│
           │  main ← release branches                    │
           └──────┬───────────────┬─────────────────────┘
                  │               │
    ┌─────────────▼──┐    ┌───────▼──────────────────────┐
    │ Jules (13x)    │    │ GitHub Actions (dein CI/CD)  │
    │ Pro WP 1 Task  │    │ Triple-Test + Auto-Merge     │
    │ Liest AGENTS.md│    │ Health-Monitoring 6h         │
    │ Öffnet PRs     │    │ Scheduled Jules-Invokes      │
    └────────────────┘    └──────────────────────────────┘
                  │
    ┌─────────────▼───────────────────────────────────────┐
    │ Gemini-CLI (13x, manuell wenn du am PC bist)        │
    │ → Spec-Review, komplexe Analysen, PR-Qualität       │
    │ → Kontext-Vorbereitung für Jules-Tasks              │
    │ → AGENTS.md-Updates wenn Dinge unklar sind          │
    └─────────────────────────────────────────────────────┘
```

### Tool-Nutzung im Detail

**Antigravity (Claude Code — du startest es manuell):**
- Algorithmic Audit (der Elite-Prompt)
- SAOS-Spec-Erstellung
- AGENTS.md-Updates wenn Architektur sich ändert
- Kritische Code-Reviews die Jules nicht selbst machen kann
- Maximal 1-2× täglich wenn du aktiv bist

**Jules (läuft 24/7 via Scheduled Tasks):**
- Implementiert was die Specs sagen
- Schreibt Tests bevor Code
- Öffnet PRs die automatisch gemerged werden
- Arbeitet auch wenn du schläfst

**Gemini-CLI (wenn du am PC bist):**
- Schnelle Analysen: `gemini "Was ist der Status von WP-1.1?"`
- PR-Beschreibungen verbessern
- Jules-Prompts verfeinern bevor Scheduled Tasks laufen
- Spec-Lücken identifizieren

**GitHub Actions (läuft permanent automatisch):**
- Triple-Test-Gate vor jedem Merge
- Health-Reports alle 6 Stunden
- Jules-Invokes nach Zeitplan
- Auto-Merge wenn alles grün

---

## 6. Tagesablauf (Realistischer Betrieb)

```
MORGENS (15 Minuten):
  • GitHub aufmachen → Pull Requests checken
  • Welche PRs wurden auto-gemerged? → develop aktuell?
  • Welche PRs sind FEHLGESCHLAGEN? → warum? → AGENTS.md korrigieren
  • AGENTS.md Status-Tabelle aktualisieren
  
ABENDS OPTIONAL (30 Minuten — wenn du Zeit hast):
  • Antigravity starten → einen Algorithmic-Audit Schritt durchführen
  • Gemini-CLI: komplexe Spec-Fragen klären
  • Morgige Jules-Prompts verfeinern falls nötig
  
ALLES ANDERE LÄUFT AUTOMATISCH:
  → Jules arbeitet 24/7 an seinen WPs
  → GitHub Actions mergt grüne PRs automatisch
  → Health-Monitoring läuft alle 6h
```

---

## 7. Problemlösungen & Fallstricke

### Problem: Jules öffnet PR aber CI schlägt fehl

```
Ursache 1: Jules hat .unwrap() geschrieben → Zero-Unwrap-Gate schlägt fehl
Lösung: Account-13 (Reserve) bekommt Task:
  "Fix the failing CI in PR #XX: replace all .unwrap() calls with proper error handling"

Ursache 2: Tests sind rot (Red-Phase nicht abgeschlossen)
Lösung: Jules-Task war zu groß → in AGENTS.md vermerken: "WP-1.1 Implementierung
  aufteilen in: 1) Tests schreiben, 2) Implementierung"
```

### Problem: Zwei Jules-Accounts arbeiten an demselben File → Merge-Konflikt

```
Lösung: AGENTS.md ist die Source-of-Truth für Dateizuständigkeit.
Jede Datei gehört genau einem Account.
Bei Konflikt: Account-13 bekommt Aufgabe "Resolve merge conflict in [datei]"
```

### Problem: Jules ignoriert AGENTS.md

```
AGENTS.md befindet sich im Repository-Root? → Ja, Jules liest sie automatisch.
Wenn Jules trotzdem abweicht: Prompt-Präambel verschärfen:
  "STOP. Lies AGENTS.md vollständig (alle Zeilen) BEVOR du irgendetwas tust."
```

### Problem: Daily Task Limit (15/Tag) erschöpft

```
Rolling 24h-Fenster: Tasks von vor 24h werden frei.
Strategie: Über die Accounts rotieren.
Account 01 voll → Account 13 (Reserve) übernimmt den dringenden Task.
```

### Problem: PAT_TOKEN für Auto-Merge nötig

```
Der GITHUB_TOKEN der Actions hat keine Berechtigung PRs zu mergen die von ihm selbst
erstellt wurden (Sicherheits-Loop-Prevention).
Lösung: Personal Access Token erstellen:
  GitHub → Settings → Developer Settings → Personal Access Tokens → Fine-grained
  → Repository: memfuse
  → Permissions: Pull requests (read+write) + Contents (write)
  → Token in Secrets als PAT_TOKEN hinterlegen
```

---

## 8. Erfolgsmessung

### Tägliche KPIs (in GitHub Actions Step Summary sichtbar)

```
Ziel nach 4 Wochen:
  - WP-0.0: DONE (Zero-Panic, Zero-Blocking-IO)
  - WP-1.1: IN TESTING
  - WP-1.2: IN PROGRESS
  - Auto-Merge Rate: > 80% der Jules-PRs (20% manuell wegen Konflikten)
  - Triple-Test-Gate: 100% (kein PR merged ohne 3× grün)
  - Unwrap in Prod: 0

Gesund-Indikatoren:
  ✅ develop-Branch ist immer grün
  ✅ Kein PR wartet > 1 Tag auf Merge (CI läuft sofort)
  ✅ AGENTS.md wird täglich aktualisiert
  ✅ Alle Spec-FRs haben klaren Status
```
