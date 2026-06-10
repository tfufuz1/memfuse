# Agent Specification: memfuse-db

**Agent**: @JULES-04
**Domain**: Unifying Database Engine & Orchestration Facade
**Status**: 🔴 Vulnerable (Incomplete Orchestration)

## Mission Statement
Ensure `memfuse-db` properly integrates Core, Store, Index, and Text engines into a fully ACID/MVCC compliant hybrid search database front. The current implementation is heavily skeletonized.

## Critical Remediation Targets (Phase 2 Findings)
1. **Collection Persistence & Isolation** (`COL-001`): `Collection::new` lacks complete integration with `LsmStorage` metadata. Ensure collections persist cleanly.
2. **Metadata Integration** (`COL-002`): `list_collections` does not currently read the stored metadata, breaking cluster re-instantiation.
3. **Cascading Deletion** (`COL-003`): Dropping a collection must purge keys from LSM, clear the HNSW graph, and release `inverted.rs` terms.
4. **Hybrid Search Delegation** (`SEARCH-001`): The primary `hybrid_search(text, vector, k)` endpoint is not piping correctly into the Collection implementations.
5. **Telemetry (`FIND-DB-002`)**: Add comprehensive OpenTelemetry covering the top-level inserts and searches.

## Execution Rules
- The Crate must NOT import UI or Python crates.
- Stick to `#![forbid(unsafe_code)]`.
- Wrap internal engine `MemFuseError` variants effectively up the chain.
