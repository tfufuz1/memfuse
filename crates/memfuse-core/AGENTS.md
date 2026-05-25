# memfuse-core — Agent Context

## 🎯 Crate Purpose
`memfuse-core` beinhaltet alle globalen Typen, Traits (`MemBank`), den `TxBuffer` und Error-Definitionen (`MemFuseError`), die im gesamten Projekt von `index`, `store` und `db` verwendet werden.

## 🛡️ Critical Invariants
- **NUR reine Typen und Traits**: Implementiere hier **keine** Laufzeitlogik für Storage oder Indexing.
- **Errors propagieren**: Jedes Versagen aus Fremdbibliotheken MUSS in `MemFuseError` übersetzt und zurückgegeben werden. Keine `.unwrap()` oder `panic!()` Aufrufe! // unwrap
- **Abhängigkeiten**: Diese Crate darf keine Abhängigkeiten zu den anderen memfuse-Crates haben (Crate-Azyklizität).

## 🔄 TDD Workflow Requirement
Änderungen an Typen und Traits müssen durch TDD gesichert sein. Verwendest du eine neue Serde-Derivation? Schreibe einen `#[tokio::test]`, der Serialisierung/Deserialisierung sicher testet, BEVOR du die Logik implementierst.
