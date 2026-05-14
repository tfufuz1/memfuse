# Critical Build Blockers (May 2026)

The following issues were discovered in production code during a CI/DevOps maintenance run. As **AGENT:11 (CI/DevOps)**, I am strictly forbidden from modifying production code. These issues must be addressed by the responsible agents.

## 1. Missing Imports in `memfuse-text`
**File:** `crates/memfuse-text/src/inverted.rs`
**Issue:** `Tokenizer`, `DefaultTokenizer`, `GermanMorphTokenizer` are used but not imported.
**Responsible:** Agent 05 (Text Engine)

## 2. Residual Merge Markers in `memfuse-orchestrator`
**File:** `crates/memfuse-orchestrator/src/lib.rs`
**Issue:** `<<<<<<< HEAD`, `=======`, `>>>>>>> main` markers remain in the file.
**Responsible:** Agent 13 (Debt Hunter) or Agent 00 (Watchdog)

## 3. Residual Merge Markers in `memfuse-runtime`
**File:** `crates/memfuse-runtime/src/lib.rs`
**Issue:** `<<<<<<< HEAD`, `=======`, `>>>>>>> main` markers remain in the file.
**Responsible:** Agent 13 (Debt Hunter) or Agent 00 (Watchdog)

## 4. Corrupted `hybrid_search` in `memfuse-db`
**File:** `crates/memfuse-db/src/collection.rs`
**Issue:** The implementation of `hybrid_search` is logically broken, contains duplicated code blocks, and has syntax errors (missing closing braces, `serde_json.` instead of `serde_json::`).
**Responsible:** Agent 04 (Database Orchestrator)
