# Task: WP-0.0 Dependency Audit & Cleanup

## Kontext
Dies ist die Initialisierungsphase. Wir müssen sicherstellen, dass die Codebasis den Sovereign Core Standards entspricht, bevor wir Features bauen.

## Aufgaben
1. **Dependency Check**:
   - Führe `cargo audit` aus und melde kritische Sicherheitslücken.
   - Führe `cargo machete` aus und entferne ungenutzte Crates aus allen `Cargo.toml`.
2. **Standard-Migration**:
   - Ersetze `once_cell` durch `std::sync::OnceLock`.
3. **Quality Gate**:
   - Stelle sicher, dass `cargo check` in allen Crates grün ist.

## Erwartetes Ergebnis
Ein sauberer Workspace ohne ungenutzte Abhängigkeiten und mit modernisierten Rust-Primitiven.
PR-Titel: `chore(workspace): WP-0.0 dependency audit and cleanup`
