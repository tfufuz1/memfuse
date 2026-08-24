# memfuse-db — Crate-Level Agent Rules

## Critical Invariants

### TxId Generation
ALWAYS use `collection.allocate_tx()` to generate transaction IDs.
NEVER generate TxIds externally (e.g., `SystemTime::now().as_nanos()`).
The allocator guarantees monotonicity and uniqueness across concurrent writers.

### relate() — Graph Edge Requirement
`relate()` MUST call `self.graph_index.add_edge()` in addition to
writing the relation metadata to the LSM store. Omitting the graph
edge breaks graph traversal queries while the relation appears to exist
in metadata lookups.

### repair_on_open() — Error Handling
`repair_on_open()` MUST return `Err` if the repair itself fails.
NEVER silently continue with a partially repaired state — this leads
to data corruption that surfaces later in unrelated operations.

### check_doc_id_collision() — Lock Scope
Call `check_doc_id_collision()` ONLY within the `insert_lock` scope.
Calling it outside the lock creates a TOCTOU race where a concurrent
insert can create the same DocId between check and actual insertion.

### 4-Signal Fusion
`hybrid_search()` fuses Vector + BM25 + Graph + Metadata via RRF.
When modifying search, ensure all 4 signals contribute to the final ranking.
