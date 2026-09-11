// FILE-CONTEXT
// STAND: 2026-09-10T19:29:31Z (SESSION: c9240483)
// ZWECK: Non-parametric probability calibration via PAVA (Pool-Adjacent Violators Algorithm).
// INVARIANTEN: INV-CAL-1 (returns None before warmup), INV-CAL-2 (resets observations on fingerprint change).
// NICHT-OFFENSICHTLICH: Pre-aggregates observations with identical raw scores prior to PAVA block merging.
// SIEHE AUCH: crates/memfuse-calibration/src/platt.rs, crates/memfuse-calibration/src/lib.rs

//! Isotonische Kalibrierung via PAVA (Pool-Adjacent Violators Algorithm).
//!
//! KOMPLEXITÄT: O(n) amortisiert (NICHT O(n log n) — Spec-Fehler korrigiert).
//! WANN: Score-Verteilung unbekannt oder multimodal.
//! WANN NICHT: Bekannte Sigmoid-Verteilung (→ PlattScaler, O(1) Inference).
//!
//! INVARIANTE INV-CAL-1: calibrated_probability() → None wenn Warmup nicht erreicht.
//! KEIN stiller 0.5-Fallback.
//! INVARIANTE INV-CAL-2: invalidate_on_config_change() setzt Observations auf 0.
//! Kein partielles Übernehmen alter Samples.

use memfuse_core::ConfigFingerprint;
use std::collections::VecDeque;

const ECE_BINS: usize = 10;
const DEFAULT_WARMUP_REQUIRED: u32 = 50;
const DEFAULT_MAX_OBSERVATIONS: usize = 2000;
const REBUILD_THRESHOLD_NEW_OBS: usize = 10;

/// Isotonischer Kalibrator für nicht-parametrische Wahrscheinlichkeitskalibrierung.
#[derive(Debug, Clone)]
pub struct IsotonicCalibrator {
    observations: VecDeque<(f32, bool)>,
    warmup_required: u32,
    max_observations: usize,
    cached_model: Option<Vec<(f32, f32)>>, // (max_score_in_block, calibrated_prob)
    model_dirty: bool,
    observations_since_rebuild: usize,
    fingerprint: Option<ConfigFingerprint>,
}

impl IsotonicCalibrator {
    /// Erstellt einen neuen `IsotonicCalibrator` mit angegebenem Warmup und Beobachtungsfenster.
    pub fn new(warmup_required: u32, max_observations: usize) -> Self {
        Self {
            observations: VecDeque::new(),
            warmup_required,
            max_observations,
            cached_model: None,
            model_dirty: true,
            observations_since_rebuild: 0,
            fingerprint: None,
        }
    }

    /// Erstellt einen `IsotonicCalibrator` mit Standardwerten (Warmup: 50, Max Obs: 2000).
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_WARMUP_REQUIRED, DEFAULT_MAX_OBSERVATIONS)
    }

    /// Zeichnet eine neue Beobachtung `(raw_score, outcome)` auf.
    pub fn record_outcome(&mut self, raw_score: f32, outcome: bool) {
        if self.observations.len() >= self.max_observations {
            self.observations.pop_front();
        }
        self.observations.push_back((raw_score, outcome));
        self.model_dirty = true;
        self.observations_since_rebuild += 1;
    }

    /// Gibt die Anzahl der aktuell gespeicherten Beobachtungen zurück.
    pub fn observation_count(&self) -> usize {
        self.observations.len()
    }

    /// Prüft, ob genügend Beobachtungen für eine kalibrierte Ausgabe vorliegen.
    pub fn is_calibrated(&self) -> bool {
        self.observations.len() as u32 >= self.warmup_required
    }

    /// Kalibrierte Wahrscheinlichkeit.
    /// INVARIANTE INV-CAL-1: None wenn Warmup nicht erreicht. Kein 0.5-Fallback.
    /// Debounced Rebuild: Rebuild erfolgt erst nach `REBUILD_THRESHOLD_NEW_OBS` neuen Beobachtungen.
    pub fn calibrated_probability(&mut self, raw_score: f32) -> Option<f32> {
        if !self.is_calibrated() {
            return None;
        }
        if self.model_dirty
            && (self.cached_model.is_none()
                || self.observations_since_rebuild >= REBUILD_THRESHOLD_NEW_OBS)
        {
            self.rebuild_model();
        }
        Some(self.lookup_isotonic(raw_score))
    }

    /// Erzwingt ein sofortiges Rebuild des PAVA-Modells, unabhängig vom Threshold für neue Beobachtungen.
    pub fn force_rebuild(&mut self) {
        if self.model_dirty {
            self.rebuild_model();
        }
    }

    /// P8-PFLICHT: Vollständiger Reset bei Fingerprint-Änderung.
    /// INVARIANTE INV-CAL-2: observations wird auf 0 gesetzt. Kein partielles Übernehmen.
    pub fn invalidate_on_config_change(&mut self, new_fingerprint: ConfigFingerprint) {
        if self.fingerprint.as_ref() != Some(&new_fingerprint) {
            tracing::warn!(
                old_fp = ?self.fingerprint,
                new_fp = ?new_fingerprint,
                obs_count = self.observations.len(),
                "IsotonicCalibrator: ConfigFingerprint changed — resetting (P8)"
            );
            self.observations.clear();
            self.cached_model = None;
            self.model_dirty = true;
            self.observations_since_rebuild = 0;
            self.fingerprint = Some(new_fingerprint);
        }
    }

    // RESOLVED: AGT-CALIBRATION-16f90c35 — pre-aggregate observations with identical raw_scores in rebuild_model() before PAVA pooling (TS: 2026-09-09T14:46:37Z) (SESSION: 74eb6216)
    /// PAVA — Pool-Adjacent Violators Algorithm, O(n) amortisiert.
    fn rebuild_model(&mut self) {
        let mut sorted: Vec<(f32, f32)> = self
            .observations
            .iter()
            .map(|&(score, outcome)| (score, if outcome { 1.0 } else { 0.0 }))
            .collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut aggregated: Vec<(f64, f64, usize)> = Vec::with_capacity(sorted.len());
        for (score, label) in sorted {
            if let Some(last) = aggregated.last_mut() {
                if (last.0 as f32).total_cmp(&score) == std::cmp::Ordering::Equal {
                    last.1 += label as f64;
                    last.2 += 1;
                    continue;
                }
            }
            aggregated.push((score as f64, label as f64, 1));
        }

        // Stack von Blöcken: (max_score, label_sum, count)
        let mut blocks: Vec<(f64, f64, usize)> = Vec::with_capacity(aggregated.len());

        for (score, label_sum, count) in aggregated {
            blocks.push((score, label_sum, count));

            while blocks.len() >= 2 {
                let n = blocks.len();
                let last_avg = blocks[n - 1].1 / blocks[n - 1].2 as f64;
                let prev_avg = blocks[n - 2].1 / blocks[n - 2].2 as f64;
                if last_avg <= prev_avg {
                    // Monotonie verletzt → merge
                    if let Some(last) = blocks.pop() {
                        if let Some(prev) = blocks.last_mut() {
                            prev.0 = last.0;
                            prev.1 += last.1;
                            prev.2 += last.2;
                        }
                    }
                } else {
                    break;
                }
            }
        }

        self.cached_model = Some(
            blocks
                .iter()
                .map(|(score, label_sum, count)| {
                    (*score as f32, (label_sum / *count as f64) as f32)
                })
                .collect(),
        );
        self.model_dirty = false;
        self.observations_since_rebuild = 0;
    }

    fn lookup_isotonic(&self, raw_score: f32) -> f32 {
        let model = match &self.cached_model {
            Some(m) => m,
            None => return 0.5,
        };
        if model.is_empty() {
            return 0.5;
        }

        match model.binary_search_by(|(threshold, _)| threshold.total_cmp(&raw_score)) {
            Ok(idx) => model[idx].1,
            Err(idx) => {
                if idx >= model.len() {
                    model.last().map(|(_, p)| *p).unwrap_or(0.5)
                } else {
                    model[idx].1
                }
            }
        }
    }

    /// Expected Calibration Error über M=10 gleichbreite Bins.
    /// Ziel: ECE < 0.03 (arXiv:2605.18796).
    // RESOLVED: AGT-CALIBRATION-b4b9ce8f — cached fitted PAVA step function with debounced dirty flag rebuilding (TS: 2026-09-10T23:45:00Z)
    pub fn expected_calibration_error(&mut self) -> Option<f32> {
        if !self.is_calibrated() {
            return None;
        }
        if self.model_dirty {
            self.rebuild_model();
        }

        let n = self.observations.len() as f32;
        if n == 0.0 {
            return Some(0.0);
        }

        let probs_and_outcomes: Vec<(f32, bool)> = self
            .observations
            .iter()
            .map(|&(score, outcome)| (self.lookup_isotonic(score), outcome))
            .collect();

        let bin_width = 1.0 / ECE_BINS as f32;
        let mut ece = 0.0f32;

        for bin_idx in 0..ECE_BINS {
            let lo = bin_idx as f32 * bin_width;
            let hi = lo + bin_width;

            let bin_obs: Vec<(f32, bool)> = probs_and_outcomes
                .iter()
                .filter(|&&(prob, _)| {
                    prob >= lo && (prob < hi || (bin_idx == ECE_BINS - 1 && prob <= hi))
                })
                .copied()
                .collect();

            if bin_obs.is_empty() {
                continue;
            }

            let bin_n = bin_obs.len() as f32;
            let avg_confidence = bin_obs.iter().map(|(p, _)| p).sum::<f32>() / bin_n;
            let avg_accuracy = bin_obs.iter().filter(|(_, o)| *o).count() as f32 / bin_n;
            ece += (bin_n / n) * (avg_confidence - avg_accuracy).abs();
        }
        Some(ece)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inv_cal1_no_fallback_before_warmup() {
        let mut cal = IsotonicCalibrator::new(50, 2000);
        for i in 0..49 {
            cal.record_outcome(i as f32 / 100.0, i % 2 == 0);
        }
        // INV-CAL-1: None, kein 0.5-Fallback
        assert!(cal.calibrated_probability(0.5).is_none());
    }

    #[test]
    fn test_calibration_available_after_warmup() {
        let mut cal = IsotonicCalibrator::new(10, 2000);
        for i in 0..10 {
            cal.record_outcome(i as f32 / 10.0, i > 5);
        }
        assert!(cal.calibrated_probability(0.5).is_some());
    }

    #[test]
    fn test_inv_cal2_invalidate_clears_observations() {
        let mut cal = IsotonicCalibrator::new(5, 2000);
        for _ in 0..10 {
            cal.record_outcome(0.5, true);
        }
        assert_eq!(cal.observation_count(), 10);

        let fp = ConfigFingerprint::new("m", "Q4", "t", 0.7);
        cal.invalidate_on_config_change(fp);
        // INV-CAL-2: komplett gecleart
        assert_eq!(cal.observation_count(), 0);
        assert!(!cal.is_calibrated());
    }

    #[test]
    fn test_same_fingerprint_no_invalidation() {
        let mut cal = IsotonicCalibrator::new(5, 2000);
        for _ in 0..10 {
            cal.record_outcome(0.5, true);
        }
        let fp = ConfigFingerprint::new("m", "Q4", "t", 0.7);
        cal.invalidate_on_config_change(fp.clone());
        let count_after_first = cal.observation_count();
        cal.invalidate_on_config_change(fp); // gleicher FP → kein Reset
        assert_eq!(cal.observation_count(), count_after_first);
    }

    #[test]
    fn test_ece_binary_signal_below_threshold() {
        // Synthetisches perfekt-kalibriertes Signal: score ≈ outcome-rate
        let mut cal = IsotonicCalibrator::new(50, 2000);
        for i in 0..200 {
            let score = (i as f32) / 200.0;
            let outcome = (i as f32 / 200.0) > 0.5;
            cal.record_outcome(score, outcome);
        }
        let ece = cal.expected_calibration_error().unwrap();
        assert!(
            ece < 0.10,
            "ECE = {ece} sollte < 0.10 für synthetisches Signal"
        );
    }

    #[test]
    fn test_pava_duplicate_scores_deterministic() {
        let mut cal = IsotonicCalibrator::new(5, 2000);
        let obs = vec![
            (0.5, true),
            (0.5, false),
            (0.5, true),
            (0.5, false),
            (0.5, true),
            (0.8, true),
        ];
        for (score, outcome) in obs {
            cal.record_outcome(score, outcome);
        }
        let prob = cal.calibrated_probability(0.5).unwrap();
        assert!((prob - 0.6).abs() < 1e-5, "Expected 0.6, got {prob}");
    }

    #[test]
    fn test_pava_monotone_output() {
        let mut cal = IsotonicCalibrator::new(5, 2000);
        // Nicht-monotone Inputs → PAVA muss monotone Ausgabe erzeugen
        let test_cases = vec![
            (0.1, false),
            (0.2, true),
            (0.15, false),
            (0.8, true),
            (0.9, true),
            (0.3, false),
            (0.7, true),
        ];
        for (score, outcome) in test_cases {
            cal.record_outcome(score, outcome);
        }
        // Ausgabe muss monoton nicht-fallend sein
        let scores: Vec<f32> = vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
        let probs: Vec<f32> = scores
            .iter()
            .map(|&s| cal.calibrated_probability(s).unwrap_or(0.0))
            .collect();
        for window in probs.windows(2) {
            assert!(
                window[0] <= window[1] + 1e-6,
                "PAVA-Ausgabe nicht monoton: {:?}",
                probs
            );
        }
    }

    #[test]
    fn test_with_defaults_initialization() {
        let cal = IsotonicCalibrator::with_defaults();
        assert_eq!(cal.warmup_required, DEFAULT_WARMUP_REQUIRED);
        assert_eq!(cal.max_observations, DEFAULT_MAX_OBSERVATIONS);
        assert_eq!(cal.observation_count(), 0);
        assert!(!cal.is_calibrated());
    }

    #[test]
    fn test_pava_identical_raw_score_conflicting_outcomes_is_deterministic() {
        let mut cal = IsotonicCalibrator::new(4, 100);
        // Order A: (0.5, true) then (0.5, false)
        cal.record_outcome(0.1, false);
        cal.record_outcome(0.3, false);
        cal.record_outcome(0.5, true);
        cal.record_outcome(0.5, false);
        cal.record_outcome(0.7, true);
        cal.record_outcome(0.9, true);

        let result_order_a = cal.calibrated_probability(0.5);
        assert!(result_order_a.is_some());

        // Expected value calculation:
        // Pre-aggregation merges (0.5, true) and (0.5, false) into a single point (0.5, label_sum=1.0, count=2)
        // with average outcome = 1.0 / 2.0 = 0.5.
        // PAVA forms monotonic blocks: [0.0 (count 2), 0.5 (count 2), 1.0 (count 2)].
        // Lookup at raw_score 0.5 hits exact threshold 0.5 with calibrated prob 0.5.
        let prob_a = result_order_a.unwrap();
        assert!(
            (prob_a - 0.5).abs() < 1e-6,
            "Expected calibrated prob 0.5 for score 0.5 with 1 true and 1 false, got {prob_a}"
        );
    }

    #[test]
    fn test_pava_identical_raw_score_reversed_insertion_order_matches() {
        // Order A: (0.5, true) then (0.5, false)
        let mut cal_a = IsotonicCalibrator::new(4, 100);
        cal_a.record_outcome(0.1, false);
        cal_a.record_outcome(0.3, false);
        cal_a.record_outcome(0.5, true);
        cal_a.record_outcome(0.5, false);
        cal_a.record_outcome(0.7, true);
        cal_a.record_outcome(0.9, true);

        // Order B: (0.5, false) then (0.5, true) - reversed order of identical scores
        let mut cal_b = IsotonicCalibrator::new(4, 100);
        cal_b.record_outcome(0.1, false);
        cal_b.record_outcome(0.3, false);
        cal_b.record_outcome(0.5, false);
        cal_b.record_outcome(0.5, true);
        cal_b.record_outcome(0.7, true);
        cal_b.record_outcome(0.9, true);

        let result_order_a = cal_a.calibrated_probability(0.5).unwrap();
        let result_order_b = cal_b.calibrated_probability(0.5).unwrap();

        // Insertion order of conflicting outcomes for identical raw scores must not affect output.
        assert!(
            (result_order_a - result_order_b).abs() < 1e-6,
            "Order A ({result_order_a}) and Order B ({result_order_b}) must match"
        );

        // Expected calculation:
        // Pre-aggregation merges both (0.5, true/false) observations into (0.5, sum=1.0, count=2),
        // giving avg = 1.0 / 2 = 0.5.
        assert!(
            (result_order_b - 0.5).abs() < 1e-6,
            "Expected 0.5, got {result_order_b}"
        );
    }

    #[test]
    fn test_pava_three_identical_scores_mixed_outcomes_deterministic() {
        // Permutation 1: (0.7, true), (0.7, true), (0.7, false)
        let mut cal1 = IsotonicCalibrator::new(4, 100);
        cal1.record_outcome(0.1, false);
        cal1.record_outcome(0.3, false);
        cal1.record_outcome(0.7, true);
        cal1.record_outcome(0.7, true);
        cal1.record_outcome(0.7, false);
        cal1.record_outcome(0.9, true);

        // Permutation 2: (0.7, false), (0.7, true), (0.7, true)
        let mut cal2 = IsotonicCalibrator::new(4, 100);
        cal2.record_outcome(0.1, false);
        cal2.record_outcome(0.3, false);
        cal2.record_outcome(0.7, false);
        cal2.record_outcome(0.7, true);
        cal2.record_outcome(0.7, true);
        cal2.record_outcome(0.9, true);

        let prob1 = cal1.calibrated_probability(0.7).unwrap();
        let prob2 = cal2.calibrated_probability(0.7).unwrap();

        assert!(
            (prob1 - prob2).abs() < 1e-6,
            "Permutation 1 ({prob1}) and Permutation 2 ({prob2}) must match"
        );

        // Expected calculation:
        // Pre-aggregation merges three 0.7 observations (2 true, 1 false) into (0.7, sum=2.0, count=3),
        // giving avg = 2.0 / 3.0 = 0.6666667.
        let expected = 2.0 / 3.0;
        assert!(
            (prob1 - expected).abs() < 1e-6,
            "Expected {expected}, got {prob1}"
        );
    }

    #[test]
    fn test_debounced_rebuild_below_threshold_returns_old_model() {
        let mut cal = IsotonicCalibrator::new(10, 2000);
        // Record 10 initial observations and compute initial model.
        for i in 0..10 {
            cal.record_outcome(i as f32 / 10.0, i >= 5);
        }
        let initial_prob = cal.calibrated_probability(0.5).unwrap();

        // Add 5 (< 10) new observations that strongly shift raw score 0.5 towards false (0.0).
        for _ in 0..5 {
            cal.record_outcome(0.5, false);
        }

        // Before threshold is reached, calibrated_probability should use the cached old model.
        let prob_below_threshold = cal.calibrated_probability(0.5).unwrap();
        assert_eq!(
            prob_below_threshold, initial_prob,
            "Probability should remain old model result when below rebuild threshold"
        );
    }

    #[test]
    fn test_debounced_rebuild_at_threshold_triggers_automatic_rebuild() {
        let mut cal = IsotonicCalibrator::new(10, 2000);
        for i in 0..10 {
            cal.record_outcome(i as f32 / 10.0, i >= 5);
        }
        let initial_prob = cal.calibrated_probability(0.5).unwrap();

        // Add exactly 10 new observations (reaching REBUILD_THRESHOLD_NEW_OBS) with false outcomes.
        for _ in 0..10 {
            cal.record_outcome(0.5, false);
        }

        // Now calibrated_probability must trigger an automatic rebuild and yield updated score.
        let prob_at_threshold = cal.calibrated_probability(0.5).unwrap();
        assert_ne!(
            prob_at_threshold, initial_prob,
            "Model should automatically rebuild when reaching rebuild threshold"
        );
    }

    #[test]
    fn test_force_rebuild_overrides_threshold() {
        let mut cal = IsotonicCalibrator::new(10, 2000);
        for i in 0..10 {
            cal.record_outcome(i as f32 / 10.0, i >= 5);
        }
        let initial_prob = cal.calibrated_probability(0.5).unwrap();

        // Add 3 (< 10) new observations.
        for _ in 0..3 {
            cal.record_outcome(0.5, false);
        }

        // Explicit force_rebuild() forces rebuild immediately.
        cal.force_rebuild();

        let prob_after_forced = cal.calibrated_probability(0.5).unwrap();
        assert_ne!(
            prob_after_forced, initial_prob,
            "force_rebuild() should rebuild model immediately regardless of threshold counter"
        );
    }
}
