# Audit Report: `memfuse-graph` PPR Dangling Nodes (Round 2)

**Client:** Global Enterprise Architecture Review
**Target:** `memfuse-graph` (Personalized PageRank / Dangling Nodes Subsystem)
**Auditor:** Senior Rust Numerischer-Algorithmen-Ingenieur (Spezialgebiet Markov-Ketten Ranking)
**Date:** August 31, 2026
**Status:** PASSED (Verified Exact Mass Conservation $1.000000$ & Zero Score Collapse across all Dangling Node Topologies)

---

## 1. Executive Summary

A targeted Round-2 numerical audit was conducted on the Personalized PageRank (PPR) implementation in `crates/memfuse-graph/src/ppr.rs`. The audit specifically focused on the **Dangling Node handling** (nodes with out-degree zero / sink-holes), which represents the most critical error surface for PageRank/PPR algorithms.

### Key Audit Findings:
- **Round-1 Audit Gap Confirmation:** The Round-1 audit (`AUDIT_memfuse-graph.md`) verified PPR against a 7-node benchmark graph containing 1 dangling node (Node 7). However, it lacked explicit coverage for:
  1. A well-connected 10-node graph with exactly 1 dangling node in the core.
  2. An extreme 90% dangling node stress test (20 nodes, 18 dangling nodes).
  3. A connected group of dangling nodes (3 sink nodes fed by core nodes).
  Therefore, the suspected Round-1 test coverage gap was **CONFIRMED**, requiring execution of the targeted test suite.
- **Dangling Node Teleportation Verification:** The implementation in `crates/memfuse-graph/src/ppr.rs` (lines 112–124) correctly accumulates rank mass residing at dangling nodes (`dangling_sum`) during each iteration and redistributes it back to the seed set via the modified teleportation factor:
  $$\text{teleport\_factor} = (1.0 - d) + d \times \text{dangling\_sum}$$
- **Numerical Verification Outcomes:**
  - **Graph 1 (1 Dangling / 10 Nodes):** Rank mass conserved to **$1.000000$** (diff $< 10^{-6}$). Core nodes retained non-zero, highly differentiated scores (range ratio $11.06\times$).
  - **Graph 2 (90% Dangling / 20 Nodes):** Rank mass conserved to **$1.000000$** (diff $< 10^{-6}$). Seed node retained $0.522186$, preventing any "score collapse" to zero.
  - **Graph 3 (Group of 3 Dangling Nodes / 12 Nodes):** Rank mass conserved to **$1.000000$** (diff $< 10^{-6}$). Dangling group received proportional rank mass without degrading core node scores.
- **Verdict:** `memfuse-graph` PPR power iteration is numerically sound, mathematically compliant with the standard Markov chain PPR definition, and fully immune to rank mass leakage or score collapse under dangling node stress conditions.

---

## 2. Runde-1-Abdeckungs-Review

Review of `docs/audits/AUDIT_memfuse-graph.md` (PPR Section):

| Aspect | Round-1 Audit State | Round-2 Audit Assessment |
| :--- | :--- | :--- |
| **Dangling Node Included?** | **YES** (Node 7 in 7-Node Benchmark) | Node 7 had out-degree $0$. However, it was part of a linear path ($6 \rightarrow 7$), not a complex well-connected core. |
| **Mass Conservation Tested?** | **YES** (Logged $1.000000$) | Mass conservation was checked for the 7-node benchmark graph. |
| **10-Node Graph w/ 1 Core Dangling Node?** | **NO** | Not tested in Round 1 (**Gap Confirmed**). |
| **Extreme 90% Dangling Graph?** | **NO** | Not tested in Round 1 (**Gap Confirmed**). |
| **Group of Dangling Nodes?** | **NO** | Not tested in Round 1 (**Gap Confirmed**). |

---

## 3. Score-Erhalt-Testmatrix

Tests were executed via `crates/memfuse-graph/tests/dangling_nodes_audit_test.rs`.
Parameters: Seed Node = Node 1, Damping Factor $d = 0.85$, Convergence Epsilon $\epsilon = 10^{-6}$, Max Iterations = 100.

### Test Graph Topologies & Numerical Results

#### Graph 1: Single Dangling Node in Well-Connected 10-Node Core
- **Topology:** Core nodes $1 \dots 9$ connected in a cycle with cross-links ($1\rightarrow3, 3\rightarrow7, 5\rightarrow2, 8\rightarrow4$). Node 10 is dangling (out-degree = 0) with incoming edges $1\rightarrow10, 5\rightarrow10, 9\rightarrow10$.

| Node ID | Role | PPR Score | Mass Conservation Sum | Convergence Check | Score Collapse Check | Status |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | Core (Seed) | $0.255138$ | - | - | Retains high score | **PASS** |
| **2** | Core | $0.095361$ | - | - | Well differentiated | **PASS** |
| **3** | Core | $0.153346$ | - | - | Well differentiated | **PASS** |
| **4** | Core | $0.095800$ | - | - | Well differentiated | **PASS** |
| **5** | Core | $0.081430$ | - | - | Well differentiated | **PASS** |
| **6** | Core | $0.023072$ | - | - | Minimum core score | **PASS** |
| **7** | Core | $0.084783$ | - | - | Well differentiated | **PASS** |
| **8** | Core | $0.072065$ | - | - | Well differentiated | **PASS** |
| **9** | Core | $0.030628$ | - | - | Well differentiated | **PASS** |
| **10** | Dangling Sink | $0.108378$ | - | - | Receives rank mass | **PASS** |
| **TOTAL** | **All 10 Nodes** | **$1.000000$** | **Exact $1.000000$** ($\Delta < 10^{-6}$) | **Converged** | **Ratio Max/Min = $11.06\times$** | **PASS** |

---

#### Graph 2: Extreme 90% Dangling Nodes (20 Nodes, 18 Dangling)
- **Topology:** Core nodes 1 and 2 connected bidirectionally ($1 \leftrightarrow 2$). Node 1 points to dangling nodes $3 \dots 11$ (out-degree = 0). Node 2 points to dangling nodes $12 \dots 20$ (out-degree = 0).

| Node ID | Role | PPR Score | Mass Conservation Sum | Convergence Check | Score Collapse Check | Status |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | Core (Seed) | $0.522186$ | - | - | Seed retains major rank mass | **PASS** |
| **2** | Core | $0.044386$ | - | - | Non-zero core score | **PASS** |
| **3..11** | Dangling (Group A) | $0.044386$ each | - | - | Identical symmetric scores | **PASS** |
| **12..20**| Dangling (Group B) | $0.003773$ each | - | - | Identical symmetric scores | **PASS** |
| **TOTAL** | **All 20 Nodes** | **$1.000000$** | **Exact $1.000000$** ($\Delta < 10^{-6}$) | **Converged** | **Zero Collapse (Seed = 0.522)** | **PASS** |

---

#### Graph 3: Group of Connected Dangling Nodes (12 Nodes, 3 Dangling Group)
- **Topology:** Core nodes $1 \dots 9$ in a cycle. Dangling group nodes $10, 11, 12$ have out-degree 0. Core feeds dangling group via $3\rightarrow10, 6\rightarrow11, 9\rightarrow12$.

| Node ID | Role | PPR Score | Mass Conservation Sum | Convergence Check | Score Collapse Check | Status |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | Core (Seed) | $0.247815$ | - | - | Core retains rank mass | **PASS** |
| **2** | Core | $0.210642$ | - | - | Non-zero core score | **PASS** |
| **3** | Core | $0.179046$ | - | - | Non-zero core score | **PASS** |
| **4** | Core | $0.076095$ | - | - | Non-zero core score | **PASS** |
| **5** | Core | $0.064680$ | - | - | Non-zero core score | **PASS** |
| **6** | Core | $0.054978$ | - | - | Non-zero core score | **PASS** |
| **7** | Core | $0.023366$ | - | - | Non-zero core score | **PASS** |
| **8** | Core | $0.019861$ | - | - | Non-zero core score | **PASS** |
| **9** | Core | $0.016882$ | - | - | Non-zero core score | **PASS** |
| **10** | Dangling Group | $0.076095$ | - | - | Receives rank from Node 3 | **PASS** |
| **11** | Dangling Group | $0.023366$ | - | - | Receives rank from Node 6 | **PASS** |
| **12** | Dangling Group | $0.007175$ | - | - | Receives rank from Node 9 | **PASS** |
| **TOTAL** | **All 12 Nodes** | **$1.000000$** | **Exact $1.000000$** ($\Delta < 10^{-6}$) | **Converged** | **Zero Collapse Across Core** | **PASS** |

---

## 4. Root Cause Analysis (Code Inspection)

Since all test cases passed without mass loss or score collapse, code inspection was performed to verify the exact mathematical mechanism in `crates/memfuse-graph/src/ppr.rs`:

```rust
// Lines 112-124 in crates/memfuse-graph/src/ppr.rs
// Rank mass accumulated at dead-end (dangling) nodes
let mut dangling_sum = 0.0f32;
for i in 0..n {
    if inner.entities.get(i).is_some_and(|e| e.is_some()) && out_weight_sums[i] == 0.0 {
        dangling_sum += ranks[i];
    }
}

// Teleport / restart contribution (including redistributed dangling rank mass)
let teleport_factor = (1.0 - damping) + damping * dangling_sum;
for &seed_idx in &valid_seeds {
    next_ranks[seed_idx] += teleport_factor * restart_prob;
}
```

### Mathematical Formula Verification:
The transition matrix for PageRank with dangling nodes is given by:
$$M = P + v \mathbf{d}^T$$
where $P_{ij} = \frac{w_{ij}}{\sum_k w_{ik}}$ for non-dangling nodes $i$, $\mathbf{d}$ is an indicator vector where $d_i = 1$ if node $i$ is dangling and $0$ otherwise, and $v$ is the personalization/teleportation vector ($v_s = \frac{1}{|S|}$ for seed nodes $s \in S$).

The power iteration update rule implemented in `ppr.rs` is:
$$r^{(k+1)} = d P^T r^{(k)} + \left( (1 - d) + d \sum_{i \in \text{dangling}} r_i^{(k)} \right) v$$

Summing all components of $r^{(k+1)}$:
$$\sum_j r_j^{(k+1)} = d \sum_{i \notin \text{dangling}} r_i^{(k)} + (1 - d) \sum_j v_j + d \sum_{i \in \text{dangling}} r_i^{(k)} \sum_j v_j$$

Since $\sum_j v_j = 1$:
$$\sum_j r_j^{(k+1)} = d \left( \sum_{i \notin \text{dangling}} r_i^{(k)} + \sum_{i \in \text{dangling}} r_i^{(k)} \right) + (1 - d) = d (1) + 1 - d = 1.0$$

This mathematically guarantees that total rank mass sum remains **exactly 1.0** at every iteration step $k$, preventing both rank leakage and rank accumulation collapse.

---

## 5. Priorisierte Bugliste

| Bug ID | Severity | Component | Description | Resolution / Status |
| :--- | :--- | :--- | :--- | :--- |
| **N/A** | **NONE** | `ppr.rs` | No bugs detected. Dangling node handling is mathematically exact and verified against all 3 test topologies. | **NO ACTION REQUIRED** |

---

## 6. Anhang: Rohlogs

```text
running 3 tests in tests/dangling_nodes_audit_test.rs

=== GRAPH 1: SINGLE DANGLING NODE (10 Nodes, Node 10 Dangling) ===
Node  1: 0.255138
Node  2: 0.095361
Node  3: 0.153346
Node  4: 0.095800
Node  5: 0.081430
Node  6: 0.023072
Node  7: 0.084783
Node  8: 0.072065
Node  9: 0.030628
Node 10: 0.108378 [DANGLING]
Total Rank Mass Sum: 1.000000
Core Score Range: min=0.023072, max=0.255138, ratio=11.06
test test_ppr_graph_1_single_dangling_node ... ok

=== GRAPH 2: EXTREME 90% DANGLING NODES (20 Nodes, 18 Dangling) ===
Node  1: 0.522186 [CORE]
Node  2: 0.044386 [CORE]
Node  3: 0.044386 [DANGLING]
Node  4: 0.044386 [DANGLING]
Node  5: 0.044386 [DANGLING]
Node  6: 0.044386 [DANGLING]
Node  7: 0.044386 [DANGLING]
Node  8: 0.044386 [DANGLING]
Node  9: 0.044386 [DANGLING]
Node 10: 0.044386 [DANGLING]
Node 11: 0.044386 [DANGLING]
Node 12: 0.003773 [DANGLING]
Node 13: 0.003773 [DANGLING]
Node 14: 0.003773 [DANGLING]
Node 15: 0.003773 [DANGLING]
Node 16: 0.003773 [DANGLING]
Node 17: 0.003773 [DANGLING]
Node 18: 0.003773 [DANGLING]
Node 19: 0.003773 [DANGLING]
Node 20: 0.003773 [DANGLING]
Total Rank Mass Sum: 1.000000
test test_ppr_graph_2_extreme_90_percent_dangling_nodes ... ok

=== GRAPH 3: GROUP OF DANGLING NODES (12 Nodes, Nodes 10,11,12 Dangling Group) ===
Node  1: 0.247815 [CORE]
Node  2: 0.210642 [CORE]
Node  3: 0.179046 [CORE]
Node  4: 0.076095 [CORE]
Node  5: 0.064680 [CORE]
Node  6: 0.054978 [CORE]
Node  7: 0.023366 [CORE]
Node  8: 0.019861 [CORE]
Node  9: 0.016882 [CORE]
Node 10: 0.076095 [DANGLING GROUP]
Node 11: 0.023366 [DANGLING GROUP]
Node 12: 0.007175 [DANGLING GROUP]
Total Rank Mass Sum: 1.000000
test test_ppr_graph_3_group_of_dangling_nodes ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
