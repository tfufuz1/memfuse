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
*All TIER 1 items have been successfully remediated and verified.*

| ID | Crate | Title | Status |
|---|---|---|---|
| **FIND-CRY-002** | `crypto` | AES-GCM Nonce-Reuse | ✅ DONE |
| **FIND-STO-003** | `store` | Rollback-Inconsistency | ✅ DONE |
| **FIND-TXT-003** | `text` | BM25 Div-by-Zero | ✅ DONE |
| **FIND-TXT-001** | `text` | DAG Violation | ✅ DONE |
| **FIND-IDX-002** | `index` | NaN/Inf Poisoning | ✅ DONE |
| **FIND-CRY-001** | `crypto` | Hardcoded Salt | ✅ DONE |

### TIER 2 — Pre-Launch (HIGH)

| ID | Crate | Title | Problem | Status |
|---|---|---|---|---|
| **FIND-STO-001** | `store` | WAL CRC & Starvation | Missing CRC and CPU starvation in Compaction. | 🟡 OPEN |
| **FIND-SBX-001** | `sandbox` | Skeleton Host-Funcs | Implement result sterilization. | ✅ DONE |
| **FIND-SBX-002** | `sandbox` | Mock AirGap | Placeholder cleaned. | ✅ DONE |
| **FIND-PY-001** | `py` | Exception Mapping | Precision mapping implemented. | ✅ DONE |
| **WP-7.1** | `text` | Markdown Chunker | Feature complete. | ✅ DONE |
| **WP-7.2** | `index` | HNSW Persistence | Save/Load implemented. | ✅ DONE |

### TIER 3 — Tech Debt (MEDIUM)

| ID | Crate | Title | Problem | Status |
|---|---|---|---|---|
| **FIND-IDX-003** | `index` | Rebuild Threshold | Tuning default to 0.3. | ✅ DONE |
| **FIND-COR-003** | `core` | Pure Core Violation | Audit complete. | ✅ DONE |
| **FIND-DB-002** | `db` | Missing Tracing | Coverage expansion. | 🟡 OPEN |

---

## 🎯 Definition of Done (Triple-Test-Gate)
1. [ ] **Code**: Sovereign Core Doctrine (Zero-Panic, Safe Rust).
2. [ ] **Tests**: Unit tests + Property tests cover the fix.
3. [ ] **Verification**: `just triple-test` passes.
4. [ ] **Compliance**: `cargo clippy -- -D warnings` is clean.
