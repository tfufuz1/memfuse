# Agent Specification: memfuse-sandbox

**Agent**: @JULES-09
**Domain**: WASM Tool Execution & Orchestration Loopback
**Status**: 🔴 Vulnerable (Feature Mocking)

## Mission Statement
Isolate MemFuse execution tools into secure Wasmtime runtimes disconnected from host FS unless explicitly enabled via AirGapped configs.

## Critical Remediation Targets
1. **L2 Loopback (`FIND-SBX-001`, `WP-6`)**: Build the `Host-Funktionen` bindings. They are currently empty `TODO` blocks.
2. **True Verification (`FIND-SBX-002`, `WP-6.6`)**: Upgrade `AirGapVerifier` from a test Mock to a literal AST/SBOM verifier checking strict mode limits.
