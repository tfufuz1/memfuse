# memfuse-py SDD Specification

## 1. Goal
`memfuse-py` provides high-performance Python bindings for the MemFuse database using PyO3. It serves as the Layer 3 boundary, exposing Rust's transactional and vector-search capabilities to Python while maintaining "Sovereign Core" invariants.

## 2. Invariants
- **Thread-Safety**: Shared multi-threaded Tokio runtime managed via `OnceLock` to ensure stable async execution from Python.
- **Error Transparency**: Strict mapping of `MemFuseError` variants (Storage, Index, Crypto) to specific Python exception types.
- **Performance**: Zero-copy search results via FlatBuffers (`search_fb`, `hybrid_search_fb`) and efficient NumPy array handling.
- **Safety**: `unsafe` strictly forbidden in the bridge logic.

## 3. Python API (PyO3)

### `Db` (Facade)
| Method | Description | Implementation |
|---|---|---|
| `collection(name: str)` | Accesses or creates a isolated namespace. | - Namespaces HNSW and TextIndex. |
| `search(vector, k)` | K-NN vector search on default collection. | - Blocks on shared Tokio runtime. |
| `hybrid_search(text, vector, k)` | Combined BM25 + Vector search. | - Uses RRF (k=60) for score fusion. |

### `Collection` (Namespace)
| Method | Description |
|---|---|
| `insert(id, vector, metadata)` | Synchronous wrapper for async `insert`. |
| `relate(from, to, label)` | Creates document relationships in storage. |
| `stats()` | Returns combined Index and Storage metrics. |

## 4. Execution Model
- **Sync-to-Async Bridge**: Python calls are synchronous; they enter the shared Tokio runtime via `rt.block_on()` and `py.allow_threads()` to prevent GIL contention.
- **Shared Runtime**: Single `RUNTIME` instance for the entire process lifetime to avoid executor fragmentation.

## 5. Metadata Handling
- **Serde Bridge**: Uses `pythonize`/`depythonize` to convert between Python dicts and `serde_json::Value`.
- **Validation**: Metadata is validated and serialized to JSON before entering Layer 2.
