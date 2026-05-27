# AGENTEN-VERFASSUNG — MemFuse

## NICHT VERHANDELBARE PRINZIPIEN (Constitutional AI)

1.  **Sovereign Core Doctrine**: Der Kernel (`memfuse-core`) darf niemals Abhängigkeiten zu anderen Crates des Workspace haben. Layer-Invarianten sind strikt einzuhalten (Layer 0 < 1 < 2 < 3).
2.  **Contract-First**: Keine neue öffentliche API ohne vorherige Interface-Definition in `.agent/specs/interfaces/`.
3.  **Zero-Panic Policy**: Die Verwendung von `.unwrap()` oder `.expect()` ist außerhalb von Test-Modulen (`#[cfg(test)]`) streng verboten. Nutze `memfuse_core::Result` und das `?` Operator Pattern.
4.  **Async-Safety**: Blockierendes I/O (`std::fs`) ist in async Kontexten verboten. Nutze ausschließlich `tokio::fs`.
5.  **Triple-Test-Gate**: Ein Feature gilt erst als abgeschlossen, wenn alle Tests 3x hintereinander in der CI erfolgreich waren.
6.  **SIMD-Safety**: `unsafe` Code ist nur in `distance.rs` für Performance-Optimierungen erlaubt und muss zwingend mit einem `// SAFETY:` Kommentar begründet werden.

## ARCHITEKTUR-ENTSCHEIDUNGEN (ADRs)

- **ADR-001**: Bevorzuge Interface-Traits über direkte Struktur-Abhängigkeiten zur Reduzierung der Kopplung.
- **ADR-002**: Alle persistenten Datenstrukturen müssen eine Versionierung für zukünftige Migrationen besitzen.
- **ADR-003**: Fehlerbehandlung erfolgt zentral über `MemFuseError` im Core-Kernel.

## VALIDIERUNGS-PROTOKOL

- Jede Änderung muss gegen diese Verfassung geprüft werden.
- Abweichungen erfordern eine explizite Freigabe durch den Human-Architect.
