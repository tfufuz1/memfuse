//! F-03: Co-Occurrence- und Traversal-basierte Kantenverstärkung (Edge Weight Reinforcement Learning).
//! Zwei unabhängige Signale pro Kante: (1) Verstärkung bei gemeinsamer Aktivierung
//! zweier Knoten in einer Retrieval-Session (Co-Occurrence-Reinforcement, mit passivem Zerfall),
//! (2) Verstärkung bei Pfad-Traversal mit längenabhängigem Reward und Zeit-Verdunstung
//! (Traversal-Reinforcement, analog zu Trail-Reinforcement in Pfadfindungs-Algorithmen).
//!
//! FEATURE-FLAG: `edge-reinforcement-learning`
//! KEIN neuer Index — nur zwei f32-Felder pro Kante.
//! Update erfolgt asynchron NACH dem Retrieval, nie während.

use crate::csr::Edge;

#[derive(Debug, Clone)]
pub struct EdgeReinforcementConfig {
    /// Co-occurrence reinforcement rate η. Default: 0.01.
    pub eta: f32,
    /// Passiver Kantenzerfall δ. Default: 0.001.
    pub delta: f32,
    /// Maximale Gesamtgewicht pro Knoten W_max. Default: 10.0.
    pub w_max: f32,
    /// Traversal-Verdunstungsrate (Zeit-Decay) ρ. Default: 0.05.
    pub rho: f32,
    /// Belohnungskonstante Q. Default: 1.0.
    pub q: f32,
    /// Mischkoeffizient α (Co-Occurrence- vs. Traversal-Signal). Default: 0.5.
    pub alpha: f32,
}

impl Default for EdgeReinforcementConfig {
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
    fn test_compute_edge_weight_formula() {
        let score = compute_edge_weight(0.3, 0.7, 0.5);
        assert!((score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_cooccurrence_reinforcement_decay() {
        let mut edge = Edge::new(EntityId::new(1), 1.0);
        edge.cooccurrence_weight = 1.0;
        let config = EdgeReinforcementConfig::default();

        // After N updates without co-activation (co_activation = 0.0), cooccurrence_weight decays towards 0
        for _ in 0..1000 {
            apply_cooccurrence_reinforcement(&mut edge, 0.0, &config);
        }
        assert!(edge.cooccurrence_weight < 0.4);
        assert!(edge.cooccurrence_weight >= 0.0);
    }

    #[test]
    fn test_weight_normalization_caps_w_max() {
        let mut edges = vec![
            Edge::new(EntityId::new(1), 1.0),
            Edge::new(EntityId::new(2), 1.0),
        ];
        edges[0].cooccurrence_weight = 8.0;
        edges[1].cooccurrence_weight = 8.0;

        let w_max = 10.0;
        apply_weight_normalization(&mut edges, w_max);

        let sum: f32 = edges.iter().map(|e| e.cooccurrence_weight).sum();
        assert!((sum - w_max).abs() < 1e-5);
        assert_eq!(edges[0].cooccurrence_weight, 5.0);
        assert_eq!(edges[1].cooccurrence_weight, 5.0);
    }

    #[test]
    fn test_traversal_reinforcement_evaporation() {
        let mut edge = Edge::new(EntityId::new(1), 1.0);
        edge.traversal_weight = 1.0;
        let config = EdgeReinforcementConfig::default();

        // Path length 0 -> no reinforcement, pure evaporation
        let initial_traversal = edge.traversal_weight;
        apply_traversal_reinforcement(&mut edge, 0, &config);
        assert!(edge.traversal_weight < initial_traversal);
        assert!((edge.traversal_weight - 0.95).abs() < 1e-5);
        assert!(edge.traversal_weight >= 0.0);
    }
}

/// Berechnet das Gesamt-Kantengewicht für eine Kante.
#[inline]
pub fn compute_edge_weight(cooccurrence_weight: f32, traversal_weight: f32, alpha: f32) -> f32 {
    alpha * cooccurrence_weight + (1.0 - alpha) * traversal_weight
}

/// Führt Co-Occurrence-Verstärkung durch und prüft Normalisierungs-Invariante.
/// Gibt `true` zurück wenn Gewichtsnormalisierung nötig war.
pub fn apply_cooccurrence_reinforcement(
    edge: &mut Edge,
    co_activation: f32,
    config: &EdgeReinforcementConfig,
) -> bool {
    let delta = config.eta * co_activation - config.delta * edge.cooccurrence_weight;
    edge.cooccurrence_weight = (edge.cooccurrence_weight + delta).max(0.0);
    edge.cooccurrence_weight > config.w_max
}

/// Führt Traversal-Verstärkung durch (Evaporation + Verstärkung).
pub fn apply_traversal_reinforcement(
    edge: &mut Edge,
    path_length: usize,
    config: &EdgeReinforcementConfig,
) {
    let reinforcement = if path_length > 0 {
        config.q / (path_length as f32)
    } else {
        0.0
    };
    edge.traversal_weight =
        ((1.0 - config.rho) * edge.traversal_weight + reinforcement).max(0.0);
}

/// Gewichtsnormalisierung: wenn Σ_j w_ij > W_max, skaliere alle Kanten von i proportional.
pub fn apply_weight_normalization(outgoing_edges: &mut [Edge], w_max: f32) {
    let sum_w: f32 = outgoing_edges.iter().map(|e| e.cooccurrence_weight).sum();
    if sum_w > w_max && sum_w > 0.0 {
        let scale = w_max / sum_w;
        for edge in outgoing_edges {
            edge.cooccurrence_weight = (edge.cooccurrence_weight * scale).max(0.0);
        }
    }
}
