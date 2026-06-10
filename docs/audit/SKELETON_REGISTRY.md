# SKELETON_REGISTRY

The following constitutes the centralized registry of all identified function skeletons, `todo!()` blocks, and unimplemented methods across the MemFuse codebase. 

## 1. Storage & Persistence (`memfuse-store`)
- **`lsm.rs:596, 601`**: `pin_checkpoint` and `unpin_checkpoint` currently execute `Ok(())` as placeholders. (Breaks durable snapshots).
- **`lsm.rs:264`**: `TODO:COMP-001` - CompactionEngine `run_loop` requires completion.

## 2. Vector Index (`memfuse-index`)
- **`diskann.rs:609`**: `TODO(WP-8.2)` - Migrate synchronous `Mmap`/`File` operations in tokio threads to async I/O.
- **`diskann.rs:710-713`**: `commit` and `rollback` return `Ok(())` since DiskANN is currently read-only, but should be correctly designed.

## 3. Text Engineering (`memfuse-text`)
- **`inverted.rs:137, 356`**: `TODO(FIND-TXT-002)` - OpenTelemetry Tracing missing on hot paths.

## 4. Unifying Database Engine (`memfuse-db`)
- **`lib.rs:294`**: `TODO:COL-001` - Missing core persistence and transaction isolation in `collection()`.
- **`lib.rs:358`**: `TODO:COL-002` - `list_collections` requires metadata store integration.
- **`lib.rs:390`**: `TODO:COL-003` - Collection deletion needs to cascade into LSM and HNSW indices.
- **`lib.rs:529`**: `TODO:SEARCH-001` - `hybrid_search(text, vector, k)` delegate implementation needed.
- **`context.rs:126`**: `TODO(WP-6.3)` - Validate chunk metadata for `geo_region` field match.
- **`collection.rs:213, 741`**: `TODO(FIND-DB-002)` - OpenTelemetry Tracing missing.

## 5. Security Sandbox (`memfuse-sandbox`)
- **`host_functions.rs:38`**: `TODO(FIND-SBX-001)` - Skeleton implementations for secure host functions (WP-6).
- **`host_functions.rs:42`**: `TODO(WP-6)` - Actual orchestrator L2 loopback mapping needed.
- **`airgap.rs:89, 92`**: `TODO(FIND-SBX-002)` and `TODO(WP-6.6)` - `AirGapVerifier` is heavily mocked and missing validation logic.

## Summary
A total of **16** explicit skeletons and TODO implementations were documented. Priority must be given to `memfuse-db` orchestration gaps and `memfuse-store` snapshot placeholders.
