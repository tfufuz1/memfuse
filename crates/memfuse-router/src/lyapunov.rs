//! Proaktiver Distributional-Drift-Wächter (Feature F-11) via Lyapunov-Exponenten.
//!
//! ARCHITEKTUR & ENTSCHEIDUNG:
//! - Reaktiv vs. Proaktiv: Während `SlmProfile.fingerprint` und `invalidate_on_config_change()`
//!   (P8) reaktiv auf explizite Konfigurations- und Modelländerungen reagieren, überwacht der
//!   `LyapunovDriftWatcher` proaktiv kontinuierliche Verteilungsverschiebungen (Distributional Drift)
//!   der Non-Conformity-Scores im laufenden Betrieb.
//! - Additive Nicht-Kollision: Der Lyapunov-Wächter arbeitet vollständig orthogonal und additiv zum
//!   Fingerprint-Mechanismus. Er erfordert keine Änderung an bestehenden Konfigurationsvalidierungen
//!   oder Fingerprint-Invalidationen.
//!
//! METHODISCHE GRUNDLAGE:
//! - Basiert auf arXiv:2605.18796 (UCCI) zur Drift-Erkennung in konformalen Vorhersagesystemen.
//! - Berechnet die KL-Divergenz D_t = KL(N_t || N_baseline) über ein 10-Bin-Histogramm der
//!   Non-Conformity-Scores und schätzt den diskreten Lyapunov-Exponenten λ_t über ein gleitendes Fenster.
//! - λ_t > 0.0 indiziert exponentielle Divergenz der Non-Conformity-Score-Verteilung von der Kalibrierungs-Baseline.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Grund für erkannte Verteilungsverschiebung (Distributional Drift).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriftReason {
    /// Aktuelle Kullback-Leibler-Divergenz D_t = KL(N_t || N_baseline).
    pub kl_divergence: f32,
    /// Diskreter Lyapunov-Exponent λ_t über das gleitende Fenster.
    pub lyapunov_exponent: f32,
}

/// Ergebnis der Lyapunov-Drift-Analyse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LyapunovResult {
    /// Die Score-Verteilung ist stabil (λ_t <= 0.0).
    Stable {
        /// Diskreter Lyapunov-Exponent λ_t.
        lyapunov_exponent: f32,
    },
    /// Proaktive Drift erkannt (λ_t > 0.0 über das Fenster).
    DriftDetected {
        /// Diskreter Lyapunov-Exponent λ_t.
        lyapunov_exponent: f32,
        /// Detailursache des Drifts.
        reason: DriftReason,
    },
    /// Noch nicht genügend Beobachtungen (< window_size) für eine verlässliche Schätzung.
    InsufficientData,
}

/// Proaktiver Drift-Wächter auf Basis diskreter Lyapunov-Exponenten über KL-Divergenzen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyapunovDriftWatcher {
    /// Fenstergröße w für die Lyapunov-Exponenten-Berechnung (Default: 20).
    pub window_size: usize,
    /// Historie der berechneten KL-Divergenzen D_t.
    pub divergence_history: VecDeque<f32>,
    /// Referenz-Verteilung (Non-Conformity-Scores) aus der Kalibrierungsphase.
    pub baseline_distribution: Vec<f32>,
    /// Neuestes Analyseergebnis.
    pub latest_result: Option<LyapunovResult>,
}

impl Default for LyapunovDriftWatcher {
    fn default() -> Self {
        Self::new(20)
    }
}

impl LyapunovDriftWatcher {
    /// Erstellt einen neuen `LyapunovDriftWatcher` mit der angegebenen Fenstergröße.
    pub fn new(window_size: usize) -> Self {
        let effective_window = window_size.max(1);
        let mut history = VecDeque::with_capacity(effective_window + 1);
        history.push_back(0.0); // D_0 Baseline-Anker
        Self {
            window_size: effective_window,
            divergence_history: history,
            baseline_distribution: Vec::new(),
            latest_result: None,
        }
    }

    /// Setzt die Baseline-Verteilung der Non-Conformity-Scores aus dem Kalibrierungs-Warmup.
    pub fn set_baseline(&mut self, baseline_scores: &[f32]) {
        self.baseline_distribution = baseline_scores.to_vec();
        self.divergence_history.clear();
        self.divergence_history.push_back(0.0);
        self.latest_result = None;
    }

    /// Nimmt aktuelle Non-Conformity-Scores auf und berechnet den Drift-Status.
    pub fn update(&mut self, current_scores: &[f32]) -> LyapunovResult {
        if self.baseline_distribution.is_empty() {
            if current_scores.is_empty() {
                let res = LyapunovResult::InsufficientData;
                self.latest_result = Some(res.clone());
                return res;
            }
            // Automatisches Sammeln von Baseline-Scores wenn noch keine Baseline gesetzt wurde
            self.baseline_distribution.extend_from_slice(current_scores);
            if self.baseline_distribution.len() < 30 {
                let res = LyapunovResult::InsufficientData;
                self.latest_result = Some(res.clone());
                return res;
            }
        }

        if current_scores.is_empty() {
            return self
                .latest_result
                .clone()
                .unwrap_or(LyapunovResult::InsufficientData);
        }

        // 1. 10-Bin Histogramm Approximation der Verteilungen
        let mut current_counts = [0usize; 10];
        let mut baseline_counts = [0usize; 10];

        for &score in current_scores {
            let bin = ((score.clamp(0.0, 1.0) * 10.0) as usize).min(9);
            current_counts[bin] += 1;
        }

        for &score in &self.baseline_distribution {
            let bin = ((score.clamp(0.0, 1.0) * 10.0) as usize).min(9);
            baseline_counts[bin] += 1;
        }

        // 2. KL-Divergenz D_t = KL(N_t || N_baseline) mit Laplace/Epsilon Smoothing
        let eps = 1e-10f32;
        let n_curr = current_scores.len() as f32;
        let n_base = self.baseline_distribution.len() as f32;

        let mut d_t = 0.0f32;
        for i in 0..10 {
            let p_i = (current_counts[i] as f32 + eps) / (n_curr + 10.0 * eps);
            let q_i = (baseline_counts[i] as f32 + eps) / (n_base + 10.0 * eps);
            d_t += p_i * (p_i / q_i).ln();
        }
        let d_t = d_t.max(0.0);

        self.divergence_history.push_back(d_t);

        if self.divergence_history.len() > self.window_size + 1 {
            self.divergence_history.pop_front();
        }

        // 3. Auswertung nach window_size Beobachtungs-Verhältnissen
        if self.divergence_history.len() <= self.window_size {
            let res = LyapunovResult::InsufficientData;
            self.latest_result = Some(res.clone());
            return res;
        }

        // 4. Diskrete Lyapunov-Exponent-Schätzung: λ_t = (1/w) * Σ log|D_{t-i+1}/D_{t-i}|
        let w = self.window_size;
        let mut sum_log_ratio = 0.0f32;

        for i in 1..=w {
            let num = self
                .divergence_history
                .get(i)
                .copied()
                .unwrap_or(0.0);
            let den = self
                .divergence_history
                .get(i - 1)
                .copied()
                .unwrap_or(0.0)
                .max(1e-10);

            let ratio = (num / den).abs().max(1e-10);
            sum_log_ratio += ratio.ln();
        }

        let lyapunov_exponent = sum_log_ratio / (w as f32);

        let result = if lyapunov_exponent > 0.0 {
            LyapunovResult::DriftDetected {
                lyapunov_exponent,
                reason: DriftReason {
                    kl_divergence: d_t,
                    lyapunov_exponent,
                },
            }
        } else {
            LyapunovResult::Stable { lyapunov_exponent }
        };

        self.latest_result = Some(result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insufficient_data_for_first_window_updates() {
        let window_size = 20;
        let mut watcher = LyapunovDriftWatcher::new(window_size);
        let baseline: Vec<f32> = (0..100).map(|i| (i as f32) / 100.0).collect();
        watcher.set_baseline(&baseline);

        // Die ersten window_size - 1 Updates müssen InsufficientData liefern
        for _ in 0..(window_size - 1) {
            let res = watcher.update(&[0.1, 0.2, 0.3]);
            assert_eq!(res, LyapunovResult::InsufficientData);
        }
    }

    #[test]
    fn test_stable_distribution_yields_stable_result() {
        let window_size = 20;
        let mut watcher = LyapunovDriftWatcher::new(window_size);
        let baseline: Vec<f32> = (0..100).map(|i| (i as f32) / 100.0).collect();
        watcher.set_baseline(&baseline);

        let mut last_res = LyapunovResult::InsufficientData;
        // Mehr als window_size Updates aus der exakt gleichen Verteilung
        for i in 0..30 {
            let sample: Vec<f32> = (0..50).map(|j| ((i + j) % 100) as f32 / 100.0).collect();
            last_res = watcher.update(&sample);
        }

        match last_res {
            LyapunovResult::Stable { lyapunov_exponent } => {
                assert!(
                    lyapunov_exponent <= 0.01,
                    "Lyapunov exponent {lyapunov_exponent} should be <= 0.01 for stable distribution"
                );
            }
            other => panic!("Expected Stable, got {:?}", other),
        }
    }

    #[test]
    fn test_artificially_shifted_distribution_triggers_drift_detected() {
        let window_size = 20;
        let mut watcher = LyapunovDriftWatcher::new(window_size);
        // Baseline mit niedrigen Scores in [0.0, 0.2]
        let baseline: Vec<f32> = (0..100).map(|i| (i as f32 / 100.0) * 0.2).collect();
        watcher.set_baseline(&baseline);

        let mut last_res = LyapunovResult::InsufficientData;
        // Kontinuierlich ansteigender Drift hin zu hohen Scores [0.8, 1.0]
        for i in 0..25 {
            let shift = (i as f32 / 25.0) * 0.8;
            let current: Vec<f32> = (0..50)
                .map(|j| ((j as f32 / 50.0) * 0.2 + shift).clamp(0.0, 1.0))
                .collect();
            last_res = watcher.update(&current);
        }

        match last_res {
            LyapunovResult::DriftDetected {
                lyapunov_exponent,
                reason,
            } => {
                assert!(lyapunov_exponent > 0.0);
                assert!(reason.kl_divergence > 0.0);
            }
            other => panic!("Expected DriftDetected, got {:?}", other),
        }
    }

    #[test]
    fn test_zero_divergence_denominator_edge_case_no_panic() {
        let mut watcher = LyapunovDriftWatcher::new(10);
        // Vorbelegen mit identischem Wert (0.0 Divergenz)
        watcher.divergence_history.clear();
        for _ in 0..=10 {
            watcher.divergence_history.push_back(0.0);
        }
        let baseline: Vec<f32> = vec![0.5; 50];
        watcher.baseline_distribution = baseline.clone();

        // Update mit identischen Scores -> Darf nicht panicken oder NaN erzeugen
        let res = watcher.update(&baseline);
        assert!(matches!(res, LyapunovResult::Stable { .. }));
    }
}
