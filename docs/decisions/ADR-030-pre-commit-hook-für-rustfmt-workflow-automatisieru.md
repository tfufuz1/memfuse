# ADR-030: Pre-Commit-Hook für rustfmt & Workflow-Automatisierung


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Erstellung von `.githooks/pre-commit`, das automatisch `cargo fmt --all` (schreibend) vor jedem Commit ausführt und durch rustfmt formatierte Dateien automatisch per `git add -u` zum Commit hinzufügt.
    2. Ergänzung von `.jules/setup/environment_script.sh` um `git config core.hooksPath .githooks`, um den Hook in jeder Jules-VM-Session beim Setup automatisch zu aktivieren.
    3. Härtung von `.github/workflows/rust-ci.yml`, um bei Fehlschlag des Format-Checks in den CI-Logs klare, direkt ausführbare Handlungsanweisungen zur lokalen Korrektur auszugeben.
*   **Alternativen**:
    - Manuelles Einfordern von `cargo fmt` ohne automatischen Hook: Verworfen, da dies nachweislich zu wiederholten CI-Fehlschlägen bei automatisierten Agenten-Commits führte.
*   **Begründung**: Beseitigt wiederkehrende rustfmt-Zeilenumbruch- und Einrückungsdifferenzen in CI an der Quelle und stellt sicher, dass alle Commits konsistent formatiert sind.
*   **Konsequenzen**:
    - `.githooks/pre-commit` existiert und ist ausführbar.
    - `AGENTS.md §6` verweist auf den Ablauf und manuelle Bypasses.

---
