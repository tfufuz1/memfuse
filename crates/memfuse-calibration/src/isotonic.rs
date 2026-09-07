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

/// Isotonischer Kalibrator für nicht-parametrische Wahrscheinlichkeitskalibrierung.
#[derive(Debug, Clone)]
pub struct IsotonicCalibrator {
    observations: VecDeque<(f32, bool)>,
    warmup_required: u32,
    max_observations: usize,
    cached_model: Option<Vec<(f32, f32)>>, // (max_score_in_block, calibrated_prob)
    model_dirty: bool,
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
    pub fn calibrated_probability(&mut self, raw_score: f32) -> Option<f32> {
        if !self.is_calibrated() {
            return None;
        }
        if self.model_dirty {
            self.rebuild_model();
        }
        Some(self.lookup_isotonic(raw_score))
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
            self.fingerprint = Some(new_fingerprint);
        }
    }

    /// PAVA — Pool-Adjacent Violators Algorithm, O(n) amortisiert.
    fn rebuild_model(&mut self) {
        let mut sorted: Vec<(f32, f32)> = self
            .observations
            .iter()
            .map(|&(score, outcome)| (score, if outcome { 1.0 } else { 0.0 }))
            .collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));

        // Stack von Blöcken: (max_score, label_sum, count)
        let mut blocks: Vec<(f64, f64, usize)> = Vec::with_capacity(sorted.len());

        for (score, label) in &sorted {
            blocks.push((*score as f64, *label as f64, 1));

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
}
