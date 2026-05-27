# memfuse-db — Agent Context

## 🎯 Crate Purpose
`memfuse-db` orchestriert die restlichen drei Crates und bildet die API (`Facade`), mit der User (AI-Agenten) interagieren. Hier werden Collections, Multi-Tenancy (Namespaces), Hybrid-Search-Fusion (RRF / Score Aggregation) und Transaktionen gemanagt.

## 🛡️ Critical Invariants
- **[INV-API-1] Facade Error Handling**: Die Facade fängt Panics *niemals* ab, da es keine Runtimespanics geben darf. Sie übersetzt Engine-Errors in für Endbenutzer extrem gut lesbare Fehler-Strings.
- **[INV-SYNC-1] Atomicity**: Eine Document Insertion in `memfuse-db` muss **zuerst** in `memfuse-store` (WAL safe) geschrieben werden, bevor der Verweis im HNSW `memfuse-index` geupdated wird.
- **[INV-NS-1] Namespace Isolation**: Wenn Collections als Feature implementiert werden, MUSS jede Abfrage den Namespace Prefix inkludieren, ansonsten bricht die Kapselung ein.

## 🔄 TDD Workflow Requirement
Da dies die Orchestration Layer ist, werden hier Integrations-Tests geschrieben.
1. Jeder Feature-TDD-Zyklus baut hier auf API-Ebene eine `MemFuse::new()` Instanz auf.
2. Setze die Calls asynchron an.
3. Assert das End-Resultat (z.B. `let results = db.search(vec![1.0], 5).await?; assert_eq!(results.len(), 5)`).
