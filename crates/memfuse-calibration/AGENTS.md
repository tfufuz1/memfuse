# AGENTS.md — memfuse-calibration
> Layer 1 | Probability Calibration Primitives | ~500 LOC

## 1. Zweck & Architekturrolle

Implements non-parametric (`IsotonicCalibrator`) and parametric (`PlattScaler`) probability calibration primitives used across MemFuse components (Router, Reranker, Importance Scoring) to map raw model scores to true empirical probabilities.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![forbid(unsafe_code)]`, `IsotonicCalibrator`, `PlattScaler`, P8 configuration fingerprint validation |

## 3. Kritische Invarianten

### Layer 1 Placement
Depends only on `memfuse-core`. Must not import Layer 2 or higher crates.

### Zero-Unsafe
Complies with `#![forbid(unsafe_code)]`.

### Configuration Shift Invalidation (Principle P8)
Calibrators invalidate state via `invalidate_on_config_change(new_fingerprint: ConfigFingerprint)`.

## 4. Public API Quick-Reference

```rust
pub struct IsotonicCalibrator { ... }
impl IsotonicCalibrator {
    pub fn new(fingerprint: ConfigFingerprint) -> Self;
    pub fn fit(&mut self, predictions: &[f32], targets: &[bool]);
    pub fn calibrate(&self, score: f32) -> f32;
    pub fn invalidate_on_config_change(&mut self, new_fingerprint: ConfigFingerprint);
}

pub struct PlattScaler { ... }
impl PlattScaler {
    pub fn new(fingerprint: ConfigFingerprint) -> Self;
    pub fn fit(&mut self, predictions: &[f32], targets: &[bool]);
    pub fn calibrate(&self, score: f32) -> f32;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Calibrator ohne ConfigFingerprint verwenden oder Fingerprint-Änderungen ignorieren:
let mut calibrator = IsotonicCalibrator::default();
// ✅ KORREKT — Fingerprint übergeben und bei Konfigurationsänderung invalidieren:
calibrator.invalidate_on_config_change(new_fingerprint);
```

## 6. Concurrency & Lock-Hierarchie

`IsotonicCalibrator` und `PlattScaler` sind im-memory Datenstrukturen. Thread-Safety wird bei Bedarf von aufrufenden Schichten (z.B. via `Arc<Mutex<...>>` in `memfuse-ollama` oder `memfuse-router`) bereitgestellt.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (Layer 0)
- **Verbotene Imports**: `memfuse-store`, `memfuse-db`, `memfuse-router`, `memfuse-ollama`
- **Genutzt von**: `memfuse-ollama` (L2), `memfuse-router` (L4)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| Principle P8 | Invalidation on configuration shift (`ConfigFingerprint`) |
