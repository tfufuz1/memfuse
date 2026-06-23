# memfuse-store SDD Specification

## 1. Goal
`memfuse-store` is the central persistence engine using a Log-Structured Merge-Tree (LSM) architecture. It manages the Write-Ahead Log (WAL), MemTables, and SSTables with a focus on durability, integrity, and cryptographic sovereignty.

## 2. Invariants
- **Atomicity**: Writes to WAL must be atomic via group-commits.
- **Integrity**: WAL records are chained with HMAC-BLAKE3. SSTables use CRC32 per block.
- **Persistence**: `flush()` ensures data is synced to disk (`fsync`).
- **Sovereignty**: All disk I/O can be encrypted using `KeyManager` (AES-256-GCM-SIV).

## 3. Public API

### `LsmStorage` (StorageEngine)
| Method | Description | Logic |
|---|---|---|
| `get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>` | Read Path: Active MemTable -> Immutable MemTables -> SSTables (newest first). | Respects `TOMBSTONE_BIT`. |
| `put(&self, tx_id: TxId, key: &[u8], value: &[u8])` | Stages key-value pair in `TxBuffer`. | Enforces memory budget backpressure. |
| `commit(&self, tx_id: TxId)` | WAL Group Commit -> MemTable update -> Trigger Flush if needed. | Atomic WAL truncate on physical I/O failure. |
| `rollback_to_tx(&self, target_tx: TxId)` | Destructive rollback: Truncate WAL + Clear MemTable + Remove SSTables > target_tx. | Deterministic replay from truncated WAL. |

## 4. Storage Structure

### WAL (Write-Ahead Log)
- **Chaining**: Each entry contains HMAC of (previous_hmac + current_entry).
- **Binding**: UUID sidecar binds WAL to a specific engine instance.

### SSTable (Sorted String Table)
- **Blocks**: 4KB data blocks + Index block + Bloom Filter.
- **Trailer**: "MFSX" magic + metadata (tx_range, seq_range, offsets).
- **Compaction**: Size-Tiered (STCS) with Tombstone Garbage Collection (MVCC-safe).

## 5. Error Handling
| Variant | Logic |
|---|---|
| `ChecksumMismatch` | CRC32 or HMAC verification failed. |
| `StorageFull` | Memory budget or disk space exhausted. |
| `CryptoError` | Decryption failed (invalid key or corrupted IV). |
