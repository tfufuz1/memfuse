---
spec_id: "SPEC-20260527-L1-CsrGraph"
title: "L1 Crate memfuse-graph CSR Implementation"
status: "DRAFT"
author: "Lead Architect"
priority: "HIGH"
---

# 1. Objective
Implement a memory-efficient Compressed Sparse Row (CSR) graph structure for the `memfuse-graph` crate. This structure serves as Signal 3 (Entity Relations) in the 4-Signal Hybrid Search Fusion.

# 2. Requirements
- **Storage Layout:** Implement CSR using contiguous arrays (`offsets`, `targets`, `weights`) to minimize memory overhead and cache misses.
- **Traversal:** Implement BFS with kausal decay: `score = starting_score * 0.7^hop * edge_weight`.
- **Constraint:** Maximum traversal depth of 3 hops (`max_hop: 3`).
- **Concurrency:** Thread-safe access using `parking_lot::RwLock`.
- **Sovereign Core:** Zero `unwrap()`, zero panics, zero `std::fs`.

# 3. Design
## 3.1 Data Structures
```rust
pub struct CsrGraph {
    // Mapping from public EntityId to internal contiguous index (0..N)
    id_map: RwLock<HashMap<EntityId, usize>>,
    // Internal index to public EntityId
    reverse_map: RwLock<Vec<EntityId>>,
    // Entity metadata
    entities: RwLock<Vec<Entity>>,
    
    // CSR Structure
    offsets: RwLock<Vec<usize>>,
    targets: RwLock<Vec<usize>>, // Internal indices
    weights: RwLock<Vec<f32>>,
    
    // Staging for dynamic updates (before compaction)
    staged_edges: RwLock<HashMap<usize, Vec<(usize, f32)>>>,
}
```

## 3.2 Compaction Logic
Since CSR is static, we provide a `compact()` method (or trigger it automatically) to move `staged_edges` into the flat CSR arrays. Traversal will combine both or enforce compaction. For this initial L1 implementation, we focus on the CSR-based traversal.

# 4. Traversal Algorithm (BFS)
1. Initialize `visited` map with `EntityId -> max_score`.
2. Initialize BFS queue with `(start_node_index, current_hop, current_score)`.
3. While queue not empty and hop < 3:
   a. Get neighbors from `offsets[node]..offsets[node+1]`.
   b. For each neighbor:
      i. `next_score = current_score * 0.7 * edge_weight`.
      ii. If `next_score > visited[neighbor]`, update and enqueue.

# 5. Tests
- `test_csr_layout_contiguity`: Verify that offsets and targets are correctly built.
- `test_bfs_decay_limit`: Verify score decay and 3-hop limit.
- `test_empty_graph`: Ensure no panics on empty or disconnected graphs.
