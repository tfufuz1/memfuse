//! Replicator Dynamics for Adaptive Hybrid-Search Signal Weights (Feature F-07).

// FILE-CONTEXT
// STAND: 2026-08-30T22:00:00Z
// ZWECK: Adaptives Signal-Gewichtungssystem basierend auf zeitdiskreter Replikatordynamik.
// INVARIANTEN: Summe der Signal-Gewichte == 1.0 (±1e-6); w_min <= w_s <= w_max;
//              P12: Physio-Feature-Default-Unsichtbarkeit (rein additiv, opt-in).
// MATHE / KONVERGENZ:
//   Diese Implementierung nutzt die zeitdiskrete Replikatorgleichung:
//   w_s(t+1) = w_s(t) * (1 + η * (f_s(t) - f̄(t)))
//   Gefolgt von Normalisierung (Σ w_s = 1) und Clamping auf [w_min, w_max] (Default: [0.05, 0.90]).
//   Garantie: Unter stationären Fitness-Erwartungswerten konvergiert dieser Mechanismus gegen ein
//   Nash-Gleichgewicht der relativen Signal-Gewichte (Standard-Resultat der Evolutionsspieltheorie).
//   Dies dient als deterministischer, stabiler Ersatz für Lotka-Volterra-Populationsdynamiken
//   oder genetische Algorithmen (siehe memfuse_spec.md Anhang B, "Verworfene Features").
// SIEHE AUCH: crates/memfuse-db/src/fusion.rs, memfuse-core/src/types/saos.rs

use crate::fusion::SignalKind;
use crate::{SearchResult, SignalContribution};
use memfuse_core::FusionWeights;
use std::collections::HashMap;

/// Alias for `SearchResult` used in fusion contexts.
pub type FusionResult = SearchResult;

/// Adaptiver Manager für `FusionWeights` mittels Replikatordynamik.
#[derive(Debug, Clone)]
pub struct AdaptiveFusionWeights {
    current: FusionWeights,
    fitness_accumulator: HashMap<SignalKind, (f32 /* hits */, f32 /* total */)>,
    eta: f32,
    window_size: u32,
    updates_in_window: u32,
    w_min: f32,
    w_max: f32,
}

impl AdaptiveFusionWeights {
    /// Erstellt eine neue Instanz von `AdaptiveFusionWeights`.
    ///
    /// Standardmäßig werden `w_min = 0.05` und `w_max = 0.90` gesetzt, um den Kollaps
    /// eines Signals auf 0.0 zu verhindern.
    pub fn new(initial: FusionWeights, eta: f32, window_size: u32) -> Self {
        Self {
            current: initial,
            fitness_accumulator: HashMap::new(),
            eta,
            window_size,
            updates_in_window: 0,
            w_min: 0.05,
            w_max: 0.90,
        }
    }

    /// Konfiguriert benutzerdefinierte Min/Max-Grenzen für die Gewichte.
    pub fn with_bounds(mut self, w_min: f32, w_max: f32) -> Self {
        if w_min >= 0.0 && w_max <= 1.0 && w_min < w_max {
            self.w_min = w_min;
            self.w_max = w_max;
        }
        self
    }

    /// Akkumuliert Beobachtungen zur Relevanz-Trefferquote pro Signal.
    ///
    /// Nutzt eine Liste von `SignalContribution`-Strukturen. Wenn `was_relevant` wahr ist,
    /// wird der Hit-Akkumulator für jedes vertretene Signal erhöht.
    pub fn record_outcome(&mut self, contributions: &[SignalContribution], was_relevant: bool) {
        if contributions.is_empty() {
            return;
        }

        let kinds = [SignalKind::Vector, SignalKind::Text, SignalKind::Graph];
        for (idx, contrib) in contributions.iter().enumerate() {
            if idx < kinds.len() && (contrib.rrf_contribution > 0.0 || contrib.rank > 0) {
                let kind = kinds[idx];
                let entry = self.fitness_accumulator.entry(kind).or_insert((0.0, 0.0));
                if was_relevant {
                    entry.0 += 1.0;
                }
                entry.1 += 1.0;
            }
        }
        self.updates_in_window += 1;
    }

    /// Akkumuliert Beobachtungen aus einer Map von Signal-Namen auf `SignalContribution`.
    pub fn record_outcome_map(
        &mut self,
        contributions: &HashMap<String, SignalContribution>,
        was_relevant: bool,
    ) {
        if contributions.is_empty() {
            return;
        }

        for (signal_name, contrib) in contributions {
            if contrib.rrf_contribution > 0.0 || contrib.rank > 0 {
                if let Some(kind) = SignalKind::from_name(signal_name) {
                    let entry = self.fitness_accumulator.entry(kind).or_insert((0.0, 0.0));
                    if was_relevant {
                        entry.0 += 1.0;
                    }
                    entry.1 += 1.0;
                }
            }
        }
        self.updates_in_window += 1;
    }

    /// Akkumuliert Beobachtungen direkt aus einem `SearchResult`.
    pub fn record_outcome_from_result(&mut self, result: &SearchResult, was_relevant: bool) {
        if let Some(prov) = &result.provenance {
            self.record_outcome_map(&prov.signal_contributions, was_relevant);
        } else {
            for signal_name in &result.matched_signals {
                if let Some(kind) = SignalKind::from_name(signal_name) {
                    let entry = self.fitness_accumulator.entry(kind).or_insert((0.0, 0.0));
                    if was_relevant {
                        entry.0 += 1.0;
                    }
                    entry.1 += 1.0;
                }
            }
            if !result.matched_signals.is_empty() {
                self.updates_in_window += 1;
            }
        }
    }

    /// Wendet nach Erreichen von `window_size` Beobachtungen die Replikatorgleichung an.
    ///
    /// Resettet den Akkumulator und gibt die neuen `FusionWeights` zurück (sonst `None`).
    pub fn maybe_update_weights(&mut self) -> Option<FusionWeights> {
        if self.updates_in_window < self.window_size {
            return None;
        }

        self.updates_in_window = 0;

        let kinds = [SignalKind::Vector, SignalKind::Text, SignalKind::Graph];
        let mut fitnesses = HashMap::new();

        for kind in &kinds {
            let (hits, total) = self
                .fitness_accumulator
                .get(kind)
                .copied()
                .unwrap_or((0.0, 0.0));
            let f = if total > 0.0 { hits / total } else { 0.0 };
            fitnesses.insert(*kind, f);
        }

        self.fitness_accumulator.clear();

        let f_vec = fitnesses.get(&SignalKind::Vector).copied().unwrap_or(0.0);
        let f_txt = fitnesses.get(&SignalKind::Text).copied().unwrap_or(0.0);
        let f_grp = fitnesses.get(&SignalKind::Graph).copied().unwrap_or(0.0);

        let f_avg = (f_vec + f_txt + f_grp) / 3.0;

        let cur_v = self.current.vector();
        let cur_t = self.current.text();
        let cur_g = self.current.graph();

        // Replikator-Update: w_s(t+1) = w_s(t) * (1 + eta * (f_s - f_avg))
        let raw_v = (cur_v * (1.0 + self.eta * (f_vec - f_avg))).max(0.0);
        let raw_t = (cur_t * (1.0 + self.eta * (f_txt - f_avg))).max(0.0);
        let raw_g = (cur_g * (1.0 + self.eta * (f_grp - f_avg))).max(0.0);

        // Clamping & Normalisierung
        let mut w_v = raw_v.clamp(self.w_min, self.w_max);
        let mut w_t = raw_t.clamp(self.w_min, self.w_max);
        let mut w_g = raw_g.clamp(self.w_min, self.w_max);

        for _ in 0..10 {
            let sum = w_v + w_t + w_g;
            if sum <= 0.0 {
                w_v = 1.0 / 3.0;
                w_t = 1.0 / 3.0;
                w_g = 1.0 / 3.0;
                break;
            }
            w_v = (w_v / sum).clamp(self.w_min, self.w_max);
            w_t = (w_t / sum).clamp(self.w_min, self.w_max);
            w_g = (w_g / sum).clamp(self.w_min, self.w_max);
        }

        let sum = w_v + w_t + w_g;
        if sum > 0.0 {
            w_v /= sum;
            w_t /= sum;
            w_g = 1.0 - w_v - w_t;
            if w_g < self.w_min || w_g > self.w_max {
                w_g = w_g.clamp(self.w_min, self.w_max);
                let sum2 = w_v + w_t + w_g;
                w_v /= sum2;
                w_t /= sum2;
                w_g = 1.0 - w_v - w_t;
            }
        } else {
            w_v = 1.0 / 3.0;
            w_t = 1.0 / 3.0;
            w_g = 1.0 - w_v - w_t;
        }

        match FusionWeights::new(w_v, w_t, w_g) {
            Ok(new_weights) => {
                self.current = new_weights.clone();
                Some(new_weights)
            }
            Err(_) => None,
        }
    }

    /// Liefert die aktuell aktiven `FusionWeights`.
    pub fn current_weights(&self) -> &FusionWeights {
        &self.current
    }
}

/// Wendet Adaptionsprüfungen an, nachdem `weighted_reciprocal_rank_fusion_with_options()` aufgerufen wurde.
///
/// Ändert nicht die Signatur bestehender Fusion-Funktionen.
pub fn apply_and_maybe_adapt(
    _results: &mut Vec<FusionResult>,
    adaptive: &mut AdaptiveFusionWeights,
) {
    let _ = adaptive.maybe_update_weights();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_higher_hitrate_gains_weight_sum_equals_one() {
        let initial = FusionWeights::new(0.33333334, 0.33333334, 0.33333332).expect("valid");
        let mut adaptive = AdaptiveFusionWeights::new(initial, 0.1, 5);

        let vec_contrib = vec![
            SignalContribution {
                raw_score: 0.9,
                rank: 1,
                rrf_contribution: 0.1,
            },
            SignalContribution {
                raw_score: 0.0,
                rank: 0,
                rrf_contribution: 0.0,
            },
            SignalContribution {
                raw_score: 0.0,
                rank: 0,
                rrf_contribution: 0.0,
            },
        ];

        let initial_vector_weight = adaptive.current_weights().vector();

        // Run multiple windows where vector has 100% hit rate and others have 0%
        for _ in 0..5 {
            for _ in 0..5 {
                adaptive.record_outcome(&vec_contrib, true);
            }
            let updated = adaptive.maybe_update_weights();
            assert!(updated.is_some());
            let weights = updated.expect("weights");

            let sum = weights.vector() + weights.text() + weights.graph();
            assert!(
                (sum - 1.0).abs() <= 1e-6,
                "Sum must equal 1.0 (got {})",
                sum
            );
        }

        assert!(
            adaptive.current_weights().vector() > initial_vector_weight,
            "Vector signal with higher hit rate must gain relative weight"
        );
    }

    #[test]
    fn test_bounds_w_min_w_max_enforced() {
        let initial = FusionWeights::new(0.33333334, 0.33333334, 0.33333332).expect("valid");
        let mut adaptive = AdaptiveFusionWeights::new(initial, 0.5, 2).with_bounds(0.05, 0.90);

        let vec_contrib = vec![
            SignalContribution {
                raw_score: 0.9,
                rank: 1,
                rrf_contribution: 0.1,
            },
            SignalContribution {
                raw_score: 0.0,
                rank: 0,
                rrf_contribution: 0.0,
            },
            SignalContribution {
                raw_score: 0.0,
                rank: 0,
                rrf_contribution: 0.0,
            },
        ];

        // Run 20 windows of extreme vector dominance
        for _ in 0..20 {
            for _ in 0..2 {
                adaptive.record_outcome(&vec_contrib, true);
            }
            adaptive.maybe_update_weights();
        }

        let w = adaptive.current_weights();
        assert!(
            w.vector() <= 0.90 + 1e-5,
            "Vector weight must not exceed w_max (got {})",
            w.vector()
        );
        assert!(
            w.text() >= 0.05 - 1e-5,
            "Text weight must not fall below w_min (got {})",
            w.text()
        );
        assert!(
            w.graph() >= 0.05 - 1e-5,
            "Graph weight must not fall below w_min (got {})",
            w.graph()
        );
    }

    #[test]
    fn test_identical_fitness_stable_weights() {
        let initial = FusionWeights::new(0.5, 0.3, 0.2).expect("valid");
        let mut adaptive = AdaptiveFusionWeights::new(initial.clone(), 0.1, 3);

        let all_contrib = vec![
            SignalContribution {
                raw_score: 0.9,
                rank: 1,
                rrf_contribution: 0.1,
            },
            SignalContribution {
                raw_score: 0.8,
                rank: 1,
                rrf_contribution: 0.1,
            },
            SignalContribution {
                raw_score: 0.7,
                rank: 1,
                rrf_contribution: 0.1,
            },
        ];

        for _ in 0..3 {
            adaptive.record_outcome(&all_contrib, true);
        }

        let updated = adaptive.maybe_update_weights();
        assert!(updated.is_some());
        let w = updated.expect("weights");

        assert!(
            (w.vector() - initial.vector()).abs() < 1e-5,
            "Vector weight drift detected"
        );
        assert!(
            (w.text() - initial.text()).abs() < 1e-5,
            "Text weight drift detected"
        );
        assert!(
            (w.graph() - initial.graph()).abs() < 1e-5,
            "Graph weight drift detected"
        );
    }

    #[test]
    fn test_maybe_update_weights_returns_none_until_window_size() {
        let initial = FusionWeights::default();
        let mut adaptive = AdaptiveFusionWeights::new(initial, 0.1, 5);

        let contrib = vec![SignalContribution {
            raw_score: 0.9,
            rank: 1,
            rrf_contribution: 0.1,
        }];

        for _ in 0..4 {
            adaptive.record_outcome(&contrib, true);
            assert!(
                adaptive.maybe_update_weights().is_none(),
                "Must return None before window_size is reached"
            );
        }

        adaptive.record_outcome(&contrib, true);
        assert!(
            adaptive.maybe_update_weights().is_some(),
            "Must return Some after window_size is reached"
        );
    }
}
