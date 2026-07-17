# Dependency-Audit-Protokoll

> Referenziert aus `AGENTS.md §8`. Pflicht bei jeder neuen Abhängigkeit.

## Checkliste (alle Punkte vor `Ask-first`-Freigabe)

- [ ] **Existiert das Paket?** `cargo search <name>` — nicht aus Trainingsdaten annehmen.
- [ ] **Version** aus `Cargo.lock` bestätigen — keine angenommene aktuelle Version nutzen.
- [ ] **Lizenz** MIT oder Apache-2.0 kompatibel? `cargo license` oder crates.io prüfen.
- [ ] **Tatsächliche Nutzung** mehr als eine triviale Funktion, die 5 Zeilen Std-Code wäre?
- [ ] **Maintenance** letzter Release < 12 Monate? Offene kritische Issues?
- [ ] **Advisories** `cargo audit` grün?
- [ ] **Slopsquatting-Check** für unbekannte/neue Pakete: crates.io direkt prüfen, Eigentümer verifizieren.

## Aktuelle Abhängigkeiten mit Risikoanmerkungen

| Crate | Version | Risiko | Anmerkung |
|---|---|---|---|
| `bincode` | 1.3.3 | MINOR | Veraltetes Format — v2 bricht Serde-Kompatibilität. Pinning intentional. |
| `uuid` | 1.23.1 | OK | Nur in `memfuse-store` für WAL-UUID. Korrekte Nutzung. |
| `aes-gcm-siv` | 0.11.1 | OK | Letzte v0.11.x — v0.12 existiert noch nicht. Lizenz: Apache/MIT. |
| `tokio-util` | 0.7.18 | MINOR | Nur für `TaskTracker` in `memfuse-store`. Evaluieren ob `tokio::task::JoinSet` reicht. |
| `flatbuffers` | 24.12.23 | MINOR | Nur in `memfuse-core` für IPC. Generierter Code (`memfuse_generated.rs`) enthält `unwrap()` — nicht manuell editieren. |
