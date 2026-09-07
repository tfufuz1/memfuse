# AGENTS.md — memfuse-calibration

## 1. Zweck & Architekturrolle

Implements probability calibration primitives (`IsotonicCalibrator` and `PlattScaler`) used across MemFuse components (Router, Reranker, Importance Scoring).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Re-Exports und Modul-Deklarationen |
| `isotonic.rs` | `IsotonicCalibrator` Implementierung |
| `platt.rs` | `PlattScaler` Implementierung |

## 3. Kritische Invarianten

1. **Layer 1 Placement**: Depends only on `memfuse-core`.
2. **Zero-Unsafe**: `#![forbid(unsafe_code)]` compliance.
3. **P8 Compliance**: Invalidate calibration via `invalidate_on_config_change(new_fingerprint: ConfigFingerprint)`.

## 4. Public API Quick-Reference

`IsotonicCalibrator`, `PlattScaler`

## 5. Anti-Patterns & LLM-Fallstricke

Keine manuellen Kalibrierungen ohne Invalidation bei Konfigurationsänderung durchführen.

## 6. Concurrency & Lock-Hierarchie

Keine internen Locks. Thread-safety via Pure Functions / Mutexes in aufrufenden Schichten.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

Nutzt `memfuse-core` (Layer 0). Wird von `memfuse-router`, `memfuse-embed` und `memfuse-db` verwendet.

## 8. Relevante ADRs & Rules

ADR-068 (Calibration Engine), P8 Calibration Principle.
