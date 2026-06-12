# MemFuse — Revised Implementation & Stabilization Plan (V2.0)

## Goal Description
The development of MemFuse has hit a critical roadblock: the codebase does not compile (failed `cargo check`), exhibiting fundamental Rust type system errors (`dyn` incompatibility, lifetime mismatches, `Sized` violations, missing `unwrap` handling). Additionally, advanced features (like HNSW persistence and MCP Provider) are inexplicably FROZEN, rendering the product unviable.

To transition MemFuse from an "AI Theater" prototype to a production-ready **Sovereign Data Operating System**, we must fundamentally pivot the strategy:
1. **P0: Fix Compilation & Architecture Fundamentals.** Resolve all compiler errors by correctly applying `async-trait` and fixing lifetimes/sized issues.
2. **P1: Implement Mission-Critical FROZEN Features.** Prioritize HNSW disk persistence (WP-7.2) and the MCP Provider.
3. **P2: Establish a Hardened Testing Infrastructure.** Transition from superficial tests to rigorous distributed and differential testing.

## Proposed Changes

### P0: Kernel Compilation Stabilization

#### [MODIFY] [memfuse-core/src/traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs)
- Add `#[async_trait]` to all core interfaces (`StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`, `Checkpoint`).
- Ensure no implicit lifetime bound errors occur when returning futures by validating that `#[async_trait]` applies correctly to signatures.

#### [MODIFY] [memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs)
- Add `#[async_trait]` to the `impl GraphIndex for CsrGraph` block to resolve the `E0195` lifetime mismatch on `add_entity`.

#### [MODIFY] [memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs)
- Remove instances of unsized `[u8]` on stack variables (replace with `Vec<u8>`, `Box<[u8]>` or references `&[u8]`).
- Add `#[async_trait]` on `impl TextIndex for InvertedIndex<...>`.
- Refactor `dyn StorageEngine` references to `<S: StorageEngine>`.

#### [MODIFY] [memfuse-embed/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-embed/src/lib.rs)
- Remove the `unwrap()` at line 84 and replace with typed `Result` mapping `?` pointing to `MemFuseError::Internal`.

---

### P1: Unfreezing Critical Features

#### [MODIFY] [memfuse-index/src/persistence.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/persistence.rs)
- Unfreeze and complete the HNSW binary Save/Load logic. A vector database that loses its index on reboot is not functional.
- Implement incremental checkpointing instead of full-rebuilds.

#### [MODIFY] [memfuse-py/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs)
- Unfreeze the MCP Provider.
- Implement 100% test coverage using Python `pytest` binding over `maturin` to guarantee stability.

---

## Verification Plan & Comprehensive Testing Infrastructure

To prevent a regression to "AI Theater," we will implement a robust testing regime.

### 1. Differential Fuzzing Suite (`memfuse-fuzz`)
- **Action**: Create a new `crates/memfuse-fuzz` target using `cargo-fuzz`.
- **Methodology**: Generate a random sequence of `put/delete/commit` operations. Simulate a mid-transaction crash by skipping `sync_all()`, then trigger WAL recovery.
- **Verification**: Assert that the state constructed by WAL replay perfectly matches an isolated in-memory deterministic BTreeMap.

### 2. Jepsen-Style Consistency Tests
- **Action**: Integrate `madsim` in `crates/memfuse-cluster/tests`.
- **Methodology**: Simulate a 3-node cluster. Inject network partitions, delayed packets, and randomized clock skew while sustaining a high write-rate. 
- **Verification**: Verify that the snapshot reads are strictly serializable and no split-brain writes persist after quorum resolution.

### 3. Automated Recall Benchmarking
- **Action**: Add `crates/memfuse-index/benches/recall.rs`.
- **Methodology**: Load the Sift1M evaluation dataset. Run queries tracking recall@10 before and after `trigger_rebuild_async` and compaction.
- **Verification**: Regression > 0.5% in Recall parity instantly fails the CI workflow.

### 4. Hardware-Under-Test (HUT) Validation
- **Action**: Configure physical CI nodes for architecture testing instead of generic cloud VMs.
- **Methodology**: Force execution of `dot_product_u8_avx512vnni` and `cosine_f32_avx512` on real AVX-512 capable hardware to test SIGILL resilience.
- **Verification**: `cargo test -p memfuse-index` must return a successful exit code `0` on actual bare-metal nodes.

### 5. Automated Debt Audit Integration
- **Action**: Enhance `just debt-audit`.
- **Methodology**: Ensure `cargo check` and `cg -D warnings` acts as hard blockers. 
- **Verification**: Running `just triple-test` ensures that no PR is accepted unless it passes compilations, unit tests, and the new fuzzing smoke-tests seamlessly. 
