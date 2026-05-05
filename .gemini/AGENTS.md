# MemFuse — Agentic OS Configuration

<agent_context>
  <system>MemFuse Embedded Hybrid-Search Database</system>
  <phase>TDD & Feature Implementation</phase>
  <tolerances>Zero-Failure, Strict Determinism</tolerances>
  <spec_directory>docs/specs/</spec_directory>
  <architecture>docs/architecture/</architecture>
</agent_context>

---

## System Overview

MemFuse is an **Embedded Hybrid-Search vector database** optimized for local AI agents.
It combines a dense vector index (HNSW) with metadata storage in a Sovereign Core architecture.

**Crate Dependency Graph:**
```text
memfuse            (Facade / lib.rs)
  ├── memfuse-db     (Collections, Search orchestration)
  │     ├── memfuse-index (HNSW, SIMD)
  │     └── memfuse-store (LSM, Compaction, WAL)
  └── memfuse-core   (Types, Traits, TxBuffer, MemBank)
```

---

## The TDD / Atomic Spec Workflow

No significant code should be altered without a corresponding **Atomic Spec**. 

### <protocol name="TDD Validation Loop">
1. **Red Phase**: Engineer a `#[tokio::test]` that formally expects the new behavior and naturally fails on the current codebase.
2. **Green Phase**: Implement the minimal Rust logic required to satisfy the invariant and make the test pass.
3. **Refactor Phase**: Execute the validation pipeline:
   ```bash
   just check  # runs fmt, clippy, check
   just test   # runs all tests
   ```
4. If ANY command fails, read the compiler output EXACTLY and fix the issue. **NEVER skip validation.**
</protocol>

---

## Coding Doctrine (NON-NEGOTIABLE)

```rust
// ❌ FORBIDDEN (Will fail the build):
.unwrap()                    // → Propagate error via MemFuseError and ?
std::fs::read()              // → strictly tokio::fs / async I/O
unsafe { ... }               // → Only allowed inside SIMD logic, paired with // SAFETY: proof
panic!()                     // → MUST be Result<()> in hot-paths

// ✅ MANDATORY:
#[tracing::instrument(skip(self))]   // Telemetry on major IO/compute boundaries
// SPEC-0XX §Y.Z                    // Link implementation to an Atomic Spec
// INVARIANT: <Condition>           // Document critical path conditions
```

---

## Agentic Roll-out (Conductor)

<agent_orchestration>
  <conductor>
    <role>System Supervisor & Task Dispatcher</role>
    <constraints>
      - ALWAYS confirm an Atomic Spec exists or draft a new one BEFORE coding.
      - ALWAYS begin implementation by writing a failing test.
      - NEVER commit logic with unresolved compiler warnings.
    </constraints>
    <protocol name="Justification">
      Every architectural decision MUST be justified against the "SQLite for AI Agents" paradigm and the Zero-Panic doctrine.
    </protocol>
  </conductor>
</agent_orchestration>
