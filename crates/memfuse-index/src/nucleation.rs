//! F-02: Lokaler Nukleations-Trigger für HNSW-Hot-Path-Regionen.
//!
//! FEATURE-FLAG: `physio-nucleation` (default: off).
//! ARCHITEKTUR: Ergänzt den globalen `HNSW_REBUILD_DELETION_RATIO`-Trigger.
//!              Kein Ersatz — der globale Trigger bleibt aktiv.
//!              Nur der Partial-Rebuild-Pfad wird ergänzt.
//!
//! INVARIANTE INV-NUC-1: Ein Partial-Rebuild darf den globalen HNSW-Graph nicht inkonsistent
//!   hinterlassen. Nachbarschaftsbeziehungen über die Regionsgrenze hinaus bleiben erhalten.

use std::collections::{HashMap, VecDeque};

/// Konfiguration für den Nukleations-Trigger.
#[derive(Debug, Clone)]
pub struct NucleationConfig {
    /// Kritische Übersättigungs-Schwelle θ_c. Default: 3.0 (3× globale Dichte).
    pub critical_ratio: f32,
    /// Ringpuffer-Größe N für Traversal-Statistik. Default: 1000.
    pub traversal_window: usize,
    /// Mindest-Tombstone-Anteil global, bevor lokale Analyse sinnvoll ist. Default: 0.01.
    pub min_global_ratio: f32,
}

impl Default for NucleationConfig {
    fn default() -> Self {
        Self {
            critical_ratio: 3.0,
            traversal_window: 1000,
            min_global_ratio: 0.01,
        }
    }
}

/// Ringpuffer für besuchte HNSW-Knoten-IDs während ef_search-Traversierungen.
#[derive(Debug)]
pub struct TraversalTracker {
    config: NucleationConfig,
    visited_ring: VecDeque<Vec<u64>>, // Ein Vec<u64> pro Traversierungsaufruf
}

impl TraversalTracker {
    /// Erstellt einen neuen TraversalTracker mit der gegebenen Konfiguration.
    pub fn new(config: NucleationConfig) -> Self {
        Self {
            visited_ring: VecDeque::with_capacity(config.traversal_window),
            config,
        }
    }

    /// Trägt die in einem ef_search-Lauf besuchten Node-IDs ein.
    pub fn record_traversal(&mut self, visited_node_ids: Vec<u64>) {
        if self.config.traversal_window == 0 || visited_node_ids.is_empty() {
            return;
        }
        if self.visited_ring.len() >= self.config.traversal_window {
            self.visited_ring.pop_front();
        }
        self.visited_ring.push_back(visited_node_ids);
    }

    /// Berechnet S_local für alle Hot-Path-Regionen.
    /// Gibt Knoten-IDs zurück, für die S_local > θ_c gilt.
    pub fn find_oversaturated_regions(
        &self,
        tombstone_map: &HashMap<u64, bool>, // node_id → is_tombstoned
    ) -> Vec<u64> {
        if tombstone_map.is_empty() {
            return Vec::new();
        }

        let total_nodes = tombstone_map.len() as f32;
        let global_tombstones = tombstone_map.values().filter(|&&ts| ts).count() as f32;
        let global_density = global_tombstones / total_nodes;

        if global_density <= 0.0 {
            return Vec::new();
        }

        let hot_nodes: Vec<u64> = self.hot_path_nodes().collect();
        if hot_nodes.is_empty() {
            return Vec::new();
        }

        let local_tombstones = hot_nodes
            .iter()
            .filter(|id| tombstone_map.get(id) == Some(&true))
            .count() as f32;
        let local_density = local_tombstones / hot_nodes.len() as f32;

        let s_local = local_density / global_density;

        if s_local > self.config.critical_ratio {
            hot_nodes
        } else {
            Vec::new()
        }
    }

    /// Gibt die N zuletzt besuchten eindeutigen Knoten-IDs zurück.
    pub fn hot_path_nodes(&self) -> impl Iterator<Item = u64> + '_ {
        let mut seen = std::collections::HashSet::new();
        let mut nodes = Vec::new();

        // Traversals starting from most recent
        for traversal in self.visited_ring.iter().rev() {
            for &node_id in traversal {
                if seen.insert(node_id) {
                    nodes.push(node_id);
                }
            }
        }
        nodes.into_iter()
    }
}

/// Entscheidet ob ein lokaler Partial-Rebuild ausgelöst werden soll.
///
/// Returns `Some(hot_node_ids)` wenn Nukleation ausgelöst werden soll,
/// `None` wenn kein Rebuild nötig.
#[cfg(feature = "physio-nucleation")]
pub fn should_trigger_nucleation(
    tracker: &TraversalTracker,
    tombstone_map: &HashMap<u64, bool>,
    global_tombstone_ratio: f32,
    config: &NucleationConfig,
) -> Option<Vec<u64>> {
    if global_tombstone_ratio < config.min_global_ratio {
        return None;
    }
    let regions = tracker.find_oversaturated_regions(tombstone_map);
    if regions.is_empty() {
        None
    } else {
        Some(regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nucleation_tracker_records_traversals() {
        let config = NucleationConfig {
            traversal_window: 5,
            ..Default::default()
        };
        let mut tracker = TraversalTracker::new(config);

        tracker.record_traversal(vec![1, 2, 3]);
        tracker.record_traversal(vec![3, 4, 5]);
        tracker.record_traversal(vec![5, 6, 7]);
        tracker.record_traversal(vec![7, 8, 9]);
        tracker.record_traversal(vec![9, 10, 1]);

        let hot_nodes: Vec<u64> = tracker.hot_path_nodes().collect();
        assert_eq!(hot_nodes.len(), 10);
        for id in 1..=10 {
            assert!(hot_nodes.contains(&id));
        }
    }

    #[test]
    fn test_nucleation_no_trigger_below_min_global_ratio() {
        let config = NucleationConfig {
            min_global_ratio: 0.05,
            critical_ratio: 3.0,
            ..Default::default()
        };
        let mut tracker = TraversalTracker::new(config.clone());
        tracker.record_traversal(vec![1, 2, 3, 4, 5]);

        let mut tombstone_map = HashMap::new();
        tombstone_map.insert(1, true);
        tombstone_map.insert(2, true);
        tombstone_map.insert(3, false);
        tombstone_map.insert(4, false);
        tombstone_map.insert(5, false);

        // Global ratio 0.005 < min_global_ratio (0.05)
        #[cfg(feature = "physio-nucleation")]
        let res = should_trigger_nucleation(&tracker, &tombstone_map, 0.005, &config);
        #[cfg(feature = "physio-nucleation")]
        assert!(res.is_none());
    }

    #[test]
    fn test_nucleation_trigger_on_local_oversaturation() {
        let config = NucleationConfig {
            critical_ratio: 3.0,
            min_global_ratio: 0.01,
            ..Default::default()
        };
        let mut tracker = TraversalTracker::new(config.clone());

        // Traversal visits nodes 1..=10
        tracker.record_traversal((1..=10).collect());

        // Total 100 nodes in tombstone map, 5 tombstones globally (5%)
        let mut tombstone_map = HashMap::new();
        for id in 1..=100 {
            tombstone_map.insert(id, false);
        }

        // Region (hot path nodes 1..=10) has 3 tombstones out of 10 (30%)
        tombstone_map.insert(1, true);
        tombstone_map.insert(2, true);
        tombstone_map.insert(3, true);
        // And 2 tombstones elsewhere (5 total = 5% global)
        tombstone_map.insert(99, true);
        tombstone_map.insert(100, true);

        // Global ratio = 0.05 (5%), Local ratio = 0.30 (30%) -> S_local = 6.0 > theta_c (3.0)
        #[cfg(feature = "physio-nucleation")]
        {
            let res = should_trigger_nucleation(&tracker, &tombstone_map, 0.05, &config);
            assert!(res.is_some());
            let triggered_nodes = res.unwrap(); // unwrap
            assert_eq!(triggered_nodes.len(), 10);
        }
    }
}
