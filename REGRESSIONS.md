# Workspace Regressions - 2026-05-15

The following regressions were identified in production code and currently block CI execution. In accordance with the NIEMALS rule for AGENT:12 (Integration Tester), these issues must be resolved by the respective responsible agents.

## 1. Critical Build Blockers
- `crates/memfuse-checkpoint/Cargo.toml`: Duplicate keys in `[dev-dependencies]` (lines 28-31: `memfuse-db`, `memfuse-core`, `serde_json`).
- `crates/memfuse-runtime/src/lib.rs`: Unresolved Git merge conflict markers (`<<<<<<< HEAD` ... `>>>>>>> main`).
- `crates/memfuse-orchestrator/src/lib.rs`: Unresolved Git merge conflict markers.
- `crates/memfuse-db/src/collection.rs`: Syntax error (brace mismatch) and incomplete logic in the `hybrid_search` method.
- `crates/memfuse-text/src/inverted.rs`: Missing imports for `Tokenizer` traits (`Tokenizer`, `DefaultTokenizer`, `GermanMorphTokenizer`).

## 2. Clippy & Doctrine Compliance
- `manual-div-ceil` and `manual-clamp` warnings are reported by the user in `memfuse-store/src/bloom.rs`.
- **Audit Note**: The file `crates/memfuse-store/src/bloom.rs` was NOT found in the current file tree during the agent's scan. Bloom filter logic was instead observed in `crates/memfuse-store/src/sstable.rs`.
- These warnings violate the Sovereign Core Doctrine (Warnings = Errors).

## 3. Impact Statement
The integration and stress tests implemented by AGENT:12 are verified for logical correctness but cannot be successfully executed or validated by CI until the workspace manifest and production source files are stabilized.
