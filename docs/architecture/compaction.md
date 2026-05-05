# Background Compaction & Garbage Collection (LSM-Tree)

## 1. Overview
The **MemFuse Storage Engine** uses a Log-Structured Merge (LSM) Tree to persist metadata and document payloads. Over time, as active MemTables are flushed to disk, the number of SSTable files grows. Without cleanup, reads degrade $O(N)$ and deleted data (tombstones) permanently consumes disk space.

This specification describes the **Size-Tiered Compaction Strategy (STCS)** implemented in `memfuse-store::compaction`.

## 2. Compaction Strategy & Architecture

MemFuse implements a **Background Size-Tiered Compaction Strategy**.

### 2.1 Design Goals
* **Write-Amplification:** Minimized by grouping similarly sized SSTables.
* **Non-Blocking:** Compaction is an I/O heavy process; it must never block foreground `insert`, `delete`, or `search` operations. 
* **Safe Garbage Collection:** Tombstones must only be purged when no active read-snapshot can possibly witness them.

### 2.2 System Components

* **`CompactionEngine`:** A background `tokio` task initialized by `LsmStorage::new()`.
* **Shared `sstables` Lock:** The LSM engine guards the list of SSTables using an `Arc<RwLock<Vec<Arc<SstableReader>>>>`. The compaction engine only needs a `read()` lock while selecting candidates and a `write()` lock for the atomic swap. 
* **`SnapshotRegistry`:** Tracks all active concurrent read queries. Provides `min_active_seqno()`, which acts as a watershed for garbage collection.

## 3. The Compaction Lifecycle

The compaction background loop (`CompactionEngine::run_loop()`) wakes up every `check_interval` (default: 30s) and executes the following steps:

### Step 1: Candidate Selection (Lock: READ)
The engine acquires a read lock on the SSTable vector. It groups SSTables into **Tiers** based on their file size using the `size_ratio` config (default: 4.0).
If a tier contains at least `min_sstables_per_tier` (default: 4) SSTables, these are selected as compaction candidates.
_The read lock is immediately dropped, allowing regular queries to continue evaluating these SSTables._

### Step 2: Multi-Way Merge (Lock: NONE)
A multi-way merge is performed over the selected `SstableReader` instances:
1. `iter()` yields all `(key, value, seq_no)` triples.
2. Entries are sorted lexicographically by key, and then by `seq_no` descending.
3. Duplicates are eliminated (the newest `seq_no` wins).

### Step 3: Tombstone Garbage Collection (Lock: NONE)
Before writing an entry to the new SSTable, the engine evaluates if it's a Tombstone (indicated by the 63rd bit in `seq_no`).
A tombstone is permanently discarded **if and only if**:
`raw_seq_no < SnapshotRegistry::min_active_seqno()`
This guarantees that no ongoing historical read query will suddenly see "deleted" data reappear because its shielding tombstone was purged too early.

### Step 4: Atomic Swap & Cleanup (Lock: WRITE)
1. The new SSTable is written to disk via `SstableBuilder` and an `SstableReader` is instantiated.
2. The engine acquires a **Write Lock** on the SSTable vector.
3. The old candidate SSTable Arcs are removed from the vector, and the new merged SSTable Arc is pushed. 
4. The write lock is dropped.
5. In a best-effort asynchronous pass, the physical `.sst` files corresponding to the old SSTables are deleted from disk using `tokio::fs::remove_file`.

## 4. Concurrency & Locking Model

| Phase | `sstables` Lock | Performance Impact |
| :--- | :--- | :--- |
| Candidate Scan | `READ` | Near zero. `Arc` clones only. |
| Multi-Way Merge | `NONE` | Heavy I/O/CPU, but zero lock contention. |
| SSTable Swap | `WRITE` | Ultra-fast (moving array elements). Blocks queries only for nanoseconds. |
| File Deletion | `NONE` | Asynchronous file unlinking. |

## 5. Configuration (`CompactionConfig`)

Developers can tune the compaction behavior depending on payload size and I/O limits:

* `min_sstables_per_tier` (default: 4): The threshold of equally sized SSTables needed to trigger a merge.
* `size_ratio` (default: 4.0): Defines the boundary of a "size tier" (e.g. 1MB files won't mix with 100MB files).
* `check_interval` (default: 30s): Background loop heartbeat duration.

## 6. Integration points (Developers Guide)
* **Storage Engine:** Ensure `scan()` and `get()` iterate the `RwLock` correctly.
* **Testing:** In tests, the `check_interval` should be disabled or made small to test deterministic merges, or manual compaction triggers should be exposed.
