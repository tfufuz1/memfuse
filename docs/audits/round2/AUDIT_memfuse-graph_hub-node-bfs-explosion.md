# AUDIT REPORT: Hub-Node BFS Explosion & Unbounded Memory/CPU Scaling in `memfuse-graph`

**Audit-ID:** AUDIT_memfuse-graph_hub-node-bfs-explosion
**Target:** `crates/memfuse-graph/src/csr.rs` & `crates/memfuse-db/src/collection/search.rs`
**Date:** 2026-08-31
**Status:** VULNERABILITY CONFIRMED (DoS Vector: Memory Exhaustion + CPU Thread Starvation)

---

## 1. Executive Summary

The Gemini security analysis warning regarding intermediate BFS explosion on **"Hub-Nodes"** (nodes with extremely high out-degree, e.g. a central document in a knowledge graph) has been **FULL CONFIRMED**.

Although `MAX_SEARCH_K` (1,000) or vector search result limits are documented globally in `memfuse-core`, the BFS traversal implementations in `CsrGraph` (`traverse`, `traverse_at`, `traverse_at_time`) do **NOT** enforce any intermediate branching limits during queue population or per depth level.

### Key Audit Findings
1. **Gemini Warning Validated:** `MAX_SEARCH_K` only caps final returned results in higher-level collection methods or limits initial search anchors. During `CsrGraph::traverse`, **ALL** outgoing edges of a visited node are unconditionally pushed to `queue` and recorded in `visited`.
2. **Intermediate Peak Memory Explosion:** For a hub node with $1,000,000$ outgoing edges, peak intermediate memory consumption during a 2-hop traversal reaches **~53.41 MB** for a single request (scaling linearly $O(N)$ with hub degree).
3. **Severe CPU Latency / DoS Vector:** Wall-clock traversal time scales linearly from **2.11 ms** (1K Hub) to **2,642.60 ms (2.64 seconds)** for a 1M Hub. Because traversal executes synchronously within the async worker task context without per-node branching limits, a single query traversing a hub node stalls Tokio worker threads and can cause system-wide denial of service (DoS).

---

## 2. BFS Termination Mechanism Code Analysis

Analysis of `CsrGraph::traverse`, `traverse_at`, and `traverse_at_time` in `crates/memfuse-graph/src/csr.rs`:

```rust
// Lines 860-910 of crates/memfuse-graph/src/csr.rs (traverse)
while let Some((node_idx, hop, current_score)) = queue.pop_front() {
    if hop > effective_max {
        continue;
    }

    let existing = visited.entry(node_idx).or_insert(0.0);
    if current_score > *existing {
        *existing = current_score;
    }

    if hop < effective_max {
        // 1. CSR traversal (compacted edges)
        if node_idx < inner.offsets.len() - 1 {
            let start_edge = inner.offsets[node_idx];
            let end_edge = inner.offsets[node_idx + 1];

            for edge_idx in start_edge..end_edge { // <-- UNBOUNDED ITERATION
                let neighbor_idx = inner.targets[edge_idx];
                if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                    continue;
                }
                let weight = inner.weights[edge_idx];
                let next_score = current_score * SCORE_DECAY * weight;

                if !visited.contains_key(&neighbor_idx)
                    || visited[&neighbor_idx] < next_score
                {
                    if inner.entities.get(neighbor_idx).is_some_and(|e| e.is_some()) {
                        queue.push_back((neighbor_idx, hop + 1, next_score)); // <-- UNBOUNDED ENQUEUE
                    }
                }
            }
        }
        // ... Delta buffer traversal ...
    }
}
```

### Flaws Identified
1. **Unbounded Level Expansion:** When a node with $N$ outgoing edges is popped from `queue` at hop $h < \text{max\_hops}$, all $N$ target neighbors are enqueued unconditionally into `queue` (`VecDeque`) and inserted into `visited` (`HashMap`).
2. **Absence of `MAX_SEARCH_K` Guard in BFS Loop:** `MAX_SEARCH_K` is nowhere referenced inside `CsrGraph`. It is only checked post-hoc in `Collection::traverse_links` or during vector index search filtering.
3. **Queue & Visited State Overhead:**
   - Queue element: `(InternalIndex, u8, f32)` tuple = 24 bytes in `VecDeque`.
   - Visited map element: `HashMap<InternalIndex, f32>` = ~32 bytes per entry (including hash table metadata).
   - For $N$ hub edges, memory requirement is $N \times (24 + 32) \text{ bytes} \approx 56N \text{ bytes}$.

---

## 3. Peak Memory vs. Hub Size Table

Measurements captured using `crates/memfuse-graph/tests/hub_node_benchmark.rs`:
- **Scenario:** Start Node $S \to \text{Hub Node } H \to N \text{ Leaf Nodes}$
- **Traversal:** `max_hops = 2`

| Hub Degree ($N$) | Peak BFS Queue Items | Peak Visited Entries | Est. Peak Queue Mem | Est. Peak Visited Mem | **Total Peak Traversal Mem** | Scaling Factor |
|------------------|----------------------|----------------------|---------------------|-----------------------|------------------------------|----------------|
| **1,000**        | 1,000                | 1,002                | 23.44 KB            | 31.31 KB              | **54.75 KB (0.05 MB)**       | $1\times$      |
| **10,000**       | 10,000               | 10,002               | 234.38 KB           | 312.56 KB             | **546.94 KB (0.53 MB)**      | $10\times$     |
| **100,000**      | 100,000              | 100,002              | 2.29 MB             | 3.05 MB               | **5.34 MB**                  | $100\times$    |
| **1,000,000**    | 1,000,000            | 1,000,002            | 22.89 MB            | 30.52 MB              | **53.41 MB**                 | $1000\times$   |

*Conclusion:* Intermediate peak memory scales strictly linearly $O(N)$ with hub degree $N$, proving that `MAX_SEARCH_K` does not constrain intermediate state.

---

## 4. Latency vs. Hub Size Table

Wall-clock CPU execution time measured for `CsrGraph::traverse(start_id, 2)`:

| Hub Degree ($N$) | Total Graph Entities | Total Graph Edges | Wall-Clock Latency (ms) | Wall-Clock Latency (Readable) | Latency Scaling |
|------------------|----------------------|-------------------|-------------------------|-------------------------------|-----------------|
| **1,000**        | 1,002                | 1,001             | 2.11 ms                 | 2.11 ms                       | $1.0\times$     |
| **10,000**       | 10,002               | 10,001            | 16.27 ms                | 16.27 ms                      | $7.7\times$     |
| **100,000**      | 100,002              | 100,001           | 168.29 ms               | 168.29 ms                     | $79.8\times$    |
| **1,000,000**    | 1,000,002            | 1,000,001         | 2,642.60 ms             | **2.64 seconds**              | $1252.4\times$  |

---

## 5. DoS Risk Assessment

### 1. CPU Thread Starvation (Primary DoS Vector)
- A single traversal passing through a 1M-edge hub consumes **2.64 seconds** of pure CPU time on the calling thread.
- If multiple concurrent requests trigger traversals through hub nodes, Tokio runtime worker threads become blocked, causing high tail latency and cascading timeouts across all API endpoints.

### 2. Memory Exhaustion / Amplification (Secondary DoS Vector)
- Concurrent execution of 100 requests traversing a 1M hub node requires $>5.3 \text{ GB}$ of ephemeral RAM for BFS queue and visited state allocation.
- In low-memory sidecar or container environments (e.g. 512 MB – 2 GB RAM limit), this rapidly triggers Out-Of-Memory (OOM) kernel panics.

### 3. Exploitability
- **Organic Hubs:** Centralized metadata nodes (e.g., "User", "System", "Main Category") naturally accumulate $10^4 - 10^6$ edges in real-world knowledge graphs.
- **Malicious Hubs:** An attacker with write access can insert an entity with thousands of trivial relations to force high latency on graph search operations.

---

## 6. Hardening Proposal

To prevent BFS hub explosion, `CsrGraph` traversal must enforce **Per-Node Branching Limits** and **Bounded Priority Expansion**:

### 1. Per-Node Max Branching Factor (`MAX_BRANCHING_PER_NODE`)
Introduce a configurable per-expansion out-degree limit (e.g., `max_branching = 256` or `MAX_SEARCH_K` upper bound):
- During node expansion, if out-degree exceeds `max_branching`, sort outgoing edges by edge weight descending and sample/take only the top `max_branching` neighbors.

```rust
// Proposed CsrGraphConfig addition
pub struct CsrGraphConfig {
    pub rebuild_threshold: usize,
    /// Maximum outgoing edges expanded per node during BFS traversal.
    pub max_branching_per_node: usize, // Default: 256
}
```

### 2. Global Visited / Candidate Cap (`MAX_SEARCH_K`)
- Enforce an upper bound on `visited.len()` during traversal (e.g., stop enqueuing when `visited.len() >= MAX_SEARCH_K * 2`).

### 3. Priority Queue / Best-First Expansion
- Replace FIFO `VecDeque` with a Max-Heap (`BinaryHeap`) ordered by `current_score`.
- Bound queue size to `MAX_SEARCH_K` to guarantee $O(K \log K)$ runtime independent of graph degree.

---

## 7. Appendix: Raw Benchmark Logs

```
=== HUB-NODE BFS TRAVERSAL SCALING BENCHMARK ===
Evaluating peak queue/visited size, memory overhead, and CPU latency across hub out-degrees

--- HUB OUT-DEGREE: 1000 ---
Graph Entities       : 1002
Graph Edges          : 1001
Final Results Count  : 1001
Peak BFS Queue Items : 1000
Peak Visited Entries : 1002
Peak Queue Mem (est) : 23.44 KB
Peak Visited Mem(est): 31.31 KB
Total Peak Traversal : 54.75 KB (0.05 MB)
Wall-Clock Latency   : 2.107232ms

--- HUB OUT-DEGREE: 10000 ---
Graph Entities       : 10002
Graph Edges          : 10001
Final Results Count  : 10001
Peak BFS Queue Items : 10000
Peak Visited Entries : 10002
Peak Queue Mem (est) : 234.38 KB
Peak Visited Mem(est): 312.56 KB
Total Peak Traversal : 546.94 KB (0.53 MB)
Wall-Clock Latency   : 16.273495ms

--- HUB OUT-DEGREE: 100000 ---
Graph Entities       : 100002
Graph Edges          : 100001
Final Results Count  : 100001
Peak BFS Queue Items : 100000
Peak Visited Entries : 100002
Peak Queue Mem (est) : 2343.75 KB
Peak Visited Mem(est): 3125.06 KB
Total Peak Traversal : 5468.81 KB (5.34 MB)
Wall-Clock Latency   : 168.293564ms

--- HUB OUT-DEGREE: 1000000 ---
Graph Entities       : 1000002
Graph Edges          : 1000001
Final Results Count  : 1000001
Peak BFS Queue Items : 1000000
Peak Visited Entries : 1000002
Peak Queue Mem (est) : 23437.50 KB
Peak Visited Mem(est): 31250.06 KB
Total Peak Traversal : 54687.56 KB (53.41 MB)
Wall-Clock Latency   : 2.642603915s
```
