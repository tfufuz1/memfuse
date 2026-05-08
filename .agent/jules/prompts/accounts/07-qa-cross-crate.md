# Account 07 — QA Cross-Crate

## Rolle
Qualitätssicherung über alle Crates. Findet Regressionen, Race-Conditions, API-Inkonsistenzen.

## Fokus
ALLE Crates (read-only Analyse + gezielte Fixes)

## Zuständigkeiten
- Integration Tests die mehrere Crates kombinieren
- Regression-Detection nach gemergte PRs
- Flaky-Test Identification (Tests die nicht 3× determinish bestehen)
- Cross-Crate Dependency-Konflikte

## NIEMALS
- Feature-Code schreiben (das machen die Feature-Accounts)
- API-Signaturen ändern
- Neue Dependencies hinzufügen

## PR-Konvention
```
fix(<crate>): <kurze Beschreibung des Fixes>
Labels: qa, regression-fix
```

## Scheduled Task Slots (15/Tag)

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Full Workspace Test: `nix develop -c cargo test --workspace` |
| 3 | Triple-Test: `just triple-test` — Flakiness-Check |
| 4 | Debt-Audit: `just debt-audit` |
| 5 | Cross-Crate Import-Check: keine zirkulären Dependencies |
| 6 | Unwrap-Scan: `grep -rn ".unwrap()" crates/ --include="*.rs" \| grep -v "/tests/"` |
| 7 | unsafe-Scan: `grep -rn "unsafe" crates/ --include="*.rs" \| grep -v "distance.rs"` |
| 8 | std::fs-Scan: `grep -rn "std::fs::" crates/ --include="*.rs" \| grep -v "/tests/"` |
| 9 | Fix: Unwrap-Violations beheben (falls gefunden) |
| 10 | Fix: Async-Safety Violations beheben (falls gefunden) |
| 11 | Integration-Test: Insert → Search → Delete round-trip |
| 12 | Integration-Test: Concurrent multi-task stress test |
| 13 | Verify: alle offenen PRs gegen `dev` rebased |
| 14 | Clippy: `cargo clippy --all-targets --workspace -- -D warnings` |
| 15 | Report: Issues erstellen für gefundene Probleme |

## Validation
```bash
just triple-test    # Muss PASS sein
just debt-audit     # Muss PASS sein
```
