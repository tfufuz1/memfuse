# memfuse-core Specification

## Status
- Phase: COMPLETE
- Agent: AGENT:01
- Letzte Änderung: 2026-05-11

## Zweck
Bereitstellung der fundamentalen Datentypen, Traits und Fehlerbehandlungs-Mechanismen für den gesamten MemFuse-Workspace. Als "Sovereign Core" bildet dieses Crate die Basis der Abhängigkeitshierarchie.

## Funktionale Anforderungen
### FR-001: Eindeutige Identifikatoren (DocId, TxId, EntityId)
- **Beschreibung:** Typsichere Newtypes für u64-Identifier zur Vermeidung von Verwechslungen.
- **Input:** u64 Werte oder Strings (für DocId via blake3).
- **Output:** Transparente u64 Wrapper.
- **Status:** [x] Implementiert | [v] Getestet

### FR-002: Transaktionaler Schreibpuffer (TxBuffer)
- **Beschreibung:** Sharded Puffer zum Zwischenspeichern von Index-Operationen vor dem Commit.
- **Fehlerverhalten:** Erkennt verwaiste Transaktionen via Timeout und bereinigt diese.
- **Status:** [x] Implementiert | [v] Getestet

### FR-003: MVCC Snapshot-Isolation (SnapshotRegistry)
- **Beschreibung:** Verwaltung aktiver Read-Snapshots zur Verhinderung von verfrühtem Garbage Collection (LSM Tombstones).
- **Invariante:** `min_active_seqno` ist der kleinste Sequence-Number-Guard, der aktuell gehalten wird.
- **Status:** [x] Implementiert | [v] Getestet

### FR-004: Unified Error Handling (MemFuseError)
- **Beschreibung:** Zentrale Enum für alle Fehlerzustände im Workspace, optimiert für `?`-Propagation.
- **Status:** [x] Implementiert | [v] Getestet

## Nicht-funktionale Anforderungen
- **Zero-Panic:** Keine Verwendung von `.unwrap()` oder `.expect()` in Produktionscode.
- **Thread-Safety:** Alle Kern-Komponenten (`TxBuffer`, `SnapshotRegistry`) sind für hoch-parallele Zugriffe optimiert (Lock-Sharding, `parking_lot`).
- **Performance:** Minimale Overhead-Kosten für Identifier-Handling und Snapshot-Management.

## Abhängigkeiten
- Intern: Keine (Wurzel des Baums)
- Extern: `ahash`, `parking_lot`, `serde`, `thiserror`, `tokio`, `blake3`, `async-trait`.

## Schnittstellen / API-Vertrag
```rust
pub trait StorageEngine: Send + Sync {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;
    // ...
}

pub trait VectorIndex: Send + Sync {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;
    // ...
}

pub struct TxBuffer<T: Clone> {
    pub fn stage(&self, tx: TxId, op: IndexOp<T>);
    pub fn drain(&self, tx: TxId) -> Vec<IndexOp<T>>;
}
```

## Implementierungsnotizen
<!-- ANCHOR:IMPL-NOTES — Sharded Locking in TxBuffer reduziert Contention bei vielen gleichzeitigen Transaktionen. -->

## Offene Fragen
<!-- Keine aktuell. -->

## Änderungsprotokoll
| Datum | Agent | Änderung |
|-------|-------|----------|
| 2026-05-11 | AGENT:01 | Initiale Spezifikation basierend auf Implementierung erstellt. |

// ANCHOR:TODO:SPEC-100 — Spec für memfuse-core erstellt
// AGENT:01 DATE:2026-05-11 STATUS:DONE
