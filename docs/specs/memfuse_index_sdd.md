# memfuse-index SDD Specification

## 1. Goal
The `memfuse-index` crate provides a high-performance vector similarity search engine based on Hierarchical Navigable Small Worlds (HNSW). It is designed to be 100% Safe-Rust, zero-panic, and hardware-accelerated via SIMD.

## 2. Invariants
- **Zero-Panic**: No `unwrap()`, `expect()`, or out-of-bounds indexing in production paths.
- **Determinism**: Identical inputs must yield identical results regardless of threading or SIMD paths.
- **Precision**: SQ8 quantization must maintain documented error bounds.
- **Safety**: All `unsafe` blocks for SIMD are documented with a `SAFETY` comment proving memory safety and valid alignment.

## 3. Public API

### `HnswIndex` (StorageEngine)
| Method | Description | Invariants |
|---|---|---|
| `search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>>` | Performs greedy search across HNSW layers. | - Input length must match `dimension`. <br/> - `k` is capped by HNSW search limits. |
| `put(&self, tx_id: TxId, id: DocId, vector: &[f32])` | Stages a vector insert into `TxBuffer`. | - Vectors are normalized if configured. |
| `commit(&self, tx_id: TxId)` | Finalizes insert by updating the local HNSW graph layers. | - Atomic layer updates. |

## 4. Hardware Optimization (SIMD)
- **Distance Metrics**: L2 (Euclidean), Cosine, Dot Product.
- **Feature Detection**: Runtime detection for `AVX-512`, `AVX2`, and `NEON`.
- **Fallback**: Scalable scalar implementation for unsupported architectures.

## 5. Error Handling
| Variant | Logic |
|---|---|
| `DimensionMismatch` | Query vector length != index definition. |
| `QuantizationError` | Value range exceeds `SQ8` capacity without normalization. |
| `Corruption` | Graph link points to non-existent node during traversal. |
