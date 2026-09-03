//! Profile definition for Small Language Models (SLMs) in MemFuse Router.

use memfuse_core::{MemFuseError, Result, TokenBudget};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn serialize_sorted_set<S>(set: &HashSet<u64>, s: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut v: Vec<u64> = set.iter().copied().collect();
    v.sort_unstable();
    v.serialize(s)
}

fn deserialize_set<'de, D>(d: D) -> std::result::Result<HashSet<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<u64>::deserialize(d).map(|v| v.into_iter().collect())
}

/// Represents a Small Language Model (SLM) target and its domain expertise parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlmProfile {
    /// Identifier name of the SLM (e.g. "coding-slm", "docs-slm").
    pub name: String,
    /// MCP endpoint address or URI for client communication.
    pub mcp_endpoint: String,
    /// Set of graph community IDs this SLM is domain-responsible for.
    #[serde(
        serialize_with = "serialize_sorted_set",
        deserialize_with = "deserialize_set"
    )]
    pub domain_communities: HashSet<u64>,
    /// Token budget configuration for prompt context trimming.
    pub token_budget: TokenBudget,
    /// Minimum relevance threshold score required for routing candidates.
    pub min_relevance_score: f32,
}

impl SlmProfile {
    /// Creates a new `SlmProfile`.
    pub fn new(
        name: impl Into<String>,
        mcp_endpoint: impl Into<String>,
        domain_communities: impl IntoIterator<Item = u64>,
        token_budget: TokenBudget,
        min_relevance_score: f32,
    ) -> Self {
        Self {
            name: name.into(),
            mcp_endpoint: mcp_endpoint.into(),
            domain_communities: domain_communities.into_iter().collect(),
            token_budget,
            min_relevance_score,
        }
    }

    /// Validates `SlmProfile` parameters.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "SLM profile name cannot be empty".to_string(),
            ));
        }
        if self.mcp_endpoint.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "MCP endpoint cannot be empty".to_string(),
            ));
        }
        if !self.min_relevance_score.is_finite() || self.min_relevance_score < 0.0 {
            return Err(MemFuseError::InvalidInput(
                "min_relevance_score must be finite and non-negative".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates and validates a new `SlmProfile`.
    pub fn try_new(
        name: impl Into<String>,
        mcp_endpoint: impl Into<String>,
        domain_communities: impl IntoIterator<Item = u64>,
        token_budget: TokenBudget,
        min_relevance_score: f32,
    ) -> Result<Self> {
        let profile = Self::new(
            name,
            mcp_endpoint,
            domain_communities,
            token_budget,
            min_relevance_score,
        );
        profile.validate()?;
        Ok(profile)
    }
}

/// Online Conformal Calibrator based on quantile-based threshold adaptation
/// (Gibbs & Candès, 2021). Provides distribution-free coverage guarantees
/// without assumptions about the underlying score distribution.
///
/// The threshold `quantile_threshold` is updated via:
///   q_{t+1} = q_t + gamma * (alpha - I(s_t > q_t))
///
/// This guarantees that the empirical error rate converges to `alpha`
/// regardless of the distribution shift in local SLM confidence scores.
///
/// # Invariants
/// - INV-ROUTER-1: `quantile_threshold` is always in `[0.0, 1.0]`.
/// - INV-ROUTER-2: Given identical seed + query history, routing is deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformalCalibrator {
    /// Target error rate (e.g., 0.05 for 95% coverage).
    pub alpha: f32,
    /// Adaptation rate for online quantile updates.
    pub gamma: f32,
    /// Current calibrated confidence threshold q_t.
    /// INV-ROUTER-1: Always clamped to [0.0, 1.0].
    pub quantile_threshold: f32,
    /// Number of errors observed in the current window.
    pub window_errors: u64,
    /// Total observations in the current window.
    pub window_total: u64,
}

impl Default for ConformalCalibrator {
    fn default() -> Self {
        Self {
            alpha: 0.05,
            gamma: 0.01,
            quantile_threshold: 0.5,
            window_errors: 0,
            window_total: 0,
        }
    }
}

impl ConformalCalibrator {
    /// Creates a new `ConformalCalibrator` with specified error tolerance and adaptation rate.
    pub fn new(alpha: f32, gamma: f32, initial_threshold: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.001, 0.5),
            gamma: gamma.clamp(0.001, 0.1),
            quantile_threshold: initial_threshold.clamp(0.0, 1.0),
            window_errors: 0,
            window_total: 0,
        }
    }

    /// Updates the conformal threshold based on a non-conformity score.
    ///
    /// The non-conformity score `s_t` measures how "surprising" the routing outcome was.
    /// Higher scores indicate that the local SLM failed to handle the query adequately.
    ///
    /// Returns `true` if the threshold was adjusted.
    pub fn update(&mut self, non_conformity_score: f32) -> bool {
        self.window_total += 1;

        let indicator = if non_conformity_score > self.quantile_threshold {
            self.window_errors += 1;
            1.0f32
        } else {
            0.0f32
        };

        let old_threshold = self.quantile_threshold;
        // Gibbs & Candes (2021) online quantile calibration:
        // When error occurs (s_t > q_t), increase threshold by gamma * (1 - alpha).
        // When no error (s_t <= q_t), decrease threshold by gamma * alpha.
        // Equilibrium: E[indicator - alpha] = 0 => P(s_t > q_t) = alpha.
        self.quantile_threshold += self.gamma * (indicator - self.alpha);
        // INV-ROUTER-1: clamp to [0.0, 1.0]
        self.quantile_threshold = self.quantile_threshold.clamp(0.0, 1.0);

        (self.quantile_threshold - old_threshold).abs() > f32::EPSILON
    }

    /// Returns the current empirical error rate.
    pub fn empirical_error_rate(&self) -> f32 {
        if self.window_total == 0 {
            return 0.0;
        }
        self.window_errors as f32 / self.window_total as f32
    }

    /// Resets statistics while preserving the current calibrated threshold.
    pub fn reset_window(&mut self) {
        self.window_errors = 0;
        self.window_total = 0;
    }
}

/// Laufzeit-Kalibrierungsstatistik für ein SLM-Profil.
/// Wird von RouterEngine verwaltet und nicht vom Aufrufer gesetzt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileCalibrationState {
    /// Gesamtzahl der Routing-Entscheidungen für dieses Profil.
    pub times_selected: u64,
    /// Kumulierte Routing-Konfidenz (best_score / second_best_score).
    /// Höher = distinktiver. Startwert: 1.0
    pub cumulative_confidence: f64,
    /// Aktuell kalibrierter min_relevance_score-Wert
    /// (kann von original_min_relevance_score abweichen).
    pub calibrated_min_score: f32,
    /// Ursprünglicher min_relevance_score aus der Konfiguration.
    pub original_min_score: f32,
    /// Conformal calibrator for distribution-free threshold adaptation.
    pub conformal: ConformalCalibrator,
}

impl Default for ProfileCalibrationState {
    fn default() -> Self {
        Self {
            times_selected: 0,
            cumulative_confidence: 1.0,
            calibrated_min_score: 0.5,
            original_min_score: 0.5,
            conformal: ConformalCalibrator::default(),
        }
    }
}

impl ProfileCalibrationState {
    pub fn new(original_min_score: f32) -> Self {
        Self {
            times_selected: 0,
            cumulative_confidence: 1.0,
            calibrated_min_score: original_min_score,
            original_min_score,
            conformal: ConformalCalibrator::new(0.05, 0.01, original_min_score),
        }
    }

    /// Durchschnittliche Konfidenz über alle bisherigen Entscheidungen.
    pub fn average_confidence(&self) -> f64 {
        if self.times_selected == 0 {
            return 1.0;
        }
        self.cumulative_confidence / self.times_selected as f64
    }

    /// Recalibrates using the conformal calibrator.
    /// The non_conformity_score represents how poorly the SLM handled the last query.
    /// Returns true if the calibrated_min_score was adjusted.
    pub fn recalibrate_conformal(&mut self, non_conformity_score: f32) -> bool {
        let adjusted = self.conformal.update(non_conformity_score);
        if adjusted {
            // Derive calibrated_min_score from conformal threshold,
            // bounded by [original * 0.5, original * 2.0]
            let lower = self.original_min_score * 0.5;
            let upper = self.original_min_score * 2.0;
            self.calibrated_min_score = self.conformal.quantile_threshold.clamp(lower, upper);
        }
        adjusted
    }

    /// Legacy recalibration (kept for backward compatibility).
    /// Erhöht min_score falls durchschnittliche Konfidenz unter Schwellenwert.
    /// Gibt true zurück wenn eine Anpassung vorgenommen wurde.
    pub fn recalibrate(&mut self, low_confidence_threshold: f64) -> bool {
        if self.times_selected < 10 {
            // Nicht genug Daten für Kalibrierung
            return false;
        }
        let avg = self.average_confidence();
        if avg < low_confidence_threshold {
            // Score um 10% erhöhen, max 2× original
            let new_score = (self.calibrated_min_score * 1.1).min(self.original_min_score * 2.0);
            if new_score != self.calibrated_min_score {
                self.calibrated_min_score = new_score;
                return true;
            }
        } else if avg > low_confidence_threshold * 1.5 {
            // Konfidenz gut → Score in Richtung original relaxieren
            let new_score = (self.calibrated_min_score * 0.95).max(self.original_min_score);
            self.calibrated_min_score = new_score;
        }
        false
    }

    /// Setzt Kalibrierungsstate vollständig zurück.
    pub fn reset(&mut self) {
        self.times_selected = 0;
        self.cumulative_confidence = 1.0;
        self.calibrated_min_score = self.original_min_score;
        self.conformal = ConformalCalibrator::new(0.05, 0.01, self.original_min_score);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conformal_calibrator_invariants() {
        // INV-ROUTER-1: quantile_threshold remains strictly in [0.0, 1.0]
        let mut cal = ConformalCalibrator::new(0.05, 0.05, 0.5);

        // Extreme error inputs
        for _ in 0..100 {
            cal.update(1.0); // Extreme non-conformity -> threshold increases
            assert!(cal.quantile_threshold >= 0.0 && cal.quantile_threshold <= 1.0);
        }

        // Extreme non-error inputs
        for _ in 0..100 {
            cal.update(0.0); // Zero non-conformity -> threshold decreases
            assert!(cal.quantile_threshold >= 0.0 && cal.quantile_threshold <= 1.0);
        }
    }

    #[test]
    fn test_conformal_calibrator_coverage_guarantee() {
        // Test empirical coverage with Gibbs & Candes quantile adaptation
        let alpha = 0.10; // 90% target coverage
        let gamma = 0.01;
        let mut cal = ConformalCalibrator::new(alpha, gamma, 0.5);

        // Deterministic pseudo-random simulation (INV-ROUTER-2)
        // Simulate 10,000 requests where error occurs when score > 0.6
        let mut lcg = 123456789u64;
        for _ in 0..10_000 {
            lcg = lcg
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let rand_val = ((lcg >> 32) as u32 as f32) / (u32::MAX as f32);
            cal.update(rand_val);
        }

        let empirical = cal.empirical_error_rate();
        // Empirical error rate should converge around alpha (0.10) within +/- 0.05 tolerance
        assert!(
            (empirical - alpha).abs() < 0.05,
            "Empirical error rate {empirical} deviated too far from target alpha {alpha}"
        );
    }

    #[test]
    fn test_profile_calibration_state_conformal_recalibrate() {
        let mut state = ProfileCalibrationState::new(0.6);
        assert_eq!(state.calibrated_min_score, 0.6);

        // Update with non-conformity score
        let adjusted = state.recalibrate_conformal(0.8);
        assert!(adjusted);
        assert!(state.calibrated_min_score >= 0.3 && state.calibrated_min_score <= 1.2);
    }

    #[test]
    fn test_domain_communities_contains_is_o1() {
        let communities: HashSet<u64> = (1..=1000).collect();
        let profile = SlmProfile::new(
            "test-slm",
            "http://localhost:8000/mcp",
            communities,
            TokenBudget::new(1000, 100),
            0.1,
        );

        // Assert .contains(&target_id) returns correct bool
        assert!(profile.domain_communities.contains(&500));
        assert!(profile.domain_communities.contains(&1));
        assert!(profile.domain_communities.contains(&1000));
        assert!(!profile.domain_communities.contains(&0));
        assert!(!profile.domain_communities.contains(&1001));

        // Assert JSON serialization produces sorted array
        let json_str = serde_json::to_string(&profile).expect("serialize profile");
        let json_val: serde_json::Value =
            serde_json::from_str(&json_str).expect("parse json value");

        let arr = json_val["domain_communities"]
            .as_array()
            .expect("domain_communities is array");

        assert_eq!(arr.len(), 1000);
        let expected_sorted: Vec<u64> = (1..=1000).collect();
        let actual_arr: Vec<u64> = arr.iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(actual_arr, expected_sorted);

        // Round-trip deserialization
        let deserialized: SlmProfile =
            serde_json::from_str(&json_str).expect("deserialize profile");
        assert_eq!(deserialized, profile);
    }
}
