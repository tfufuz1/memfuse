# AGENTS.md — memfuse-calibration (Layer 1)

## Purpose & Scope
`memfuse-calibration` implements probability calibration primitives (`IsotonicCalibrator` and `PlattScaler`) used across MemFuse components (Router, Reranker, Importance Scoring).

## Rules & Invariants
1. **Layer 1 Placement**: Depends only on `memfuse-core`.
2. **Zero-Unsafe**: `#![forbid(unsafe_code)]` compliance.
3. **P8 Compliance**: Invalidate calibration via `invalidate_on_config_change(new_fingerprint: ConfigFingerprint)`.
