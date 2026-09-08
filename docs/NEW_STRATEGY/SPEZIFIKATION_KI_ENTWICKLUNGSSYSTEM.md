# MemFuse — KI-Entwicklungssystem & Governance-Spezifikation
> **Dokument-ID:** SPEC-20260908-GOVERNANCE-ARCH
> **Rolle:** Principal Senior Rust Architect Review & System-Spezifikation
> **Repository:** `github.com/tfufuz1/memfuse`
> **Stand:** 2026-09-08
> **Zielgruppe:** Solo-Entwickler (`tfufuz1`), Google-Jules KI-Agenten-Schwarm, Claude Orchestrator

---

## 1. Executive Summary & System-Analyse der KI-Entwicklungsumgebung

Das Softwareprojekt **MemFuse** stellt eine hochperformante, eingebettete AI-Datenbankinfrastruktur in Rust dar. Es zeichnet sich durch ein außergewöhnliches Entwicklungsmodell aus: Ca. **78 % aller Commits** der letzten 14 Tage (~1.030 Commits) wurden vollständig autonom von einer Flotte aus **Google-Jules AI-Coding-Agenten** (bis zu 25 parallele Instanzen, max. 195 Sessions pro Tag) generiert, gesteuert von einem Solo-Entwickler in Kombination mit einem Claude-3.5/3.7-Analyse-Prompting-System (`Memfuse-Prompter-v24.html`).

Eine tiefgreifende Architektur- und Audit-Analyse des gesamten Entwicklungssystems deckt ein gravierendes Paradoxon auf: **MemFuse besitzt eines der ausgefeiltesten theoretischen Governance-Systeme in Open-Source-Rust-Projekten (Regeln, Verfassung, Veto-Register, ADRs, Tag-Taxonomien, CI-Gates) — jedoch ist ein signifikanter Teil dieser Infrastruktur praktisch wirkungslos, fehlerhaft verdrahtet, widersprüchlich oder verwaist.**

### Die 3 Kern-Ursachen des System-Versagens

1. **Kontext-Lade-Nicht-Determinismus:**
   Das System baut auf der Annahme auf, dass `AGENTS.md` und `.jules/`-Richtlinien von Google-Jules "ambient" (automatisch unsichtbar im Hintergrund) geladen werden. Die Live-Analyse von Jules' Arbeitsweisen (`JULES_LOG.md` vs. `JULES_LOG_2.md`) beweist eindeutig: **Jules lädt diese Dateien NICHT automatisch.** Das Laden erfolgt ausschließlich, wenn es vom Start-Prompt explizit instruiert wird oder der Agent zufällig per Tool-Call darauf stößt.
2. **Kollisionen paralleler Agenten-Sessions (Lack of Concurrency Control):**
   Durch bis zu 25 zeitgleich agierende Jules-Instanzen ohne zentrales Locking- oder Reservierungssystem kommt es regelmäßig zu Massen-Kollisionen. Das dramatischste Beispiel: Am 07.09.2026 wurde die Identische Anforderung (Router-Kalibrierungs-Fingerprint, ADR-063) **innerhalb von 66 Minuten drei Mal unabhängig voneinander in verschiedenen Layern implementiert** (#1627, #1634, #1645), was zu einem kritischen Syntax-/Logik-Defekt in der final gemergten Version führte.
3. **Infrastruktur-Drift & Toter Code in xtask:**
   Ca. **50 KB fertiger Rust-Code in `xtask/src/`** (darunter ein produktionsreifer Preflight-Checker `jules_preflight.rs`) sind nicht in `xtask/src/main.rs` als Module deklariert. Sie existieren als "tote Dateien" auf der Festplatte, während CI-Workflows (`context-gates.yml`) versuchen, diese nicht-existenten xtask-Subcommands aufzurufen.

---

## 2. Tiefenanalyse aller Systemkomponenten & Schwachstellen-Katalog

### 2.1 Entwickler-Environment & Bootstrapping (`environment_script.sh` & `.jules/`)

* **Ablauf-Lücke im Setup-Skript:**
  `environment_script.sh` springt in der Ausgabe kommentarlos von `[3/8]` (ONNX Check) zu `[5/8]` (`just` install). Schritt `[4/8]` fehlt im Skript ersatzlos. Grund: Ursprünglich war dort die Installation von `ast-grep` (`sg`) vorgesehen, welche entfernt wurde.
* **Fehlendes `ast-grep` (`sg`):**
  Das `justfile` rufen im Target `debt-audit` das Tool `sg scan` auf (mithilfe von `rules/detect_nested_locks.yml`). Da `ast-grep` in der Jules-VM weder vorinstalliert noch vom Setup-Skript installiert wird, schlägt die Prüfung auf verschachtelte Lock-Hierarchien (Lock-Ordering-Verletzungen laut `CONSTITUTION.md`) leise fehl.
* **Inaktiver Pre-Commit-Hook:**
  Eine Hook-Datei `.githooks/pre-commit` existiert im Repository, aber `git config core.hooksPath .githooks` wird weder im Setup-Skript noch im Bootstrapping ausgeführt. Commits werden in der Jules-VM daher ohne lokale Pre-Commit-Prüfung erzeugt.

### 2.2 Dokumentations-Landschaft & Single Source of Truth

* **Falschaussage in `AGENTS.md`:**
  `AGENTS.md` (laut quellenhierarchischem Grundsatz die absolut höchste Autorität) behauptet explizit, dass `memfuse-kv-bridge` nicht existiere. In Realität ist `memfuse-kv-bridge` als voll funktionsfähiges Crate mit LRU-Eviction, Multi-Tenant Isolation und Unit-Tests implementiert.
* **Massive ADR-Nummernkollisionen:**
  In `docs/decisions/` existieren **4 verschiedene Dateien als `ADR-070`** und **2 verschiedene als `ADR-072`**. Grund: Parallele Agenten-Sessions haben unabhängig voneinander freie ADR-Nummern gewählt.
* **Ignorierter ADR-060:**
  ADR-060 (beschlossen am 2026-09-04) ordnete die Auflösung des Verzeichnisses `docs/decisions/` zugunsten einer zentralen `DECISIONS.md` an. Diese Entscheidung wurde nie umgesetzt; stattdessen wurden 12 weitere Einzel-ADR-Dateien angelegt.
* **Invalide Pfade und Anker:**
  In mehreren Dokumenten (`SECURITY.md`, `AUDIT_INTAKE_PROTOCOL.md`, `TYPE_REGISTRY.md`) wird auf Anker wie `AGENTS.md §1/§3` verwiesen, obwohl `AGENTS.md` keine Paragraphennummerierung besitzt. `TYPE_REGISTRY.md` referenziert `crates/memfuse-db/src/collection.rs`, welche zu einem Verzeichnis `collection/` refaktoriert wurde.

### 2.3 Tools & Governance-Gates (`xtask` & GitHub Actions)

* **Toter Code in `xtask`:**
  Folgende Module in `xtask/src/` sind nicht in `main.rs` eingebunden:
  - `jules_preflight.rs`: Vollständiger Preflight-Aggregator.
  - `check_duplicate_intent.rs`: Vom CI-Workflow `context-gates.yml` aufgerufen, bricht dort ab!
  - `init_audit_fix.rs`, `generate_adr.rs`, `check_type_registry.rs`, `validate_pr_checklist.rs`.
* **CI-Effizienz & Flaky-Handling:**
  `rust-ci.yml` führt den gesamten Workspace-Testlauf dreimal hintereinander aus ("Triple-Run"), um Flakiness zu erkennen. Dies verdreifacht die CI-Laufzeit und Runner-Kosten unnötig.

### 2.4 Prompting-System (`Memfuse-Prompter-v24.html`)

* **Unvollständige Prompt-Generierung:**
  Der HTML-Baukasten generiert Instruktionen für Jules, versäumt es aber, den Pfad zu `.jules/SESSION_BOOTSTRAP.md` und die crate-spezifischen `AGENTS.md`-Dateien zwingend als **1. Ladeschritt** voranzustellen.
* **Fehlender Kollisionsschutz:**
  Der Prompter erlaubt dem Entwickler das Starten von N parallelen Sessions ohne Prüfung, ob für das Ziel-Crate bereits eine aktive Bearbeitung vorliegt.

---

## 3. Architektur-Spezifikation für das KI-Entwicklungssystem (Ziel-Zustand)

Um die Entwicklung von MemFuse für die Finale Produktphase auf maximale Stabilität und Effizienz zu heben, wird folgendes 5-Säulen-Governance-System spezifiziert:

```
+-----------------------------------------------------------------------------------+
|                            MEMFUSE GOVERNANCE ENGINE                              |
+-----------------------------------------------------------------------------------+
                                          |
     +------------------------------------+------------------------------------+
     |                                    |                                    |
     v                                    v                                    v
[SÄULE 1: PREFLIGHT]             [SÄULE 2: LOCKING]                 [SÄULE 3: DOKU-SYNC]
cargo xtask jules-preflight      .jules/ACTIVE_CLAIMS.json           cargo xtask sync-docs
Erzwungene Session-Validierung   Session-Lock per Crate/ADR         Single Source of Truth
     |                                    |                                    |
     +------------------------------------+------------------------------------+
                                          |
                                          v
                              [SÄULE 4: CI GUARDRAILS]
                              GitHub Actions & Git Hooks
                              Unbending Safety Checks
```

### 3.1 Säule 1: Der `jules-preflight` Orchestrator

Das Modul `xtask/src/jules_preflight.rs` wird vollständig in `main.rs` verdrahtet und bildet das zentrale Eintrittstor für jede Entwickler- und Agenten-Session.

#### Spezifikation `cargo xtask jules-preflight`:
1. **System & Toolchain Verification:** Prüft Rust-Version, `clippy`, `rustfmt` und das Vorhandensein von `ast-grep` (`sg`).
2. **Context Freshness Gate:** Prüft `cargo xtask check-jules-context-freshness` auf Veraltung von `AGENTS.md` und `JULES_CONTEXT.md`.
3. **Unwrap Baseline Check:** Prüft `cargo xtask check-unwrap-baseline` gegen `.unwrap-baseline.json`.
4. **Veto-Register Check:** Prüft `cargo xtask check-vetoes` gegen `VETOES.md`.
5. **DAG-Topology Check:** Prüft `cargo xtask check-dag` auf Layer-Verletzungen (Layer 0–6).
6. **Active Claims Check:** Verifiziert, ob die aktuelle Session ein exklusives Claim auf das Ziel-Crate hält.

### 3.2 Säule 2: Anti-Collision Concurrency Lock (`ACTIVE_CLAIMS.json`)

Um Mehrfach-Implementierungen durch parallele Jules-Sessions zu verhindern, wird ein dateibasiertes, atomares Claim-System eingeführt.

* **Speicherort:** `.jules/ACTIVE_CLAIMS.json`
* **Schema:**
```json
{
  "claims": [
    {
      "crate_name": "memfuse-router",
      "feature_or_adr": "ADR-063",
      "session_id": "2c814094",
      "claimed_at": "2026-09-08T14:30:00Z",
      "ttl_minutes": 120
    }
  ]
}
```
* **Integration:**
  - `cargo xtask claim --crate <CRATE> --issue <ADR/FEATURE>` setzt eine Reservierung.
  - Wenn ein Crate reserviert ist, verweigert `cargo xtask jules-preflight` in anderen Sessions auf demselben Crate die Schreibfreigabe mit einem klaren Hinweis.

### 3.3 Säule 3: Single Source of Truth & Doku-Synchronisation

* **Automatisierte Dokumentation:**
  Inhalt von `WORKING_STATE.md` und Crate-Beständen wird exklusiv durch `cargo xtask sync-docs` erzeugt.
* **Echtzeit-Validierung von `AGENTS.md`:**
  Ein neues Gate `cargo xtask check-agents-integrity` prüft, dass:
  1. Jedes im `Cargo.toml`-Workspace vorhandene Crate in `AGENTS.md` mit korrekter Beschreibung gelistet ist.
  2. Keine veralteten Falsaussagen (wie die zu `memfuse-kv-bridge`) existieren.
* **Atomare ADR-Vergabe:**
  `cargo xtask generate-adr` liest die höchste im Ordner `docs/decisions/` vorhandene dreistellige Zahl dynamisch aus, sperrt die Reservierung in `ACTIVE_CLAIMS.json` und verhindert dadurch ADR-Nummern-Kollisionen (wie bei ADR-070).

### 3.4 Säule 4: Gehärtete CI/CD Guardrails & Git Hooks

* **`context-gates.yml` Reparatur:**
  Ersatz des defekten `check-duplicate-intent`-Aufrufs durch den vollwertigen `cargo xtask jules-preflight`.
* **Git Pre-Commit Enforcer:**
  Aufnahme von `git config core.hooksPath .githooks` in `environment_script.sh` und `justfile`. Der Pre-Commit-Hook führt lokal vor jedem Commit `cargo fmt --check`, `cargo clippy` und `cargo xtask check-unwrap-baseline` aus.
* **CI Test-Optimierung:**
  Ersetzung des ineffizienten "Triple-Runs" in `rust-ci.yml` durch `cargo nextest` mit gezieltem Flag `--retries 2`.

### 3.5 Säule 5: Prompter-v25-Integration & Handoff Protocol

* **Zwei-Phasen-Entwicklungs-Protokoll (Claude -> Jules):**
  1. **Phase 1 (Claude Analysis):** Claude analysiert die Architektur, liest ADRs und erstellt eine präzise Task-Spezifikation inklusive Angabe des betroffenen Crates und der ADR-Nummer.
  2. **Phase 2 (Jules Execution):** Jules empfängt den Prompt aus `Memfuse-Prompter-v25`.
* **Injektion des Mandatory Bootstrap Header:**
  Jeder vom Prompter generierte Anweisungstext beginnt zwingend mit folgendem Ladebefehl für Jules:
  ```
  [MANDATORY BOOTSTRAP] Bevor du Code änderst, führe folgende Aktionen aus:
  1. read_file(".jules/SESSION_BOOTSTRAP.md")
  2. read_file("crates/<TARGET_CRATE>/AGENTS.md")
  3. run_in_bash_session("cargo xtask claim --crate <TARGET_CRATE> --issue <TASK_ID>")
  4. run_in_bash_session("cargo xtask jules-preflight")
  ```

---

## 4. Konkreter Umsetzungs- & Migrationsplan

### Phase 1: Notfall-Reparatur der Infrastruktur (Sofort)
1. Verdrahten aller 6 verwaisten Module in `xtask/src/main.rs`.
2. Beheben der Aufrufe in `.github/workflows/context-gates.yml`.
3. Korrigieren der Falsaussage zu `memfuse-kv-bridge` in `AGENTS.md`.
4. Hinzufügen von `ast-grep` (Schritt `[4/8]`) in `.jules/setup/environment_script.sh`.

### Phase 2: ADR-Bereinigung & Locking-Einführung
1. Zusammenführen/Umbenennen der duplizierten ADR-070- (4x) und ADR-072-Dateien (2x).
2. Implementieren des Claim-Befehls in `xtask` (`cargo xtask claim`) und Verankerung in `.jules/ACTIVE_CLAIMS.json`.
3. Ausweiten von `generate_adr.rs` auf dynamische Ordnerinspektion.

### Phase 3: Prompter v25 & CI-Härtung
1. Erneuern von `Memfuse-Prompter-v24.html` zu v25 mit automatischem Bootstrap-Präfix.
2. Umstellen von `rust-ci.yml` auf `nextest` mit Retries.
3. Bereinigung veralteter Pfadreferenzen in `SECURITY.md`, `TESTING.md` und `TYPE_REGISTRY.md`.

---
*Spezifikation erstellt und verifiziert durch Principal Senior Rust Architect Review.*
