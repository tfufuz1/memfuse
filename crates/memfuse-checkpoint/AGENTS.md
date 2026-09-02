# AGENTS.md — memfuse-checkpoint
> Layer 1 | Snapshot-Pinning, RAII-Transaktions-Guards, Time-Travel | ~1000 LOC

## 1. Zweck & Architekturrolle

Koordiniert konsistente Snapshots über alle Storage-Komponenten hinweg (LSM, Vector, Graph).
Bietet `PersistentCheckpointStore` zur Ablage von benannten Checkpoints.
Herzstück ist der `CheckpointGuard`: Er erzwingt per RAII-Semantik, dass Transaktionen
entweder explizit committet werden oder andernfalls (bei Panic/Drop) ein Rollback auslösen.
Implementiert den `CheckpointCoordinator` Trait (bzw. greift auf diesen zu).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, `CheckpointGuard`, `PersistentCheckpointStore`, `StateCheckpoint` |

*(Hinweis: Diese Crate ist klein und vollständig in `lib.rs` zusammengefasst, um die Kopplung zwischen Guard-Lifecycle und Persistenz eng zu halten).*

## 3. Kritische Invarianten

### RAII-Semantik des CheckpointGuard
Ein `CheckpointGuard` **MUSS** immer konsumiert werden.
- `guard.commit()` überführt die Transaktion in einen dauerhaften Checkpoint.
- `guard.rollback()` (oder `drop(guard)`) verwirft uncommittete Änderungen der `TxId`.
- **Wichtig:** Da `Drop` in async-Rust nicht asynchron sein kann, registriert
der synchrone Drop-Handler des Guards die Transaktion als "orphaned".
Ein asynchroner Reaper (`recover_orphaned_checkpoints`) muss diese später aufräumen.

### TxId-Zuweisung für System-Checkpoints
Manuell erstellte named Checkpoints (nicht reguläre Agent-Steps) nutzen TxIds 
aus dem internen Bereich `TxId::INTERNAL_BASE` aufwärts.
Dies verhindert Konflikte mit regulären Dokument/Kanten-Einfügungen.

### Snapshot Pinning
`PersistentCheckpointStore` ruft `storage.pin_checkpoint(seq_no)` auf.
Gepinnte Checkpoints verhindern, dass der LSM-Compactor (Layer 1) Versionen 
löscht, die für `rollback_to_tx` noch benötigt werden.
Wenn ein Checkpoint gelöscht wird, MUSS er unpinned werden.

## 4. Public API Quick-Reference

```rust
// === Checkpoint Lifecycle ===
pub struct CheckpointGuard<S: StorageEngine> { ... }
impl<S> CheckpointGuard<S> {
    pub async fn for_agent_step(storage: Arc<S>, tx: TxId) -> Result<Self>;
    pub fn commit(self) -> Result<StateCheckpoint>;
    pub async fn rollback(self) -> Result<()>;
}

// === Persistent Checkpoint Store ===
pub struct PersistentCheckpointStore<S: StorageEngine> { ... }
impl<S> PersistentCheckpointStore<S> {
    pub async fn open(storage: Arc<S>, namespace: impl Into<String>) -> Result<Self>;
    pub async fn create_checkpoint(&self, name: &str, ...) -> Result<CheckpointMeta>;
    pub async fn restore_checkpoint(&self, name: &str) -> Result<CheckpointMeta>;
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()>;
}

// === Orphan Management ===
pub async fn await_pending_rollbacks();
pub fn orphaned_checkpoint_count() -> usize;
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Guard unabsichtlich droppen vor Await (Blockiert LSM-Locks!):
let guard = store.create_guard(tx)?;
let result = some_long_async_task().await; // Guard lebt hier noch, Drop passiert danach!
// ✅ KORREKT — Guard explizit handhaben:
let result = some_long_async_task().await;
if result.is_ok() { guard.commit()?; } else { guard.rollback().await?; }

// ❌ FALSCH — System-Checkpoints mit aktueller Systemzeit als TxId:
let tx = TxId(SystemTime::now()...);
// ✅ KORREKT — `allocate_tx` des Stores nutzen:
let tx = store.allocate_tx().await?;

// ❌ FALSCH — public struct Felder modifizieren:
// ✅ KORREKT — `StateCheckpoint` Felder sind privat oder read-only nach Erstellung.
```

## 6. Concurrency & Lock-Hierarchie

`CheckpointGuard` hält keine blockierenden OS-Locks (Mutex/RwLock) über asynchrone Grenzen,
aber er repräsentiert eine offene, uncommittete Transaktion im `TxBuffer` der `StorageEngine`.
Lange offene Guards verbrauchen Memory (weil Writes nicht in den LSM fließen) und können
andere Lese-Transaktionen behindern (weil das MVCC-Watermark nicht voranschreitet).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-store` (L1 Peer), `memfuse-db` (L2)
- **Genutzt von**: `memfuse-db` (MultiStepEngine), `memfuse-agent` (für Agenten-Steps)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-011 | RAII-basierte Checkpoint Guards (Orphan-Reaping bei Panic) |
| `rules/async_drop.md` | Hintergrund-Reaping von synchronen Drops (Orphaned Checkpoints) |
| `COMMON_LLM_ERRORS.md` | Fehler-Klasse 11: Lock-Guard über `.await` halten |
