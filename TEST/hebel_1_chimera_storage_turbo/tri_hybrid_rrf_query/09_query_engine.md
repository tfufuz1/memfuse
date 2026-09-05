# Architecture 09: Query Engine (Hybrid Execution)

> **Status:** ✅ PHASE 2 COMPLETE | **Crate:** `chimera-query`

This document specifies the design and implementation of the Query Engine. This component is the "brain" of ChimeraDB, responsible for planning, executing, and fusing the results from the various specialized indexes.

---

### 9.1 Three-Stage Execution Pipeline

The Query Engine follows a rigid 3-stage pipeline to maximize performance and ensure cross-modal consistency.

#### Stage 1: Pre-Filtering (Candidate Generation)
The goal is to reduce the search space as quickly as possible using high-throughput indices.
- **Metadata Index (FST/Bitmap):** Filters by JSON attributes. (SPEC-019)
- **Spatial Index (Octree):** Filters by 3D bounding boxes or radius. (SPEC-009)
- **Result:** A `RoaringTreemap` representing the intersection of all filter results.

#### Stage 2: Parallel Retrieval & Scoring
The candidate bitmap is passed to the scoring-heavy indices. These operate in parallel using `tokio` (SPEC-018).
- **Vector Index (HNSW):** Performs ANN search on the candidate set.
- **Sparse Index (BM25/WAND):** Performs keyword-based scoring with WAND. (SPEC-005, SPEC-014)
- **Graph Index (BFS):** Computes relationship-based candidates.
- **Result:** Multiple ranked lists with source-specific weights.

#### Stage 3: Weighted Reciprocal Rank Fusion (RRF)
Results from all scoring indices are merged into a single ranked list. (SPEC-022)
- **Algorithm:** Weighted RRF using `Score(d) = Σ w_i * (1 / (k + rank_i(d)))`.
- **Weights:** Controllable via `QueryPlanner` or `HybridQuery` request.
- **Performance:** Optimized using `AHashMap` for sub-millisecond fusion.

---

### 9.2 Implementation Details

#### 9.2.1 Query Planner (`planner.rs`)
The planner analyzes the `UnifiedQuery` and builds an execution graph. It identifies which indices can run in parallel and ensures that the `RoaringBitmap` from Stage 1 is correctly propagated to Stage 2.

#### 9.2.2 Fusion Engine (`fusion.rs`)
The fusion engine implements the **Weighted RRF** logic. It handles heterogeneous result sets (e.g., one index might return 100 results, while another returns only 5) and produces a globally ranked top-K list.

#### 9.2.3 Executor (`executor.rs`)
The executor manages the asynchronous execution of the plan using `tokio`. It handles timeouts and ensures that slow indices do not block the entire query pipeline (using a "best-effort" or "partial-result" strategy if configured).

---

### 9.2 Core Data Structures and APIs

The implementation MUST be located in the `chimera-query` crate.

#### 9.2.1 `struct UnifiedQuery`

This is the primary input to the query engine, representing a complex, multi-faceted query from a user.

```rust
// In: crates/chimera-api/src/lib.rs (or similar)

use chimera_core::types::Point3D;
use chimera_index_metadata::FilterExpr; // Assuming this is made public

/// Represents a query that can span multiple indexes.
/// All parts are optional.
pub struct UnifiedQuery {
    /// Spatial part of the query.
    pub spatial: Option<SpatialQuery>,
    /// Vector/semantic part of the query.
    pub vector: Option<VectorQuery>,
    /// Metadata filtering part of the query.
    pub metadata: Option<FilterExpr>,
    /// Graph traversal part of the query. (Future extension)
    // pub graph: Option<GraphQuery>,

    /// The final number of results to return.
    pub top_k: usize,
}

pub struct SpatialQuery {
    pub center: Point3D,
    pub radius: f32,
}

pub struct VectorQuery {
    pub vector: Vec<f32>,
    /// The number of vector results to consider for fusion.
    pub k: usize,
}
```

#### 9.2.2 `struct QueryEngine`

This is the main public interface for the query engine.

```rust
// In: crates/chimera-query/src/lib.rs

use chimera_core::types::DocId;
use anyhow::Result;
use std::sync::Arc;
// References to the actual index implementations
use chimera_index_spatial::SpatialIndex;
use chimera_index_vector::VectorIndex;
use chimera_index_metadata::MetadataIndex;

// The final result returned to the user
pub struct QueryResult {
    pub doc_id: DocId,
    pub score: f32,
    // pub document: Document, // The full document from storage
}

/// The public interface for the Query Engine.
pub struct QueryEngine {
    // The engine holds thread-safe references to all the indexes.
    spatial_index: Arc<SpatialIndex>,
    vector_index: Arc<VectorIndex>,
    metadata_index: Arc<MetadataIndex>,
    // graph_index: Arc<GraphIndex>,
}

impl QueryEngine {
    pub fn new(
        spatial_index: Arc<SpatialIndex>,
        vector_index: Arc<VectorIndex>,
        metadata_index: Arc<MetadataIndex>,
    ) -> Self;

    /// Executes a UnifiedQuery and returns the top-K results.
    /// This is the main entry point for all queries.
    pub async fn execute_query(&self, query: &UnifiedQuery) -> Result<Vec<QueryResult>>;
}
```

---

### 9.3 Implementation Details

#### 9.3.1 Query Planning and Execution

The `execute_query` method MUST follow a "filter-then-score" strategy:

1.  **Pre-Filtering Stage:**
    -   If `query.metadata` is `Some`, execute it against the `MetadataIndex` to get a `RoaringBitmap` of candidate `DocId`s.
    -   If `query.spatial` is `Some`, execute it against the `SpatialIndex` to get a `Vec<DocId>` of candidate `DocId`s. Convert this to a `RoaringBitmap`.
    -   **Combine Candidates:** If both filters were used, compute the **intersection** (`&`) of the two resulting bitmaps. If only one was used, use its result. If neither was used, the candidate set includes all documents (this should be used with caution).

2.  **Scoring Stage:**
    -   This stage operates on the `candidate_bitmap` from the pre-filtering stage.
    -   If `query.vector` is `Some`, execute a search on the `VectorIndex`, passing the `candidate_bitmap` to the `allowed_doc_ids` parameter. This will return a ranked list of `ScoredDoc`s.
    -   **(Future):** If other scoring indexes (like Graph) are present, execute them here, also constrained by the `candidate_bitmap`.

3.  **Fusion Stage:**
    -   If multiple ranked lists are produced in the scoring stage (e.g., from Vector and Graph), they MUST be fused using **Reciprocal Rank Fusion (RRF)**.
    -   **RRF Algorithm:** For each document, its RRF score is calculated as `Σ (1 / (k + rank_i))`, where `rank_i` is the document's rank in the result list from index `i`. A constant `k` (commonly 60) is used to diminish the influence of lower-ranked items.
    -   The final results are sorted by their RRF score in descending order.

4.  **Final Retrieval:**
    -   Take the `top_k` `DocId`s from the final ranked list.
    -   Retrieve the full documents from the `StorageEngine` (this will require a reference to the storage engine).
    -   Format and return the `QueryResult`s.

---

### 9.4 Testing Mandates

- **Unit Tests:** Test the query planner logic. For a given `UnifiedQuery`, assert that the correct sequence of index calls is planned. Test the RRF fusion logic with known ranked lists to ensure the merged result is correct.
- **Integration Tests:**
    -   Create an end-to-end test with all indexes populated.
    -   Execute a `UnifiedQuery` that combines metadata, spatial, and vector components.
    -   Verify that the result is correct. For example, the top result must satisfy the metadata filter, be within the spatial radius, AND be semantically similar to the query vector.
- **Performance Benchmarks:** A `criterion` benchmark MUST be created for `execute_query` with a complex, three-part query on a large dataset (1M+ documents). The target P99 latency for the entire query pipeline is **< 5ms**.
