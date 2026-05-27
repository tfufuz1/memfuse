# MemFuse — Project Constitution

This document defines the core principles and non-negotiable standards of the MemFuse project. It serves as the moral and technical compass for all development.

---

## 🏛️ Core Principles

### 1. Safety First (Sovereign Core Doctrine)
-   **Memory Safety**: We prefer Safe Rust. `unsafe` is only permitted for hardware-specific optimizations (SIMD) and must be accompanied by a rigorous `// SAFETY:` proof comment.
-   **No Panics**: Libraries must never crash their host. Explicit error handling (`Result`) is mandatory. `.unwrap()` and `.expect()` are banned from production code.

### 2. Reliability & Durability
-   **WAL First**: No data modification in memory before the change is physically committed to the Write-Ahead-Log and synced to disk.
-   **Deterministic Recovery**: The system must be able to reconstruct its state from logs alone.

### 3. Modularity & The DAG
-   Architectural integrity is maintained by a strict Directed Acyclic Graph. 
-   Layer 0 (Core) must remain agnostic of high-level features.

### 4. Agentic Alignment
-   Code must be readable by both humans and LLM agents. 
-   Comments should explain **why** an invariant exists (e.g., `// ANCHOR:ARCH:LSM-001`).

---

## 🚦 Mandatory Development Standards

### 1. Error Handling
-   All errors must be categorizable in `memfuse_core::MemFuseError`.
-   Errors crossing the FFI boundary (e.g., to Python) must be mapped to native types.

### 2. Testing (The Triple-Test-Gate)
-   Unit tests are required for all logic.
-   Integration tests are required for all storage/recovery paths.
-   Benchmarks must be provided for all performance-critical hot-paths.

### 3. Documentation
-   All `pub` items must have doc comments.
-   Critical invariants must be tagged with `// ANCHOR` for cross-referencing.

---

## ⚖️ Governance

Changes to this Constitution require a consensus of the lead architects. Technical decisions (ADRs) must be documented in `docs/specs/decisions/`.
