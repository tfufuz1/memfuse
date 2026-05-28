# MemFuse — Central Backlog

This document is the **single source of truth** for all open tasks, findings, and technical debt in the MemFuse project.

---

## 🚦 Priority Tiers

- **TIER 1 (BLOCKING):** Critical security vulnerabilities, data loss paths, or catastrophic stability issues.
- **TIER 2 (HIGH):** Major architectural gaps, performance bottlenecks, or missing production-readiness features.
- **TIER 3 (MEDIUM):** Technical debt, observability gaps, or minor usability improvements.
- **TIER 4 (LOW):** Documentation, non-critical optimizations.

---

## 🛠️ Active Backlog

### TIER 1 — Release Blockers (CRITICAL)

| ID | Crate | Title | Problem | Spec / Plan |
|---|---|---|---|---|
| **FIND-CRY-002** | `crypto` | AES-GCM Nonce-Reuse | Multiple files using same offset as nonce. | [GOLD-Nonce](./specs/SPEC-20260528-GOLDSTANDARD-ZeroPanicPersistence.md) |
| **FIND-STO-003** | `store` | Rollback-Inconsistency | SSTables ignored during rollback. | [GOLD-Persist](./specs/SPEC-20260528-GOLDSTANDARD-ZeroPanicPersistence.md) |
| **FIND-COR-001** | `core` | Trait Integrity | Dangerous `Ok(())` defaults in core traits. | - |
| **FIND-COR-002** | `core` | Atomic Underflow | `ResourceTracker` can wrap around. | - |
| **FIND-SAOS-001**| `saos` | Atomic Final State | Missing final checkpoint at `NodeType::End`. | - |
| **FIND-IDX-002** | `index` | NaN/Inf Poisoning | HNSW index poisoned by non-finite vectors. | [Hnsw-Async](./specs/SPEC-20260528-GOLDSTANDARD-HnswAsyncIO.md) |
| **FIND-TXT-003** | `text` | BM25 Div-by-Zero | Empty index causes `NaN` scores. | - |
| **FIND-CRY-001** | `crypto` | Hardcoded Salt | Weakened password security via static HKDF salt. | - |
| **FIND-TXT-001** | `text` | DAG Violation | `memfuse-store` in `dev-dependencies`. | - |

### TIER 2 — Pre-Launch (HIGH)

| ID | Crate | Title | Problem | Spec / Plan |
|---|---|---|---|---|
| **FIND-STO-001** | `store` | WAL CRC & Starvation | Missing CRC and CPU starvation in Compaction. | - |
| **FIND-DB-001** | `db` | Snapshot-Recovery | Missing high-level API for snapshot management. | [WP-5.1](./specs/SPEC-20260508-WP-5.1-Checkpointing.md) |
| **FIND-IDX-001** | `index` | SIMD Safety | `unsafe` blocks lack rigorous comments. | [Stable-SIMD](./specs/SPEC-20260528-GOLDSTANDARD-StableSIMD.md) |
| **FIND-SBX-001** | `sandbox` | Skeleton Host-Funcs | Sandbox cannot access DB. | - |
| **FIND-SBX-002** | `sandbox` | Mock AirGap | `AirGapVerifier` is a placeholder. | - |
| **FIND-GRA-001** | `graph` | Isolation & Perf | CSR-Graph lacks transaction isolation. | - |
| **FIND-PY-001** | `py` | Exception Mapping | Skeleton "Zero Vector" in Python. | [WP-7.3](./specs/SPEC-20260524-WP-7.3-MCPProvider.md) |
| **WP-7.1** | `text` | Markdown Chunker | New Feature: Hierarchical chunking. | [WP-7.1](./specs/SPEC-20260524-WP-7.1-MarkdownChunker.md) |
| **WP-7.2** | `index` | HNSW Persistence | New Feature: Save/Load HNSW Index. | [WP-7.2](./specs/SPEC-20260524-WP-7.2-HnswPersistence.md) |

### TIER 3 — Tech Debt (MEDIUM)

| ID | Crate | Title | Problem |
|---|---|---|---|
| **FIND-DB-002** | `db` | Missing Tracing | Critical API paths lack OpenTelemetry instrumentation. |
| **FIND-TXT-002** | `text` | Missing Tracing | BM25 and Tokenizer paths lack instrumentation. |
| **FIND-STO-002** | `store` | Budgeted Compaction | Compaction ignores memory limits. |
| **FIND-IDX-003** | `index` | Rebuild Threshold | Default 0.8 is too high for production. |
| **FIND-COR-003** | `core` | Pure Core Violation | Async/Tokio usage in "pure" type crate. |

---

## 🎯 Definition of Done (Triple-Test-Gate)
1. [ ] **Code**: Sovereign Core Doctrine (Zero-Panic, Safe Rust).
2. [ ] **Tests**: Unit tests + Property tests cover the fix.
3. [ ] **Verification**: `just triple-test` passes.
4. [ ] **Compliance**: `cargo clippy -- -D warnings` is clean.
