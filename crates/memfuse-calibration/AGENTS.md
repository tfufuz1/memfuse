# AGENTS.md — memfuse-calibration
> Layer 1 | Probability & Score Calibration | ~600 LOC

## 1. Zweck & Architekturrolle

Score- und Wahrscheinlichkeitskalibrierungs-Primitives für MemFuse (`IsotonicCalibrator`, `PlattScaler`, `PidController`).
Wird von höheren Schichten genutzt (Router, Reranker, Importance Scoring), um Roh-Scores auf kalibrierte
Intervalle $[0.0, 1.0]$ zu mappen sowie Reranking-Pool-Größen dynamisch zu regeln.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklarationen, Re-Exports, `#![deny(unsafe_code)]`, `#![warn(missing_docs)]` |
| `isotonic.rs` | `IsotonicCalibrator` — Nicht-parametrisches PAVA (Pool Adjacent Violators Algorithm) Fitting |
| `platt.rs` | `PlattScaler` — Parametrische Sigmoid-Regression (Platt Scaling) für Logits |
| `pid.rs` | `PidController` — F-08 PID-Regler für Reranking-Kandidatenpool-Größe mit Anti-Windup |

## 3. Kritische Invarianten

### Layer-1-Garantie
Importiert ausschließlich `memfuse-core`. Besitzt **keine** I/O, Dateisystem- oder Netzwerkabhängigkeiten.

### Zero-Unsafe
Standardmäßig `#![deny(unsafe_code)]` erzwingen.

### P8 / INV-CAL Compliance
Kalibrierungen müssen bei Modell- oder Konfigurationsänderungen via `invalidate_on_config_change(new_fingerprint)` ungültig gemacht werden.

### PID-Homeostase (F-08)
`PidController` hält Reranking-Latenz auf `target_latency_ms`. Integral-Term wird strikt auf `[-max_integral, max_integral]` geclippt (Anti-Windup), Pool-Größe bleibt in `[min_pool_size, max_pool_size]`.

## 4. Public API Quick-Reference

```rust
pub struct IsotonicCalibrator;
pub struct PlattScaler;
pub struct PidController {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub target_latency_ms: f32,
    pub min_pool_size: usize,
    pub max_pool_size: usize,
    pub current_pool_size: Option<usize>,
}

impl PidController {
    pub fn update(&mut self, current_pool_size: usize, measured_latency_ms: f32) -> usize;
    pub fn reset(&mut self);
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ Integral-Overflow ohne Clipping:
self.integral += error; // Kann bei dauerhafter Latenz-Abweichung explodieren

// ✅ KORREKT (Anti-Windup):
self.integral = (self.integral + error).clamp(-self.max_integral, self.max_integral);
```

## 6. Concurrency & Lock-Hierarchie

`memfuse-calibration` ist stateless/pure data structures und besitzt keine internen Lock-Mechanismen.
Synchronization (z.B. `Arc<parking_lot::Mutex<PidController>>`) erfolgt in konsumierenden Schichten (`memfuse-db`).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core`, `serde`, `parking_lot`, `tracing`.
- **Konsumenten**: `memfuse-db`, `memfuse-embed`, `memfuse-router`.

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| `rules/error-handling.md` | Fehlerpropagierung über `memfuse-core::MemFuseError` |
| ADR-070 | Conformal Score Calibration Standard |
