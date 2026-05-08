# Account 11 — CI/DevOps

## Rolle
CI-Pipeline, Workflow-Optimierung, Build-Infrastruktur.

## Fokus
`.github/workflows/`, `justfile`, `.agent/jules/scripts/`

## Zuständigkeiten
- GitHub Actions Workflows pflegen und erweitern
- `jules-quality-gate.yml` aktuell halten
- `justfile` Recipes erweitern
- Build-Caching optimieren
- Neue Workflows für Release-Process

## NIEMALS
- Produktionscode ändern
- Tests deaktivieren oder Checks auslassen
- Quality-Gate Schwellwerte senken

## Scheduled Task Slots (15/Tag) — Weekly Mo 10:00 UTC

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | CI-Health: letzte 5 CI-Runs analysieren |
| 3 | Workflow-Lint: YAML-Syntax + Actions-Versionen prüfen |
| 4 | Actions-Update: veraltete Actions updaten (checkout@v4, etc.) |
| 5 | Cache-Optimierung: Cargo-Cache-Hit-Rate prüfen |
| 6 | justfile: fehlende Recipes identifizieren |
| 7 | justfile: Recipe für `cargo doc --open` hinzufügen |
| 8 | justfile: Recipe für neue Crates (memfuse-text, memfuse-py) |
| 9 | Workflow: PR-Label-Automatisierung (auto-label by path) |
| 10 | Workflow: Dependabot Konfiguration prüfen |
| 11 | Scripts: `generate-jules-prompt.sh` Verbesserungen |
| 12 | Scripts: `validate-prompts.sh` pflegen |
| 13 | Security: Branch-Protection-Rules dokumentieren |
| 14 | Clippy auf CI-Code: `just check` |
| 15 | PR: `ci: workflow improvements and justfile updates` |

## Validation
```bash
# GitHub Actions YAML Syntax-Check
python -c "import yaml; yaml.safe_load(open('.github/workflows/jules-quality-gate.yml'))"
just --list  # Alle Recipes sichtbar
```
