//! Memory Importance scoring and Recency-Decay functionality.

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Memory Importance Scoring und Recency-Decay Berechnungen (ADR-025).
// INVARIANTEN: ImportanceScore ist garantiert [0.0, 1.0] geclampt; NaN wird zu 0.0 (Zero-Panic).
// HOTSPOTS: 15-120
// NICHT-OFFENSICHTLICH: Recency Decay basiert auf TxId-Distanz (nicht Wall-Clock Time).
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md (ADR-025)

//! # Invarianten
//! - `ImportanceScore` ist garantiert im Bereich `[0.0, 1.0]`. NaN wird zu `0.0`.
//! - `decay_factor` und `effective_score` sind reine, nicht-fehlschlagende Funktionen (Zero-Panic).

use crate::types::domain::TxId;
use serde::{Deserialize, Serialize};

/// Scores the importance of a memory item on a normalized scale of `0.0` to `1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImportanceScore(f32);

impl ImportanceScore {
    /// Creates a new `ImportanceScore` clamped strictly within `[0.0, 1.0]`.
    ///
    /// # Zero-Panic Guarantee
    /// NaN inputs return `ImportanceScore(0.0)`. Positive/negative infinity are clamped safely.
    pub fn new(raw: f32) -> Self {
        if raw.is_nan() {
            return Self(0.0);
        }
        Self(raw.clamp(0.0, 1.0))
    }

    /// Returns the raw `f32` importance score value.
    #[inline]
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Default for ImportanceScore {
    fn default() -> Self {
        Self(0.5)
    }
}

/// Recency-decay mathematical function for episodic relevance.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DecayFunction {
    /// No recency decay (factor remains 1.0).
    #[default]
    None,
    /// Exponential decay measured in `TxId` distance (not wall-clock time).
    Exponential {
        /// Half-life in transaction count.
        half_life_tx: u64,
    },
    /// Step decay function that steps down to a floor factor after a transaction threshold.
    StepFloor {
        /// Transaction distance threshold before step reduction.
        access_count_floor: u32,
    },
    /// Exponential decay measured in wall-clock seconds (bi-temporal decoupling).
    WallClockExponential {
        /// Half-life in elapsed wall-clock seconds.
        half_life_secs: u64,
    },
}

impl DecayFunction {
    /// Pure non-failing decay factor calculation.
    ///
    /// Returns a factor in `[0.0, 1.0]`.
    ///
    /// # Invarianten & Edge Cases
    /// - If `now_tx < created_at_tx` (e.g., due to snapshot reads or TxId rewind), returns `1.0`.
    /// - Never panics under any combination of TxId values or zero division.
    ///
    /// # Bi-Temporal Note
    /// For `Exponential` and `StepFloor`, decay is measured in TxId distance (mutation events),
    /// NOT wall-clock time. In systems with high background activity, TxIds advance rapidly
    /// causing premature decay. Use `WallClockExponential` for time-based decay.
    pub fn decay_factor(&self, created_at_tx: TxId, now_tx: TxId) -> f32 {
        let created_raw = created_at_tx.inner();
        let now_raw = now_tx.inner();

        if now_raw < created_raw {
            return 1.0;
        }

        let elapsed_tx = now_raw - created_raw;
        if elapsed_tx == 0 {
            return 1.0;
        }

        let factor = match self {
            Self::None => 1.0,
            Self::Exponential { half_life_tx } => {
                if *half_life_tx == 0 {
                    0.0
                } else {
                    0.5f32.powf(elapsed_tx as f32 / *half_life_tx as f32)
                }
            }
            Self::StepFloor { access_count_floor } => {
                if elapsed_tx < (*access_count_floor as u64) {
                    1.0
                } else {
                    0.5
                }
            }
            Self::WallClockExponential { .. } => {
                // Wall-clock decay cannot be computed from TxId alone.
                // Return 1.0 (no decay) — callers should use decay_factor_wallclock() instead.
                1.0
            }
        };

        if factor.is_nan() || !factor.is_finite() {
            0.0
        } else {
            factor.clamp(0.0, 1.0)
        }
    }

    /// Wall-clock based decay factor calculation.
    ///
    /// Returns a factor in `[0.0, 1.0]` based on elapsed wall-clock seconds.
    /// For non-WallClockExponential variants, delegates to the TxId-based decay_factor.
    ///
    /// # Arguments
    /// * `created_at_epoch_secs` - Unix timestamp (seconds) when the memory was created.
    /// * `now_epoch_secs` - Current Unix timestamp (seconds).
    /// * `created_at_tx` - TxId at creation time (used for TxId-based variants).
    /// * `now_tx` - Current TxId (used for TxId-based variants).
    pub fn decay_factor_wallclock(
        &self,
        created_at_epoch_secs: u64,
        now_epoch_secs: u64,
        created_at_tx: TxId,
        now_tx: TxId,
    ) -> f32 {
        match self {
            Self::WallClockExponential { half_life_secs } => {
                if now_epoch_secs <= created_at_epoch_secs {
                    return 1.0;
                }
                let elapsed = now_epoch_secs - created_at_epoch_secs;
                if elapsed == 0 || *half_life_secs == 0 {
                    return if *half_life_secs == 0 { 0.0 } else { 1.0 };
                }
                let factor = 0.5f32.powf(elapsed as f32 / *half_life_secs as f32);
                if factor.is_nan() || !factor.is_finite() {
                    0.0
                } else {
                    factor.clamp(0.0, 1.0)
                }
            }
            _ => self.decay_factor(created_at_tx, now_tx),
        }
    }
}

/// Tracks the importance score and decay parameters of a document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryImportance {
    /// Base LLM or heuristic evaluation score.
    pub base_score: ImportanceScore,
    /// Recency decay function strategy.
    pub decay: DecayFunction,
    /// Transaction ID when the memory item was created.
    pub created_at_tx: TxId,
}

impl MemoryImportance {
    /// Creates a new `MemoryImportance` configuration.
    pub fn new(base_score: ImportanceScore, decay: DecayFunction, created_at_tx: TxId) -> Self {
        Self {
            base_score,
            decay,
            created_at_tx,
        }
    }

    /// Computes the effective importance score: `base_score * decay_factor`.
    ///
    /// # Zero-Panic Guarantee
    /// Always returns a valid `f32` in `[0.0, 1.0]`.
    pub fn effective_score(&self, now_tx: TxId) -> f32 {
        let factor = self.decay.decay_factor(self.created_at_tx, now_tx);
        let effective = self.base_score.value() * factor;
        if effective.is_nan() || !effective.is_finite() {
            0.0
        } else {
            effective.clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance_score_clamping_and_nan() {
        assert_eq!(ImportanceScore::new(0.5).value(), 0.5);
        assert_eq!(ImportanceScore::new(1.5).value(), 1.0);
        assert_eq!(ImportanceScore::new(-0.2).value(), 0.0);
        assert_eq!(ImportanceScore::new(f32::NAN).value(), 0.0);
        assert_eq!(ImportanceScore::new(f32::INFINITY).value(), 1.0);
        assert_eq!(ImportanceScore::new(f32::NEG_INFINITY).value(), 0.0);
    }

    #[test]
    fn test_decay_factor_none() {
        let decay = DecayFunction::None;
        let created = TxId::new(10);
        let now = TxId::new(100);
        assert_eq!(decay.decay_factor(created, now), 1.0);
    }

    #[test]
    fn test_decay_factor_exponential() {
        let decay = DecayFunction::Exponential { half_life_tx: 10 };
        let created = TxId::new(10);

        // Same TxId -> 1.0
        assert_eq!(decay.decay_factor(created, TxId::new(10)), 1.0);
        // 1 half life elapsed -> 0.5
        assert_eq!(decay.decay_factor(created, TxId::new(20)), 0.5);
        // 2 half lives elapsed -> 0.25
        assert_eq!(decay.decay_factor(created, TxId::new(30)), 0.25);

        // Zero half life handling
        let zero_decay = DecayFunction::Exponential { half_life_tx: 0 };
        assert_eq!(zero_decay.decay_factor(created, TxId::new(20)), 0.0);
    }

    #[test]
    fn test_decay_factor_step_floor() {
        let decay = DecayFunction::StepFloor {
            access_count_floor: 5,
        };
        let created = TxId::new(10);

        // Before threshold
        assert_eq!(decay.decay_factor(created, TxId::new(12)), 1.0);
        assert_eq!(decay.decay_factor(created, TxId::new(14)), 1.0);

        // At / after threshold
        assert_eq!(decay.decay_factor(created, TxId::new(15)), 0.5);
        assert_eq!(decay.decay_factor(created, TxId::new(25)), 0.5);
    }

    #[test]
    fn test_decay_factor_out_of_order_tx() {
        let decay = DecayFunction::Exponential { half_life_tx: 10 };
        let created = TxId::new(50);
        let now = TxId::new(10); // now < created

        assert_eq!(decay.decay_factor(created, now), 1.0);
    }

    #[test]
    fn test_memory_importance_effective_score() {
        let importance = MemoryImportance::new(
            ImportanceScore::new(0.8),
            DecayFunction::Exponential { half_life_tx: 20 },
            TxId::new(100),
        );

        // now_tx = 120 (1 half life elapsed) -> 0.8 * 0.5 = 0.4
        let eff = importance.effective_score(TxId::new(120));
        assert!((eff - 0.4).abs() < 1e-5);
    }

    #[test]
    fn test_decay_factor_wall_clock_exponential() {
        let decay = DecayFunction::WallClockExponential {
            half_life_secs: 3600, // 1 hour half-life
        };
        let created_epoch = 1_700_000_000u64;
        let created_tx = TxId::new(10);
        let now_tx = TxId::new(100);

        // TxId-based decay_factor returns 1.0 (requires wall-clock time)
        assert_eq!(decay.decay_factor(created_tx, now_tx), 1.0);

        // Same time -> 1.0
        let f0 = decay.decay_factor_wallclock(created_epoch, created_epoch, created_tx, now_tx);
        assert_eq!(f0, 1.0);

        // 1 half-life elapsed (3600 secs) -> 0.5
        let f1 =
            decay.decay_factor_wallclock(created_epoch, created_epoch + 3600, created_tx, now_tx);
        assert!((f1 - 0.5).abs() < 1e-5);

        // 2 half-lives elapsed (7200 secs) -> 0.25
        let f2 =
            decay.decay_factor_wallclock(created_epoch, created_epoch + 7200, created_tx, now_tx);
        assert!((f2 - 0.25).abs() < 1e-5);

        // Out of order timestamps (now < created) -> 1.0
        let f_rev =
            decay.decay_factor_wallclock(created_epoch + 100, created_epoch, created_tx, now_tx);
        assert_eq!(f_rev, 1.0);
    }
}
