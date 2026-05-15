# Workspace Regressions

The following regressions were identified on 2026-05-15 and are currently blocking the full execution of the integration test suite. These issues exist in production code and, according to the NIEMALS rule, cannot be fixed by Agent AGENT:12.

## 1. Conflict Markers
The following files contain unresolved Git merge conflict markers:
- `crates/memfuse-runtime/src/lib.rs`
- `crates/memfuse-orchestrator/src/lib.rs`

## 2. Duplicate Dependencies in Cargo.toml
The following crate has duplicate entries in its `Cargo.toml`, which prevents `cargo` from parsing the workspace manifest:
- `crates/memfuse-checkpoint/Cargo.toml` (Duplicate keys: `memfuse-db`, `memfuse-core`, `serde_json` in `[dev-dependencies]`)

## 3. Syntax Errors and Logic Bugs
- `crates/memfuse-db/src/collection.rs`: Brace mismatch (99 open braces vs 97 close braces) around the `hybrid_search` method.
- `crates/memfuse-db/src/collection.rs`: `hybrid_search` implementation is incomplete/broken (missing closing braces and proper hydration logic).
- `crates/memfuse-text/src/inverted.rs`: Missing imports for `Tokenizer`, `DefaultTokenizer`, and `GermanMorphTokenizer`.

## 4. Build Impact
These regressions cause `cargo check` and `cargo test` to fail at the manifest loading or compilation stage, meaning that even new, correctly implemented tests cannot be fully verified in the current environment until these production issues are resolved by the respective responsible agents.
