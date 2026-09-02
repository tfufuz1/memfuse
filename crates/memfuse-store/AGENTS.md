# AGENTS.md — memfuse-store
> Layer 1 | LSM-Tree Storage Engine mit WAL, MemTable, SSTable, Compaction | ~13100 LOC

## 1. Zweck & Architekturrolle

Persistenzschicht des gesamten Systems. Implementiert einen vollständigen LSM-Tree
mit Write-Ahead-Log, MemTable (Skip-List), SSTable (Block-basiert mit Bloom-Filter,
CRC32, Compression), Background-Compaction und MVCC-Snapshot-Isolation.
Einziger Implementor des `StorageEngine` Traits aus `memfuse-core`.

**Datenpfad**: Client → `TxBuffer` → WAL → MemTable → SSTable → Compaction

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklaration, `#![deny(unsafe_code)]`, Datenpfad-Invariante |
| `lsm.rs` | `LsmStorage` — Orchestrator: öffnet DB, koordiniert WAL/MemTable/SSTable/Compaction, implementiert `StorageEngine` Trait |
| `wal.rs` | Write-Ahead-Log: Append-Only, HMAC-Chaining, CRC32 pro Entry, fsync-Pflicht |
| `memtable.rs` | In-Memory Skip-List mit Sequenznummern, Tombstone-Unterstützung |
| `sstable.rs` | On-Disk sortierte Segmente: Block-Kompression, Bloom-Filter, Index, CRC32 |
| `compaction.rs` | `CompactionEngine` — Hintergrund-Merge von SSTables (Tiered/Leveled) |
| `checkpoint.rs` | `pub(crate)` — Internes MVCC-Snapshot-Pinning, **NICHT** die öffentliche Checkpoint-API (die ist in `memfuse-checkpoint`) |
| `mmap.rs` | Memory-Mapped File Utilities für SSTable-Lesezugriff |
| `util.rs` | `pub(crate)` Hilfsfunktionen (Atomic Rename, load_or_create_integrity_key) |

## 3. Kritische Invarianten

### fsync Error Propagation (ABSOLUT)
JEDER `sync_all()` und `sync_data()` Aufruf **MUSS** Fehler mit `?` propagieren.
`let _ = dir.sync_all()` ist **VERBOTEN** — verschluckt WAL-Durability-Garantien.
CI Gate 3 erzwingt dies automatisch.

### last_committed_tx — Single Load Rule
In `get_at_seq()` und `scan_prefix_at()`: `last_committed_tx` **EINMAL** am Start
in eine lokale Variable laden. NICHT während der Iteration neu lesen — bricht
Snapshot Isolation unter konkurrierenden Writes.

### WAL HMAC Key Sourcing
**IMMER** `load_or_create_integrity_key()` verwenden. **NIEMALS** Schlüssel hartcodieren.
Der Key wird via HKDF aus dem Master Key abgeleitet (siehe `memfuse-crypto`).

### TOMBSTONE_BIT-Disziplin (ADR-041)
Bit 63 **strikt** maskieren (`seq & !TOMBSTONE_BIT`) vor allen `max_seq` Vergleichen.
Unmaskierte Tombstone-Sequenznummern führen zu Phantom-Sichtbarkeit gelöschter Einträge.

### Flush-before-Visible (ADR-043)
`last_committed_tx` **VOR** `sstables.push()` in `LsmStorage::flush` aktualisieren.
Umgekehrte Reihenfolge erzeugt ein Race-Window: SSTable ist sichtbar bevor
die Transaktion als committed markiert ist.

### I/O Pattern (ADR-012)
- `tokio::fs` für Metadaten/Lifecycle (WAL-Append, Flush, Directory-Create)
- `std::fs::File` **NUR** inside `spawn_blocking` für Block-Level Random-Access (SSTable pread)

### Atomic Rename Pattern (Writes)
`write_to_file()` MUSS tmp-file + atomic rename verwenden:
1. Schreibe nach `path.with_extension("tmp")`
2. `fsync` die Datei
3. `rename(tmp, final)` — atomar auf POSIX
4. `fsync` das Parent-Directory

### pub(crate) checkpoint Sichtbarkeit
`checkpoint.rs` ist `pub(crate)` — internes Snapshot-Pinning.
Die öffentliche benannte Checkpoint-API lebt in `memfuse-checkpoint` (ADR-011).
**NIEMALS** `memfuse-store::checkpoint` von außerhalb importieren.

## 4. Public API Quick-Reference

```rust
// === LsmStorage (lsm.rs) — Implementiert StorageEngine ===
pub struct LsmStorage { ... }
impl LsmStorage {
    pub async fn open(config: LsmConfig) -> Result<Self>;
    pub async fn repair_on_open(&self) -> Result<()>;
    // Alle StorageEngine Methoden (get, put, commit, flush, scan_prefix_at, ...)
}

pub struct LsmConfig {
    pub data_dir: PathBuf,
    pub memtable_size_limit: usize,     // Default: 4 MB
    pub compaction: CompactionConfig,
    pub encryption_passphrase: Option<String>,
}

// === CompactionEngine (compaction.rs) ===
pub struct CompactionEngine { ... }
pub struct CompactionConfig {
    pub max_sstable_count: usize,       // Trigger-Schwelle
    pub size_ratio: f64,                // Tiered Size Ratio
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — IO-Fehler verschluckt:
let _ = file.sync_all().await;
let _ = dir.sync_all();
// ✅ KORREKT:
file.sync_all().await.map_err(|e| MemFuseError::Storage(format!("fsync: {e}")))?;

// ❌ FALSCH — Deserialisierung mit Default-Fallback:
let entry = bincode::deserialize(&bytes).unwrap_or_default();
// ✅ KORREKT:
let entry = bincode::deserialize(&bytes)
    .map_err(|e| MemFuseError::ParseError(format!("WAL corrupt: {e}")))?;

// ❌ FALSCH — Unmaskierter Tombstone-Vergleich:
if entry.seq > max_seq { ... }  // Bit 63 kann gesetzt sein!
// ✅ KORREKT:
if (entry.seq & !TOMBSTONE_BIT) > max_seq { ... }

// ❌ FALSCH — std::fs direkt im async Kontext:
let file = std::fs::File::open(&path)?;
// ✅ KORREKT:
let file = tokio::task::spawn_blocking(move || std::fs::File::open(&path)).await??;

// ❌ FALSCH — checkpoint.rs von extern importieren:
use memfuse_store::checkpoint::SnapshotPinner;
// ✅ KORREKT — pub(crate), nutze stattdessen:
use memfuse_checkpoint::CheckpointGuard;
```

## 6. Concurrency & Lock-Hierarchie

| Lock | Typ | Scope | Reihenfolge |
|---|---|---|---|
| `write_lock` | `tokio::sync::Mutex` | WAL + MemTable Atomizität | 1. (äußerster) |
| `memtable` | `Arc<parking_lot::RwLock>` | MemTable Read/Write | 2. |
| `sstables` | `Arc<parking_lot::RwLock>` | SSTable-Liste | 3. |
| `snapshot_registry` | `parking_lot::Mutex` | Snapshot Pins | 4. (innerster) |

**Regel**: Niemals einen Lock in umgekehrter Reihenfolge akquirieren. Niemals einen `parking_lot` Guard über `.await`-Punkte halten.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0), `memfuse-crypto` (L1 Peer)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-index` (L1 Peer — kein Peer-Import!), `memfuse-text` (L1 Peer)
- **Implementiert**: `StorageEngine` Trait aus `memfuse-core`
- **Genutzt von**: `memfuse-db`, `memfuse-agent`, `memfuse-router` (als `Arc<dyn StorageEngine>`)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-012 | I/O Pattern (tokio::fs vs. spawn_blocking) |
| ADR-041 | TOMBSTONE_BIT-Maskierung |
| ADR-043 | Flush-before-Visible Race Fix |
| `rules/async-io.md` | spawn_blocking Pattern für SSTable Reads |
| `rules/wal_crypto.md` | HMAC Chaining & Key Derivation |
| `rules/error-handling.md` | MemFuseError Variant Policy |
