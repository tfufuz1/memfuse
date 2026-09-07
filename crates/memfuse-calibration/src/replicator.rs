//! F-07: Replikatordynamik für Online-Adaptive RRF-Signalgewichte.
//!
//! Basiert auf: Arora et al. (2012), The Multiplicative Weights Update Method.
//! Konvergenzgarantie: Regret O(√T·ln(N)) nach T Runden mit N Signalen.
//! KEINE Abhängigkeit zu memfuse-db oder memfuse-embed.

use memfuse_core::ConfigFingerprint;
use std::collections::HashMap;
use std::sync::Arc;

/// Zustand des Replikatordynamik-Algorithmus für adaptive RRF-Gewichte.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicatorState {
    /// Aktuelle Gewichte ω_i. Invariante: Σ ω_i = 1.0, alle ω_i > 0.
    pub weights: Vec<f32>,
    /// Signalnamen in der Reihenfolge der Gewichte.
    pub signal_names: Vec<String>,
    /// Lernrate η. Default: 0.05.
    pub eta: f32,
    /// Anzahl bisheriger Updates.
    pub update_count: u64,
    /// ConfigFingerprint der Kalibrierungsphase (P8-Compliance).
    pub fingerprint: Option<ConfigFingerprint>,
}

impl ReplicatorState {
    /// Erstellt einen neuen State mit Gleichverteilung.
    pub fn new(signal_names: Vec<String>, eta: f32) -> Self {
        let signal_names = if signal_names.is_empty() {
            vec!["vector".to_string(), "text".to_string(), "graph".to_string()]
        } else {
            signal_names
        };
        let eta = eta.clamp(0.001, 0.5);
        let n = signal_names.len();
        let weights = vec![1.0 / n as f32; n];
        Self {
            weights,
            signal_names,
            eta,
            update_count: 0,
            fingerprint: None,
        }
    }

    /// Multiplicative Weights Update für ein Belohnungsvektor r_i.
    /// `rewards`: Skalar-Belohnungen für jedes Signal (höher = besser).
    /// INVARIANTE: Nach dem Update gilt Σ ω_i = 1.0.
    pub fn update(&mut self, rewards: &[f32]) -> bool {
        assert_eq!(
            rewards.len(),
            self.weights.len(),
            "rewards length must match weights length"
        );
        let old_weights = self.weights.clone();

        // Multiplicative update
        for (w, &r) in self.weights.iter_mut().zip(rewards) {
            *w *= (self.eta * r).exp();
            if *w <= 0.0 || !w.is_finite() {
                *w = f32::EPSILON;
            }
        }

        // Normalisierung
        let sum: f32 = self.weights.iter().sum();
        if sum > 0.0 && sum.is_finite() {
            self.weights.iter_mut().for_each(|w| *w /= sum);
        } else {
            // Fallback: Gleichverteilung (numerische Stabilität)
            let n = self.weights.len() as f32;
            self.weights.iter_mut().for_each(|w| *w = 1.0 / n);
        }

        // Guarantee all weights > 0 and sum = 1.0 numerically
        for w in &mut self.weights {
            if *w <= 0.0 || !w.is_finite() {
                *w = f32::EPSILON;
            }
        }
        let final_sum: f32 = self.weights.iter().sum();
        if final_sum > 0.0 && final_sum.is_finite() {
            self.weights.iter_mut().for_each(|w| *w /= final_sum);
        }

        self.update_count += 1;
        self.weights != old_weights
    }

    /// Gibt die aktuellen Gewichte für die drei Kern-Signale zurück.
    pub fn fusion_weights(&self) -> memfuse_core::FusionWeights {
        let mut vector_w = 0.0;
        let mut text_w = 0.0;
        let mut graph_w = 0.0;

        for (name, &w) in self.signal_names.iter().zip(&self.weights) {
            match name.as_str() {
                "vector" => vector_w = w,
                "text" => text_w = w,
                "graph" => graph_w = w,
                _ => {}
            }
        }

        let core_sum = vector_w + text_w + graph_w;
        if core_sum > 0.0 && core_sum.is_finite() {
            vector_w /= core_sum;
            text_w /= core_sum;
            graph_w /= core_sum;
        } else {
            vector_w = 1.0 / 3.0;
            text_w = 1.0 / 3.0;
            graph_w = 1.0 / 3.0;
        }

        memfuse_core::FusionWeights::new(vector_w, text_w, graph_w).unwrap_or_default()
    }

    /// P8: Setzt State auf Gleichverteilung zurück bei Fingerprint-Änderung.
    pub fn invalidate_on_config_change(&mut self, new_fp: ConfigFingerprint) {
        if self.fingerprint.as_ref() != Some(&new_fp) {
            tracing::warn!(
                old_fp = ?self.fingerprint,
                new_fp = ?new_fp,
                "ReplicatorState: ConfigFingerprint changed — resetting to uniform distribution (P8)"
            );
            let n = self.weights.len();
            if n > 0 {
                let uniform = 1.0 / n as f32;
                for w in &mut self.weights {
                    *w = uniform;
                }
            }
            self.fingerprint = Some(new_fp);
        }
    }
}

/// Meldet Retrieval-Erfolg für ein Signal zurück (aus Implizit-Feedback oder Nutzer-Klick).
/// `signal_rewards`: {signal_name → reward in [0,1]}
pub fn record_retrieval_feedback(
    state: &Arc<parking_lot::RwLock<ReplicatorState>>,
    signal_rewards: HashMap<String, f32>,
) {
    let mut state_guard = state.write();
    let rewards: Vec<f32> = state_guard
        .signal_names
        .iter()
        .map(|name| {
            signal_rewards
                .get(name)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        })
        .collect();
    state_guard.update(&rewards);
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_replicator_equal_rewards_preserves_uniform() {
        let mut state = ReplicatorState::new(
            vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
            0.05,
        );
        let initial_weights = state.weights.clone();
        assert_eq!(initial_weights, vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]);

        // Updates with equal rewards for all signals
        let changed = state.update(&[0.5, 0.5, 0.5]);
        assert!(!changed, "Equal rewards should preserve equal weights");
        for &w in &state.weights {
            assert!((w - 1.0 / 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_replicator_positive_reward_increases_weight() {
        let mut state = ReplicatorState::new(
            vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
            0.1,
        );
        let initial_w0 = state.weights[0];

        // Signal 0 receives reward 1.0, others receive 0.0
        let changed = state.update(&[1.0, 0.0, 0.0]);
        assert!(changed);
        assert!(
            state.weights[0] > initial_w0,
            "w_0 should increase when signal 0 receives positive reward"
        );
        assert!(state.weights[1] < initial_w0);
        assert!(state.weights[2] < initial_w0);
    }

    #[test]
    fn test_replicator_invalidate_on_config_change() {
        let mut state = ReplicatorState::new(
            vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
            0.05,
        );
        state.update(&[1.0, 0.0, 0.0]);
        assert!(state.weights[0] > 1.0 / 3.0);

        let fp = ConfigFingerprint::new("model-a", "Q4", "template", 0.1);
        state.invalidate_on_config_change(fp.clone());
        assert_eq!(state.fingerprint, Some(fp.clone()));
        for &w in &state.weights {
            assert!((w - 1.0 / 3.0).abs() < 1e-6);
        }

        // Calling with same fingerprint should not reset weights
        state.update(&[1.0, 0.0, 0.0]);
        let w0_before = state.weights[0];
        state.invalidate_on_config_change(fp);
        assert_eq!(state.weights[0], w0_before);
    }

    #[test]
    fn test_record_retrieval_feedback() {
        let state = Arc::new(parking_lot::RwLock::new(ReplicatorState::new(
            vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
            0.05,
        )));

        let mut feedback = HashMap::new();
        feedback.insert("vector".to_string(), 0.9);
        feedback.insert("text".to_string(), 0.1);

        record_retrieval_feedback(&state, feedback);

        let guard = state.read();
        assert_eq!(guard.update_count, 1);
        assert!(guard.weights[0] > guard.weights[1]);
    }

    proptest! {
        #[test]
        fn test_replicator_sum_always_one(
            r0 in 0.0f32..1.0f32,
            r1 in 0.0f32..1.0f32,
            r2 in 0.0f32..1.0f32,
            steps in 1usize..20
        ) {
            let mut state = ReplicatorState::new(
                vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
                0.1,
            );
            let rewards = [r0, r1, r2];

            for _ in 0..steps {
                state.update(&rewards);
                let sum: f32 = state.weights.iter().sum();
                prop_assert!((sum - 1.0).abs() < 1e-5, "Sum of weights must always be 1.0, got {}", sum);
            }
        }

        #[test]
        fn test_replicator_invariant_all_positive(
            r0 in 0.0f32..1.0f32,
            r1 in 0.0f32..1.0f32,
            r2 in 0.0f32..1.0f32,
            steps in 1usize..20
        ) {
            let mut state = ReplicatorState::new(
                vec!["vector".to_string(), "text".to_string(), "graph".to_string()],
                0.1,
            );
            let rewards = [r0, r1, r2];

            for _ in 0..steps {
                state.update(&rewards);
                for &w in &state.weights {
                    prop_assert!(w > 0.0, "Weight must always be strictly positive, got {}", w);
                    prop_assert!(w.is_finite(), "Weight must be finite");
                }
            }
        }
    }
}
