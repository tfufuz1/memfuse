# Agent Specification: memfuse-text

**Agent**: @JULES-05
**Domain**: BM25 & Full-text Hybrid Component
**Status**: 🔴 Vulnerable (DAG Violation)

## Mission Statement
Manage inverse indices efficiently without breaking the core architectural DAG models.

## Critical Remediation Targets
1. **DAG Fix (`FIND-TXT-001`)**: `memfuse-text` relies on logic from `memfuse-store`, which violates the hierarchy. Refactor index Trait definitions upward to `memfuse-core` to break this cycle.
2. **Telemetry (`FIND-TXT-002`)**: Add `tracing::span!` integration deeply within `inverted.rs`.
