# Systematische Bestandsaufnahme: Temporäre, redundante & verbliebene Dateien

> **Stand:** 2026-09-07  
> **Repository:** `memfuse` (`HEAD`)  
> **Fokus:** Gründliche Überprüfung des Dateibaums auf temporäre Artefakte, veraltete Pläne, Test-Rückstände und Redundanzen.

---

## 1. Übersicht & Klassifizierung

Im Rahmen der systematischen Überprüfung des gesamten Repository-Dateibaums (`find`, `git status`, `git check-ignore`, `xtask check-consistency`) wurden alle Dateien auf ihre Notwendigkeit, Gültigkeit und Zugehörigkeit hin analysiert.

| Kategorie | Gefundene Dateien | Status | Bewertung / Empfehlung |
|---|---|---|---|
| **A. Gelöschte Planungsdateien** | `implementation_plan_2.md`, `Implementationplan.md` | Bereits gelöscht ✅ | Vollständig erledigt. |
| **B. Test-Artefakt (Unversioniert)** | `crates/memfuse-checkpoint/memfuse_orphaned_pins.json` | 🗑️ **Löschbar** | Temporäres Test-Generat aus Checkpoint-Tests. Nicht ignoriert durch `*_orphaned_checkpoints.json`. |
| **C. ADR-Nummern-Kollision (Governance)** | `docs/decisions/ADR-064-duplicate-symbol-ci-gate.md` vs. `docs/decisions/ADR-064-memfuse-py-separater-workspace-panic-strategie.md` | ⚠️ **Umzubenennen** | Kollision blockiert `cargo xtask check-consistency`. Eine der beiden Dateien muss auf `ADR-065` umnummeriert werden. |
| **D. Temporäre Test-JSONs (Git-Ignored)** | `crates/memfuse-checkpoint/*_orphaned_checkpoints.json`, `crates/memfuse-agent/agent_step_orphaned_checkpoints.json` | 🟡 **Ignoriert / Löschbar** | Durch `.gitignore` maskiert, verbleiben aber lokal auf der Platte nach Testläufen. |
| **E. Historische Audit- & Prompt-Dateien** | `docs/audits/*`, `docs/archive/*`, `docs/prompts/*`, `docs/reviews/*` | 🟢 **Behalten** | Dienen als Nachweisdokumente für Jules/Audit-Pässe und werden von CI/Governance referenziert. |
| **F. Root- & Konfigurations-Dateien** | `AGENTS.md`, `CONSTITUTION.md`, `VETOES.md`, `WORKING_STATE.md`, `justfile`, `flake.nix`, etc. | 🟢 **Behalten** | Alle aktiv im Build-, CI- oder Governance-Prozess eingebunden. |

---

## 2. Detaillierte Bewertung pro Datei

### 🗑️ Kategorie B & D: Temporäre Test-Rückstände (Löschbar)

#### 1. `crates/memfuse-checkpoint/memfuse_orphaned_pins.json`
* **Zweck / Herkunft:** Wird bei Unit-Tests der `PersistentCheckpointStore`- / `PinGuard`-Logik erzeugt, wenn kein Namespacing greift.
* **Problem:** `.gitignore` enthält `memfuse_orphaned_checkpoints.json` und `*_orphaned_checkpoints.json`, aber nicht `memfuse_orphaned_pins.json`. Daher bleibt die Datei als unversioniertes Artefakt im Repo liegen.
* **Bewertung:** **Sofort löschbar** 🗑️. Zudem sollte `.gitignore` um `*_orphaned_pins.json` und `memfuse_orphaned_pins.json` ergänzt werden.

#### 2. `crates/memfuse-checkpoint/*_orphaned_checkpoints.json` & `crates/memfuse-agent/*_orphaned_checkpoints.json`
* **Dateien (15 Stück):**
  - `crates/memfuse-checkpoint/ns_a_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_c_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_b_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_barrier_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_panic_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/prop_ns_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_e_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/ns_inst_a_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/ns_b_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_orphan_recovery_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/ns_guard_alpha_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_auto_rollback_orphaned_checkpoints.json`
  - `crates/memfuse-checkpoint/test_guard_rollback_on_drop_orphaned_checkpoints.json`
  - `crates/memfuse-agent/agent_step_orphaned_checkpoints.json`
* **Bewertung:** **Löschbar / Bereinigbar** 🗑️. Diese Dateien sind zwar git-ignored, verunreinigen aber das Arbeitsverzeichnis nach lokalen Testausführungen.

---

### ⚠️ Kategorie C: Governance-Konflikte & Synchronisations-Drift

#### 1. Doppelte ADR-Nummer `ADR-064`
* **Betroffene Dateien:**
  - `docs/decisions/ADR-064-duplicate-symbol-ci-gate.md` (Commit `41268a55`)
  - `docs/decisions/ADR-064-memfuse-py-separater-workspace-panic-strategie.md` (Commit `134c50d0`)
* **Befund:** `cargo xtask check-consistency` schlägt mit folgendem Fehler fehl:
  ```text
  ❌ Consistency error: ADR-064 ist doppelt vergeben!
  ```
* **Bewertung:** **Nicht löschbar, sondern umzubenennen.** `ADR-064-duplicate-symbol-ci-gate.md` oder die Py-Workspace-ADR muss auf `ADR-065` angepasst werden, und `docs/decisions/README.md` muss aktualisiert werden.

#### 2. Doku-Drift in `WORKING_STATE.md` und `docs/SOURCE_OF_TRUTH.md`
* **Befund:** `cargo xtask sync-docs --check` meldet Drift in der Crate-Tabelle und Tag-Statistik durch neu hinzugefügte Workspace-Crates (`memfuse-candle`, `memfuse-kv-bridge`).
* **Bewertung:** Mit `cargo xtask sync-docs` synchronisieren.

---

### 🟢 Kategorie E: Dokumente & Audits (Behalten)

| Pfad / Bereich | Inhalt | Status | Begründung |
|---|---|:---:|---|
| `docs/audits/AUDIT_*.md` | Crate-Audits (Runde 1 & 2) | Behalten 🟢 | Dienen als Audit-Trail für ISO-/Sicherheits-Nachweise. |
| `docs/audits/GATE7_VALIDATE_TAGS_FIX.md` | Protokoll für Gate-7-Absicherung | Behalten 🟢 | Referenzbericht für CI-Gate 7. |
| `docs/audits/PHASE2_*.md` | Implementierungsberichte (DiskANN, Provenance, Routing) | Behalten 🟢 | Belege für ADR-Erfüllung. |
| `docs/prompts/` | Prompt-Vorlagen für Jules-Audits | Behalten 🟢 | Dokumentieren die Test- & Audit-Methodik. |
| `docs/reviews/` | Review-Berichte externer Prüfer | Behalten 🟢 | Qualitätsnachweis für Security & Unsafe-Checks. |
| `docs/decisions/` | ADR-001 bis ADR-064 | Behalten 🟢 | Kanonischer Entscheidungskontext (`CONSTITUTION.md`). |

---

## 3. Handlungsempfehlungen

1. **Test-Artefakte bereinigen:**
   - Datei `crates/memfuse-checkpoint/memfuse_orphaned_pins.json` löschen.
   - Alle temporären `*_orphaned_checkpoints.json` in den Crate-Ordnern aufräumen.
   - `.gitignore` um `*_orphaned_pins.json` und `memfuse_orphaned_pins.json` erweitern.
2. **ADR-Kollision beheben:**
   - `docs/decisions/ADR-064-duplicate-symbol-ci-gate.md` in `ADR-065-duplicate-symbol-ci-gate.md` umbenennen.
   - `docs/decisions/README.md` aktualisieren.
3. **Dokumentation synchronisieren:**
   - `cargo xtask sync-docs` ausführen, um `WORKING_STATE.md` und `SOURCE_OF_TRUTH.md` auf den aktuellen 18-Crate-Stand zu bringen.
