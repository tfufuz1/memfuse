# memfuse-db SDD Specification

## 1. Goal
`memfuse-db` is the orchestration layer that provides a high-level Document/Collection API, integrating LSM storage, HNSW vector indexing, and inverted text indexing into a unified hybrid-search database.

## 2. Invariants
- **Consistency**: 2-Phase Commit (2PC) for atomic updates across storage and multiple indices.
- **Isolation**: Collections are logically isolated via key namespacing (`__col:{name}:\x00`).
- **Resilience**: Intent logging for transactions (`pending` -> `committed`/`aborted`) to enable forensic repair.
- **Determinism**: Markdown chunking and RRF fusion yield bit-identical results for identical inputs.

## 3. Public API

### `MemFuse` (Database Orchestrator)
| Method | Description | Invariants |
|---|---|---|
| `collection(&self, name: &str)` | Returns or initializes a namespaced collection. | - Creates `__col_idx` entry in storage. |
| `search(&self, query: &[f32], k: usize)` | Vector search on the default collection. | - Hardware-accelerated via `memfuse-index`. |
| `hybrid_search(&self, text, vector, k)` | BM25 + Vector search via RRF. | - Fuses results using default k=60. |

### `Collection` (Namespace)
| Method | Description | Logic |
|---|---|---|
| `insert(&self, id, embedding, metadata)` | Orchestrates entry into LSM, HNSW, and TextIndex. | - Uses `DbTransaction` for atomicity. |
| `repair(&self)` | Reconciles storage and index. | - Adds missing docs, removes ghost entries. |
| `rollback_to_tx(&self, tx_id)` | Coordinated rollback of all sub-engines. | - Synchronizes WAL and HNSW state. |

## 4. Transactional Logic (`DbTransaction`)
- **Intent Logging**: Writes `pending` marker before committing.
- **Compensating Transactions**: If the second engine (Index) fails after the first (Storage) succeeds, a durable retry-loop executes a reverse operation (Delete) to maintain consistency.
- **Strict Error Handling**: [INV-DB-3] requires explicit logging of rollback failures to prevent silent split-brains.

## 5. Components
- **MarkdownChunker**: Splits docs by heading hierarchy with breadcrumb metadata.
- **MetadataFilter**: Adaptive pre/post-filtering based on collection size (threshold: 1000 docs).
- **RRF Fusion**: Combines rankings mathematically to normalize disparate scoring distributions (BM25 vs Vector).
