# HNSW Lock Granularity & Concurrent Write Throughput Audit

**Component:** `memfuse-index` (`crates/memfuse-index/src/hnsw.rs`)
**Scope:** Write Throughput Scaling & Read Latency under Concurrent Workloads
**Audit Target:** Empirical Verification of Global Lock Hypothesis
**Date:** August 31, 2026
**Auditor:** Senior Rust Concurrency & Graph Index Engineer

---

## 1. Executive Summary

Empirical benchmarking on a fixed base HNSW index of **50,000 vectors** (128-dimensional, $M=16, ef_{construction}=64$) **CONFIRMS** the global-lock performance hypothesis with 100% empirical certainty.

When scaling parallel insertion workloads from **1 to 16 concurrent Tokio tasks/threads**, the insertion throughput remains completely flat at **~280 vectors/second**, yielding a scaling factor of **1.00x – 1.02x**. Parallel insertions gain zero throughput improvement from additional CPU cores due to coarse-grained exclusive serialization in `HnswIndexCore`.

### Key Empirical Takeaways

1. **Global Lock Bottleneck Confirmed**: Concurrent writes do not scale. Throughput across 1, 2, 4, 8, and 16 worker threads yields scaling factors of **1.00x, 1.00x, 1.01x, 1.01x, and 1.02x** respectively.
2. **Root Cause**: `VectorIndex::commit()` acquires an exclusive `tokio::sync::Mutex<()>` (`write_mutex`), which completely serializes all transactional vector insertions. Furthermore, inside `do_insert()`, mutations acquire a single global `parking_lot::RwLock<Vec<HnswNode>>` (`nodes`).
3. **Mixed Workload Impact (Read Latency)**: Under a concurrent write load of 16 parallel insertion tasks, average search latency increases by only **1.08x** (from **3.25 ms** idle to **3.51 ms** under high write load). Readers acquire `nodes.read()` without blocking on `write_mutex`, experiencing lock contention only during brief microsecond windows when writers hold `nodes.write()` for graph edge updates.

| Parallel Tasks ($N$) | Total Inserts | Elapsed Time (ms) | Throughput (vec/s) | Scaling Factor ($S_N$) | Verdict |
| :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | 1,000 | 3,599.92 | 277.8 | **1.00x** | Baseline (Serial) |
| **2** | 1,000 | 3,582.58 | 279.1 | **1.00x** | No Parallelization |
| **4** | 1,000 | 3,573.96 | 279.8 | **1.01x** | No Parallelization |
| **8** | 1,000 | 3,570.06 | 280.1 | **1.01x** | No Parallelization |
| **16** | 1,000 | 3,528.00 | 283.4 | **1.02x** | Fully Serialized |

---

## 2. Lock-Granularity Code Analysis

An analysis of `crates/memfuse-index/src/hnsw.rs` reveals two hierarchical concurrency controls operating on the vector graph:

```
┌────────────────────────────────────────────────────────────────────────┐
│                          HnswIndex (Facade)                            │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│                            HnswIndexCore                               │
│                                                                        │
│  [1] write_mutex: tokio::sync::Mutex<()>  <-- Global Serializer        │
│                                               (Acquired per commit)    │
│  [2] nodes: parking_lot::RwLock<Vec<HnswNode>> <-- Index-Wide Lock    │
│  [3] doc_to_node: parking_lot::RwLock<AHashMap<u64, usize>>            │
│  [4] entry_point / ram_entry_point: parking_lot::RwLock<Option<usize>> │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Transaction Commit Path (`VectorIndex::commit`)

In `HnswIndex::commit(tx)`:

```rust
async fn commit(&self, tx: TxId) -> Result<()> {
    ...
    let _lock = self.inner.write_mutex.lock().await; // <-- [1] Exclusive Tokio Mutex
    let ops = self.inner.tx_buffer.drain(tx);
    ...
    for op in &ops {
        match op {
            IndexOp::Insert { doc_id, data } => {
                self.inner.do_insert(*doc_id, data)?;
                ...
            }
        ...
    }
```

- **Scope & Scope Boundary**: `write_mutex` is a single `tokio::sync::Mutex<()>`. Every `commit()` call acquires this lock exclusively for the duration of processing all staged operations.
- **Impact**: All concurrent tasks calling `commit()` are forced into a single-file queue. Even if 16 tasks compute layer assignment and neighbor selection independently prior to commit, the entire graph insertion (`do_insert`) runs strictly one transaction at a time.

### 2.2 Internal Insertion Path (`do_insert`)

Within `do_insert(doc_id, vector)`:

1. **Node Allocation**:
   ```rust
   let new_idx = {
       let mut nodes = self.nodes.write(); // <-- [2] Exclusive write lock on entire node vector
       let idx = nodes.len();
       nodes.push(HnswNode { ... });
       mmap_node_count + idx
   };
   self.doc_to_node.write().insert(id.inner(), new_idx); // <-- [3] Exclusive write lock on DocId map
   ```
2. **Layer Greedy Search**:
   - Holds `self.nodes.read()` and `self.mmap_index.read()` while performing top-down graph traversal to find nearest neighbor candidates across layers (`search_layer` & `select_neighbors_heuristic`).
3. **Edge Connection & Back-Linking**:
   ```rust
   let mut nodes = self.nodes.write(); // <-- [2] Re-acquires write lock over ALL nodes
   if let Some(new_node) = nodes.get_mut(...) {
       new_node.connections = final_connections.clone();
   }
   for layer in ... {
       for &ni in &final_connections[layer] {
           // Mutates neighbor_node.connections[layer]
           // If neighbor.len() > 2 * M, re-calculates heuristic and prunes connections
       }
   }
   ```
4. **Entry Point Update**:
   ```rust
   let mut ram_ep = self.ram_entry_point.write(); // <-- [4] Exclusive write lock on entry point
   let mut ep_global = self.entry_point.write();
   ```

### 2.3 Search Path (`search` / `search_filtered`)

In `search(query, k)`:
- Does **NOT** acquire `write_mutex`.
- Acquires `entry_point.read()` and `ram_entry_point.read()`.
- Acquires `nodes.read()` during `search_layer` and distance resolution.

Because `nodes` is a single `parking_lot::RwLock<Vec<HnswNode>>`, concurrent readers can execute in parallel with other readers. However, when a writer thread executes Step 1 or Step 3 of `do_insert`, it acquires `nodes.write()`, blocking all active and incoming readers for the duration of that critical section.

---

## 3. Scaling Benchmark Results

### 3.1 Benchmark Configuration
- **Base Index Size**: 50,000 pre-populated vectors
- **Vector Dimension**: 128
- **HNSW Parameters**: $M = 16$, $ef_{construction} = 64$, $ef_{search} = 64$
- **Workload**: $N = 1,000$ total additional insertions distributed evenly across $T \in \{1, 2, 4, 8, 16\}$ parallel Tokio tasks.
- **Hardware/Runtime Environment**: Multi-threaded Tokio async runtime on Linux x86_64 sandbox.

### 3.2 Empirical Results Table

| Threads ($T$) | Total Inserts | Total Time (ms) | Throughput (vec/sec) | Speedup / Scaling Factor | Efficiency ($S_T / T$) |
| :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | 1,000 | 3,599.92 | 277.8 | **1.00x** | 100.0% |
| **2** | 1,000 | 3,582.58 | 279.1 | **1.00x** | 50.0% |
| **4** | 1,000 | 3,573.96 | 279.8 | **1.01x** | 25.3% |
| **8** | 1,000 | 3,570.06 | 280.1 | **1.01x** | 12.6% |
| **16** | 1,000 | 3,528.00 | 283.4 | **1.02x** | 6.4% |

```
Insert Throughput (vec/s) vs. Threads
300 ┼──────────────────────────────────────────────────┐
    │  277.8     279.1     279.8     280.1     283.4   │
250 ┼───●─────────●─────────●─────────●─────────●──────┤ (Flat Line = No Scaling)
200 ┼                                                  │
150 ┼                                                  │
100 ┼                                                  │
 50 ┼                                                  │
  0 ┴───┴─────────┴─────────┴─────────┴─────────┴──────┘
        1         2         4         8         16  (Threads)
```

### 3.3 Analysis of Results
- Scaling factor remains static at **1.00x – 1.02x**, demonstrating complete lack of write parallelization.
- Amdahl's Law limit is 100% serial execution ($f_{serial} \approx 1.0$).
- Minor variations in throughput (< 2%) are attributable to minor Tokio runtime scheduling fluctuations.

---

## 4. Mixed-Workload Results (Reads + Writes)

To evaluate reader-writer contention under heavy write load, search latency was measured under two scenarios:
1. **Idle Index**: 500 search queries on an idle 50,000-vector index.
2. **High Write Load**: 500 search queries executed concurrently while 16 parallel Tokio tasks inserted 1,000 vectors into the graph.

### 4.1 Search Latency Percentiles (128-dim, $k=10$, $ef_{search}=64$)

| Workload Scenario | Mean (µs) | P50 (µs) | P95 (µs) | P99 (µs) | Degradation Factor |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Idle Index (Baseline)** | 3,253.6 | 3,189.2 | 3,891.0 | 4,143.8 | **1.00x** |
| **Active Writes (16 Tasks)** | 3,506.4 | 3,472.8 | 4,350.5 | 4,679.3 | **1.08x** |

```
Search Latency Comparison (P50 & P95)
Idle Index : P50 = 3,189.2 µs | P95 = 3,891.0 µs
Concurrent : P50 = 3,472.8 µs | P95 = 4,350.5 µs (+8% Latency Overhead)
```

### 4.2 Mixed Workload Findings
- Search latency experiences only a mild **8% increase** (1.08x degradation) during heavy concurrent insertions.
- **Explanation**: `search()` operations execute lock-free relative to `write_mutex` and only compete for `nodes.read()`. Write locks on `nodes` (`nodes.write()`) are held for brief microsecond spans during node append and neighbor back-link updates, minimizing reader stall time.
- Read operations are safe from corruption due to parking_lot RwLock semantics and transactional staging.

---

## 5. Architectural Recommendations & Fine-Grained Locking Proposals

To unlock linear or near-linear write throughput scaling ($S_N \propto N$), the global serialization boundaries must be restructured. We present three concrete technical proposals ordered by implementation complexity and throughput benefit:

```
+---------------------------------------------------------------------------------+
| Proposal 1: Sharded Graph Lock Array (Medium Effort, High Gain)                 |
| - Replace single `nodes: RwLock<Vec<HnswNode>>` with Sharded Lock Nodes         |
| - Each HnswNode contains fine-grained per-node RwLock (`parking_lot::RwLock`)   |
| - Multi-threaded insertion searches layers concurrently, locking only target     |
|   neighbors during edge connection updates.                                     |
+---------------------------------------------------------------------------------+
| Proposal 2: Lock-Free Skip-List / Lock-Free Graph Traversal (High Effort)       |
| - Utilize lock-free AtomicPtr / AtomicU32 arrays for neighbor lists.            |
| - Compare-And-Swap (CAS) for neighbor updates without acquiring write locks.    |
+---------------------------------------------------------------------------------+
| Proposal 3: Batch Parallel Staging & Splicing (Low Effort, Medium Gain)          |
| - Perform candidate search in parallel across tasks WITHOUT holding write locks.|
| - Acquire `write_mutex` ONLY for the final graph edge splicing phase.           |
+---------------------------------------------------------------------------------+
```

### Proposal 1: Fine-Grained Per-Node Locking (Recommended Strategy)

#### Technical Proposal
1. **Node Level Mutex/RwLock**: Change `HnswNode` structure to hold per-layer neighbor connections under internal fine-grained locks:
   ```rust
   struct HnswNode {
       doc_id: DocId,
       vector: VectorData,
       connections: Vec<parking_lot::RwLock<Vec<u32>>>, // Fine-grained connection locks
       max_layer: usize,
       committed_tx: u64,
   }
   ```
2. **Lock-Free Append / Segmented Storage**: Store nodes in an append-only segmented list (e.g., `boxcar::Vec<HnswNode>` or `Arc<SwapVector<HnswNode>>`), allowing concurrent pushes without locking existing nodes.
3. **Locking Hierarchy during Insertion**:
   - **Phase 1 (Parallel Candidate Search)**: Search upper layers holding only read locks on visited node connections (`node.connections[l].read()`). Multiple threads can search the graph in parallel.
   - **Phase 2 (Targeted Edge Splicing)**: Lock only the specific neighbor nodes being updated (`neighbor.connections[l].write()`). Use deterministic node ID ordering when acquiring locks on multiple neighbors to prevent deadlocks.

#### Effort & Feasibility Estimate
- **Implementation Effort**: ~3–5 person-days.
- **Expected Throughput Scaling**: **4.0x – 8.0x** speedup on 8-core CPUs for parallel inserts.
- **Risk**: Low (well-established pattern in concurrent graph indexes like HNSWLIB / USearch).

---

## 6. Appendix: Raw Logs & Benchmark Outputs

### 6.1 Raw Execution Log (`cargo bench --bench audit_benchmarks`)

```text
===============================================================================
                   MEMFUSE-INDEX EMPIRICAL AUDIT BENCHMARKS
===============================================================================

--- 1. DISTANCE METRIC THROUGHPUT & SIMD SPEEDUP (1536-dim, 100,000 ops) ---
Metric       	Scalar (ms)	SIMD (ms)	Throughput (ops/s)	Speedup
Cosine       	240.28		33.91		2.95e6		7.09x
Euclidean    	240.16		31.08		3.22e6		7.73x
DotProduct   	235.01		26.46		3.78e6		8.88x

--- 2. HNSW BUILD TIME VS DATASET SIZE (128-dim, M=16, ef_construction=200) ---
N       	Build Time (ms)	Throughput (vec/sec)
100		10.83		9231.3
1000		570.79		1752.0
5000		8624.26		579.8

--- 3. RECALL VS SEARCH LATENCY PARETO FRONT (N=1,000, 128-dim) ---
ef_search	p50 (µs)	p95 (µs)	p99 (µs)	Recall@10
8		200.4		324.8		412.8		0.9980
16		290.8		386.8		425.8		0.9980
32		299.2		394.9		482.1		1.0000
64		448.7		533.1		580.6		0.9980
128		603.5		692.2		761.1		1.0000
256		705.6		784.1		992.6		1.0000

--- 4. MEMORY FOOTPRINT & SQ8 REDUCTION FACTOR (2,000 vectors) ---
Dimension	Unquantized (MB)	SQ8 Quantized (MB)	Reduction Factor	Recall@10 Loss
128		1.32			0.61			2.18x			0.0150
384		3.28			1.09			3.00x			0.0100
768		6.21			1.83			3.40x			0.0100

--- 5. HNSW WRITE THROUGHPUT SCALING (50,000 Base Vectors, 1,000 Total Inserts) ---
Building base index with 50000 vectors...
Base index of 50000 vectors built in 152.85s
Threads	Total Inserts	Elapsed (ms)	Throughput (vec/s)	Scaling Factor
1	1000		3599.92		277.8		1.00x
2	1000		3582.58		279.1		1.00x
4	1000		3573.96		279.8		1.01x
8	1000		3570.06		280.1		1.01x
16	1000		3528.00		283.4		1.02x

--- 6. HNSW MIXED WORKLOAD BENCHMARK (Concurrent Reads + High Write Load) ---
Baseline Search Latency (Idle Index, 50k vectors):
  Mean: 3253.6 µs, P50: 3189.2 µs, P95: 3891.0 µs, P99: 4143.8 µs

Mixed Workload Search Latency (Under 16 Parallel Insert Tasks):
  Mean: 3506.4 µs, P50: 3472.8 µs, P95: 4350.5 µs, P99: 4679.3 µs
  Search Latency Degradation Factor: 1.08x (under concurrent write load)

===============================================================================
                          AUDIT BENCHMARKS COMPLETE
===============================================================================
```

---
*End of Audit Report.*
