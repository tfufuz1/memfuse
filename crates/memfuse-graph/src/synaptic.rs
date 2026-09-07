//! F-03: Hebbian/Stigmergische Kantenverstärkung.
//!
//! FEATURE-FLAG: `physio-synaptic-edges`
//! KEIN neuer Index — nur zwei f32-Felder pro Kante.
//! Update erfolgt asynchron NACH dem Retrieval, nie während.

use crate::csr::Edge;

#[derive(Debug, Clone)]
pub struct SynapticConfig {
    /// Hebbian learning rate η. Default: 0.01.
    pub eta: f32,
    /// Passiver Kantenzerfall δ. Default: 0.001.
    pub delta: f32,
    /// Maximale Gesamtgewicht pro Knoten W_max. Default: 10.0.
    pub w_max: f32,
    /// Pheromon-Verdunstungsrate ρ. Default: 0.05.
    pub rho: f32,
    /// Belohnungskonstante Q. Default: 1.0.
    pub q: f32,
    /// Mischkoeffizient α (Hebb vs. Pheromon). Default: 0.5.
    pub alpha: f32,
}

impl Default for SynapticConfig {
    fn default() -> Self {
        Self {
            eta: 0.01,
            delta: 0.001,
            w_max: 10.0,
            rho: 0.05,
            q: 1.0,
            alpha: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::EntityId;

    #[test]
    fn test_synaptic_score_formula() {
        let score = synaptic_score(0.3, 0.7, 0.5);
        assert!((score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_hebbian_update_decay() {
        let mut edge = Edge::new(EntityId::new(1), 1.0);
        edge.hebbian_weight = 1.0;
        let config = SynapticConfig::default();

        // After N updates without co-activation (co_activation = 0.0), hebbian_weight decays towards 0
        for _ in 0..1000 {
            apply_hebbian_update(&mut edge, 0.0, &config);
        }
        assert!(edge.hebbian_weight < 0.4);
        assert!(edge.hebbian_weight >= 0.0);
    }

    #[test]
    fn test_homeostatic_scaling_caps_w_max() {
        let mut edges = vec![
            Edge::new(EntityId::new(1), 1.0),
            Edge::new(EntityId::new(2), 1.0),
        ];
        edges[0].hebbian_weight = 8.0;
        edges[1].hebbian_weight = 8.0;

        let w_max = 10.0;
        apply_homeostatic_scaling(&mut edges, w_max);

        let sum: f32 = edges.iter().map(|e| e.hebbian_weight).sum();
        assert!((sum - w_max).abs() < 1e-5);
        assert_eq!(edges[0].hebbian_weight, 5.0);
        assert_eq!(edges[1].hebbian_weight, 5.0);
    }

    #[test]
    fn test_pheromone_evaporation() {
        let mut edge = Edge::new(EntityId::new(1), 1.0);
        edge.pheromone = 1.0;
        let config = SynapticConfig::default();

        // Path length 0 -> no reinforcement, pure evaporation
        let initial_pheromone = edge.pheromone;
        apply_pheromone_update(&mut edge, 0, &config);
        assert!(edge.pheromone < initial_pheromone);
        assert!((edge.pheromone - 0.95).abs() < 1e-5);
        assert!(edge.pheromone >= 0.0);
    }
}

/// Berechnet SynapticScore für eine Kante.
#[inline]
pub fn synaptic_score(hebbian_weight: f32, pheromone: f32, alpha: f32) -> f32 {
    alpha * hebbian_weight + (1.0 - alpha) * pheromone
}

/// Führt Hebbian-Update durch und prüft Homöostase-Invariante.
/// Gibt `true` zurück wenn homöostatische Skalierung nötig war.
pub fn apply_hebbian_update(edge: &mut Edge, co_activation: f32, config: &SynapticConfig) -> bool {
    let delta = config.eta * co_activation - config.delta * edge.hebbian_weight;
    edge.hebbian_weight = (edge.hebbian_weight + delta).max(0.0);
    edge.hebbian_weight > config.w_max
}

/// Führt Pheromon-Update durch (Evaporation + Verstärkung).
pub fn apply_pheromone_update(edge: &mut Edge, path_length: usize, config: &SynapticConfig) {
    let reinforcement = if path_length > 0 {
        config.q / (path_length as f32)
    } else {
        0.0
    };
    edge.pheromone = ((1.0 - config.rho) * edge.pheromone + reinforcement).max(0.0);
}

/// Homöostatische Skalierung: wenn Σ_j w_ij > W_max, skaliere alle Kanten von i proportional.
pub fn apply_homeostatic_scaling(outgoing_edges: &mut [Edge], w_max: f32) {
    let sum_w: f32 = outgoing_edges.iter().map(|e| e.hebbian_weight).sum();
    if sum_w > w_max && sum_w > 0.0 {
        let scale = w_max / sum_w;
        for edge in outgoing_edges {
            edge.hebbian_weight = (edge.hebbian_weight * scale).max(0.0);
        }
    }
}
