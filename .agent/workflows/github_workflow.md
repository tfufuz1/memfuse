---
description: GitHub Versioning and Collaboration Workflow for Memfuse Coding Agents
---

# GitHub-Workflow & Qualitäts-Gates

Der Prozess zur Übernahme von Code ins Hauptrepository ist durch automatisierte Gate-Barrieren geschützt. Jeder Agent MUSS sicherstellen, dass er die Gates passiert.

## 3 Core Gates

| Gate | Akteur | Prüfung |
| --- | --- | --- |
| **Gate 1 (Commit)** | Agent | Lokale Tests (Triple-Test-Gate) & Linting (`cargo clippy -- -D warnings` & `cargo fmt`) müssen GRÜN sein. |
| **Gate 2 (PR)** | Agent | Bei Eröffnung eines Pull Requests: Vergleich der Implementierung gegen die Akzeptanzkriterien (ACs) der Spec. PR-Description MUSS explizit Bezug nehmen. |
| **Gate 3 (Merge)** | Lead Architect | Human-Review: Einhaltung des MECE-Prinzips und Audit der ANCHOR-Dokumentation. |

## Branch- und Commit-Standard

- **Branch-Naming:**
  - Feature: `feature/WP-1.1-compaction`
  - Bugfix:  `fix/WP-1.1-tombstones`

- **Commit-Format:** Conventional Commits
  - `<type>(<scope>): <Kurzbeschreibung (Imperativ)>`
  - `Body:` Detaillierte Begründung, warum die Änderung getätigt wurde, inklusive `ANCHOR-CLOSE: WP-1.1`.

// turbo-all
