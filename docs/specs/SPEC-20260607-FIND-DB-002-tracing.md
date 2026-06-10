# SPEC: 20260607-FIND-DB-002-Tracing-Injection

## 🎯 CONTEXT
Implementierung von systematischer Tracing-Instrumentierung in `memfuse-db`, um Latenz- und Fehleranalyse an öffentlichen Einstiegspunkten zu ermöglichen (FIND-DB-002).

## 🛡️ INVARIANTS
- **[INV-01]**: Alle primären öffentlichen Facade-Methoden (`open`, `open_with_config`, `insert`, `insert_many`, `search_text`, `hybrid_search`) MÜSSEN mit `#[tracing::instrument(skip(self))]` annotiert sein.
- **[INV-02]**: Die Instrumentierung darf keine funktionalen Änderungen am Code-Verhalten oder der Fehlerbehandlung einführen.
- **[INV-03]**: `skip(self)` muss verwendet werden, um unnötiges Tracing des `MemFuse`-Struct-Zustands zu vermeiden (der oft groß oder nicht serialisierbar ist).

## 📍 TARGET
- **Crate**: `memfuse-db`
- **Module/File**: `src/lib.rs`

## 🏗️ PROPOSED CHANGE
Hinzufügen von `#[tracing::instrument(skip(self))]` (bzw. `skip_all` für statische Methoden oder wo passend) zu den Ziel-Methoden.

## ✅ TDD GATE
- **Red Test**: Ein Integrationstest in `memfuse-db`, der `tracing-subscriber` nutzt, um zu verifizieren, dass Aufrufe von `open` oder `insert` Spans erzeugen.
- **Verification**: `cargo test -p memfuse-db`
