# MemFuse — Codebase State Assessment
**Date:** 2026-05-08  
**Commit:** `378a52e3ca6d78ceaa64a6f9348f82c89ef5b334`

## Workspace Members (Actual vs. Spec)

| Crate | In Workspace | Spec Exists | Implementation State |
|-------|:---:|:---:|---|
| `memfuse-core` | ✅ | WP-0.0 | Stable (~280 LoC) — Error, types, TxBuffer, Snapshot, Traits |
| `memfuse-store` | ✅ | WP-1.1 | Stable (~1400 LoC) — LSM, MemTable, SSTable, WAL, Compaction, Checkpoint |
| `memfuse-index` | ✅ | WP-2.2 | Stable (~1300 LoC) — HNSW, CSR-Graph, SIMD Distance |
| `memfuse-db` | ✅ | WP-1.2 | Stable (~700 LoC) — Facade, Collections, SearchResult |
| `memfuse-text` | ✅ | WP-2.1 | **Skeleton** (~100 LoC) — BM25 + InvertedIndex stubs, no tests |
| `memfuse-runtime` | ✅ | — | **Skeleton** (~40 LoC) — SandboxConfig + WasmSandbox placeholder |
| `memfuse-orchestrator` | ✅ | — | **Skeleton** (~50 LoC) — StateGraph + AgentNode placeholder |
| `memfuse-py` | ✅ | WP-3.1 | **Partial** (~94 LoC) — PyO3 Agent/Collection bindings, no tests |

## Missing Crates (Planned but not yet created)

| Crate | Required by |
|---|---|
| `memfuse-checkpoint` | SAOS WP-5.1 (currently a module in `memfuse-store`) |
| `memfuse-sandbox` | SAOS WP-5.2 (currently `memfuse-runtime`) |
| `memfuse-saos-agent` | SAOS WP-5.3 (currently `memfuse-orchestrator`) |

## Build Status

- ✅ `cargo check --workspace` — **Passes** (0 errors, 0 warnings)
- ✅ 45 tests listed across workspace
- ⚠️ Tests in `memfuse-db` are `#[ignore]` pending WP-1.2 Collections implementation

## LoC Distribution

| Language | Files | Code Lines |
|---|---|---|
| Rust | 27 | 4,460 |
| Markdown (specs/docs) | 22 | 1,968 |
| TOML | 10 | 158 |
| Python | 4 | 207 |
| **Total** | 65 | ~5,150 |
