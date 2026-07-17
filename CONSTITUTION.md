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

### 4. Code Alignment
-   Code must be readable and maintainable by humans. 
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

### 3. Unified Documentation System
To keep context synchronous and strictly organized, we enforce a precise MECE (Mutually Exclusive, Collectively Exhaustive) documentation model based on the v4 context hierarchy:
-   **`README.md`**: Entry point and high-level feature list.
-   **`CONSTITUTION.md` & `DEVELOPERS.md`**: Immutable system rules. 
-   **`AGENTS.md`**: Single source of truth for agent rules.
-   **`docs/ARCHITECTURE.md` (or `ARCHITECTURE.md`)**: The structural DAG. Rare changes.
-   **`DECISIONS.md`**: Architecture Decision Records (ADRs) repository.
-   **`GLOSSARY.md`**: Domain vocabulary and definitions.
-   **`SECURITY.md`**: Threat model, sandboxing, and ingestion defense rules.
-   **`TESTING.md`**: Testing philosophy, mutation testing, and allowances.
-   **`docs/SOURCE_OF_TRUTH.md` (Living State)**: Must be updated in the **same transaction/PR** as the code when components or findings change.
-   **No Temporary Folders**: `docs/specs`, `docs/archive`, and `docs/audit` are prohibited. If a spec is implemented, its knowledge must be merged entirely into `SOURCE_OF_TRUTH.md` or relevant core docs, and the spec is discarded. Ensure every item has a distinct, single location. Code-level documentation (`pub` items) and core invariant comments (`// ANCHOR`) are required inside the codebase directly.

---

## ⚖️ Governance

Changes to this Constitution require a consensus of the lead architects. Technical decisions (ADRs) must be immediately documented in the ADR registry inside `DECISIONS.md`.
