# Lock Audit in `memfuse-db`

| Datei | Typ | Verwendungszweck | Async-Kontext (ja/nein) |
|---|---|---|---|
| `crates/memfuse-db/src/lib.rs` | `tokio::sync::RwLock` | `MemFuse::collections`: Hashmap aller aktiven Collections | Ja |
| `crates/memfuse-db/src/lib.rs` | `tokio::sync::OnceCell` | `MemFuse::raft`: Lazily initialisierter Raft-Zustand | Ja |
| `crates/memfuse-db/src/lib.rs` | `tokio::sync::RwLock` | `MemFuse::embedder`: Globaler Text-Embedder Fallback | Ja |
| `crates/memfuse-db/src/collection/mod.rs` | `tokio::sync::Mutex` | `Collection::insert_lock`: Serialisiert Schreib- und Mutationsoperationen pro Collection | Ja |
| `crates/memfuse-db/src/collection/mod.rs` | `tokio::sync::RwLock` | `Collection::embedder`: Collection-spezifischer Text-Embedder | Ja |
| `crates/memfuse-db/src/transaction.rs` | `std::sync::Mutex` | `DbTransaction::staged_*`: In-Memory Staging-Puffer für Transaktionsänderungen | Nein (SYNC-ONLY: guard wird nie über `.await` gehalten) |
| `crates/memfuse-db/src/multistep.rs` | `std::sync::Mutex` | `MockQueryEngine::responses`: Test-Mock Antwort-Queue | Nein (nur in `#[cfg(test)]`) |
