// SPEC-038: Adaptive Index-Ressourcen-Allokation
// Status: ✅ IMPLEMENTIERT | Basis: SPEC-034 §3.4
//
// Mission: Dynamische Umverteilung von ResourceBudget zwischen Indizes
// basierend auf der aktuellen Abfrage-Last.

use crate::budget::{Domain, ResourceTracker};
use crate::error::Result;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tracing::instrument;

/// Rollierendes Fenster der letzten N Query-Typen zur Workload-Analyse.
pub struct AdaptiveAllocator {
    tracker: Arc<ResourceTracker>,
    /// Rollierendes Fenster der letzten Query-Typen (Standard: 1000)
    query_history: Arc<RwLock<VecDeque<QueryType>>>,
    max_history: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum QueryType {
    Spatial,
    Vector,
    Metadata,
    Sparse,
    Graph,
    /// Abfrage, die mehrere Indizes gleichzeitig beansprucht.
    Hybrid,
    /// SPEC-041: Tensor engine workload (In-database embedding generation).
    Embedding,
}

#[derive(Debug, Clone)]
pub struct AllocationPlan {
    pub hnsw_budget_ratio: f32,
    pub spatial_budget_ratio: f32,
    pub metadata_budget_ratio: f32,
    pub sparse_budget_ratio: f32,
    pub storage_budget_ratio: f32,
    pub graph_budget_ratio: f32,
    pub compute_budget_ratio: f32,
}

impl Default for AllocationPlan {
    fn default() -> Self {
        // Konservative Gleichverteilung bei unbekanntem Workload
        Self {
            hnsw_budget_ratio: 0.20,
            spatial_budget_ratio: 0.15,
            metadata_budget_ratio: 0.15,
            sparse_budget_ratio: 0.15,
            storage_budget_ratio: 0.15,
            graph_budget_ratio: 0.10,
            compute_budget_ratio: 0.10,
        }
    }
}

impl AdaptiveAllocator {
    /// Erstellt einen neuen AdaptiveAllocator.
    pub fn new(tracker: Arc<ResourceTracker>, max_history: usize) -> Self {
        Self {
            tracker,
            query_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history))),
            max_history,
        }
    }

    /// Protokolliert eine Query für die adaptive Analyse.
    /// Thread-safe, minimaler Overhead im Query-Hot-Path.
    pub fn record_query(&self, query_type: QueryType) {
        let mut history = self.query_history.write();
        if history.len() >= self.max_history {
            history.pop_front();
        }
        history.push_back(query_type);
    }

    /// Berechnet die optimale Ressourcenverteilung basierend auf der Historie.
    /// [DETERMINISM]: O(max_history)
    pub fn rebalance(&self) -> AllocationPlan {
        let history = self.query_history.read();
        let total = history.len() as f32;

        if total == 0.0 {
            return AllocationPlan::default();
        }

        // Zähle Vorkommen der Query-Typen
        // INVARIANT: INV-R4 — Alle Domains erhalten einen Mindestanteil.
        let mut vector_count = 0.0_f32;
        let mut spatial_count = 0.0_f32;
        let mut sparse_count = 0.0_f32;
        let mut graph_count = 0.0_f32;
        let mut metadata_count = 0.0_f32;
        let mut compute_count = 0.0_f32;

        for q in history.iter() {
            match q {
                QueryType::Vector => vector_count += 1.0,
                QueryType::Spatial => spatial_count += 1.0,
                QueryType::Sparse => sparse_count += 1.0,
                QueryType::Graph => graph_count += 1.0,
                QueryType::Metadata => metadata_count += 1.0,
                // SPEC-038: Hybrid verteilt gleichmäßig auf alle Index-Typen.
                // [DETERMINISM]: O(1) pro Entry
                QueryType::Hybrid => {
                    vector_count += 0.5;
                    spatial_count += 0.5;
                    sparse_count += 0.5;
                    graph_count += 0.5;
                    metadata_count += 0.5;
                }
                // SPEC-041: Compute memory load tracking
                QueryType::Embedding => compute_count += 1.0,
            }
        }

        // SPEC-038: Dynamische Gewichtung zwischen Vector und Graph
        // Vector-Queries (HNSW) vs. Graph-Queries (CSR/Adjacency)
        let total_weight = vector_count
            + spatial_count
            + sparse_count
            + graph_count
            + metadata_count
            + compute_count;

        // Storage-Minimum immer garantiert (SPEC-032 INV-R4)
        let storage_ratio = 0.15_f32;
        let mut remaining = 1.0 - storage_ratio;

        // Minimums (Safety Floor)
        let min_hnsw = 0.10;
        let min_spatial = 0.05;
        let min_sparse = 0.05;
        let min_graph = 0.05;
        let min_meta = 0.05;
        let min_compute = 0.05;

        remaining -= min_hnsw + min_spatial + min_sparse + min_graph + min_meta + min_compute;

        let (hnsw_dyn, spatial_dyn, sparse_dyn, graph_dyn, meta_dyn, compute_dyn) =
            if total_weight > 0.0 {
                (
                    (vector_count / total_weight) * remaining,
                    (spatial_count / total_weight) * remaining,
                    (sparse_count / total_weight) * remaining,
                    (graph_count / total_weight) * remaining,
                    (metadata_count / total_weight) * remaining,
                    (compute_count / total_weight) * remaining,
                )
            } else {
                let share = remaining / 6.0;
                (share, share, share, share, share, share)
            };

        AllocationPlan {
            hnsw_budget_ratio: min_hnsw + hnsw_dyn,
            spatial_budget_ratio: min_spatial + spatial_dyn,
            metadata_budget_ratio: min_meta + meta_dyn,
            sparse_budget_ratio: min_sparse + sparse_dyn,
            storage_budget_ratio: storage_ratio,
            graph_budget_ratio: min_graph + graph_dyn,
            compute_budget_ratio: min_compute + compute_dyn,
        }
    }

    /// Wendet den berechneten Plan auf den ResourceTracker an.
    #[instrument(skip(self, plan))]
    pub fn apply_plan(&self, plan: &AllocationPlan) {
        let total_bytes = self.tracker.total_budget_bytes();

        // Übertrag der Ratios in absolute Byte-Limits
        self.tracker.set_budget(
            Domain::Hnsw,
            (total_bytes as f32 * plan.hnsw_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Spatial,
            (total_bytes as f32 * plan.spatial_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Metadata,
            (total_bytes as f32 * plan.metadata_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Sparse,
            (total_bytes as f32 * plan.sparse_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Storage,
            (total_bytes as f32 * plan.storage_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Graph,
            (total_bytes as f32 * plan.graph_budget_ratio) as u64,
        );
        self.tracker.set_budget(
            Domain::Compute,
            (total_bytes as f32 * plan.compute_budget_ratio) as u64,
        );

        tracing::info!(
            hnsw_ratio = plan.hnsw_budget_ratio,
            graph_ratio = plan.graph_budget_ratio,
            compute_ratio = plan.compute_budget_ratio,
            "Resource-Budget rebalanciert (SPEC-038)"
        );
    }
}

/// Startet den Hintergrund-Rebalancing-Loop.
pub async fn start_adaptive_rebalancer_loop(
    allocator: Arc<AdaptiveAllocator>,
    interval: Duration,
) -> Result<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let plan = allocator.rebalance();
            allocator.apply_plan(&plan);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{Domain, ResourceBudget};

    #[test]
    fn test_adaptive_rebalance_vector_heavy() {
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget::default()));
        let allocator = AdaptiveAllocator::new(tracker, 100);

        for _ in 0..80 {
            allocator.record_query(QueryType::Vector);
        }
        for _ in 0..20 {
            allocator.record_query(QueryType::Spatial);
        }

        let plan = allocator.rebalance();
        assert!(plan.hnsw_budget_ratio > plan.spatial_budget_ratio);
    }

    #[test]
    fn test_adaptive_rebalance_equilibrium() {
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget::default()));
        let allocator = AdaptiveAllocator::new(tracker, 100);

        // Keine History -> Default Plan
        let plan = allocator.rebalance();
        let default = AllocationPlan::default();
        assert_eq!(plan.storage_budget_ratio, default.storage_budget_ratio);
    }

    /// SPEC-038: Hybrid-Queries müssen Graph und Metadata anteilig erhalten.
    #[test]
    fn test_hybrid_distributes_to_graph() {
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget::default()));
        let allocator = AdaptiveAllocator::new(tracker, 100);

        // 100 Hybrid-Queries — alle Domains erhalten Anteil.
        for _ in 0..100 {
            allocator.record_query(QueryType::Hybrid);
        }

        let plan = allocator.rebalance();
        // Graph muss ein positives Budget erhalten (kein Zero-out).
        assert!(
            plan.graph_budget_ratio > 0.0,
            "Graph-Budget muss bei Hybrid > 0 sein, war: {}",
            plan.graph_budget_ratio
        );
        // Metadata ebenfalls.
        assert!(
            plan.metadata_budget_ratio > 0.0,
            "Metadata-Budget muss bei Hybrid > 0 sein, war: {}",
            plan.metadata_budget_ratio
        );
        // Storage-Minimum eingehalten.
        assert_eq!(plan.storage_budget_ratio, 0.15);
    }

    /// SPEC-038: Das Rollier-Fenster darf nie über max_history wachsen.
    #[test]
    fn test_history_rollover() {
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget::default()));
        let max_history = 100;
        let allocator = AdaptiveAllocator::new(tracker, max_history);

        // Doppelt so viele Einträge wie das Fenster zulässt.
        for _ in 0..200 {
            allocator.record_query(QueryType::Vector);
        }

        let history_len = allocator.query_history.read().len();
        assert_eq!(
            history_len, max_history,
            "History darf max_history nicht überschreiten (got {})",
            history_len
        );
    }

    /// SPEC-038: apply_plan muss domain_limit im ResourceTracker tatsächlich aktualisieren.
    #[test]
    fn test_vector_vs_graph_rebalance() {
        let tracker = Arc::new(ResourceTracker::new(ResourceBudget::default()));
        let allocator = AdaptiveAllocator::new(tracker, 100);

        // Vector heavy workload
        for _ in 0..70 {
            allocator.record_query(QueryType::Vector);
        }
        for _ in 0..30 {
            allocator.record_query(QueryType::Graph);
        }

        let plan_v = allocator.rebalance();

        // Reset and Graph heavy workload
        let allocator_g = AdaptiveAllocator::new(
            Arc::new(ResourceTracker::new(ResourceBudget::default())),
            100,
        );
        for _ in 0..30 {
            allocator_g.record_query(QueryType::Vector);
        }
        for _ in 0..70 {
            allocator_g.record_query(QueryType::Graph);
        }
        let plan_g = allocator_g.rebalance();

        assert!(plan_v.hnsw_budget_ratio > plan_g.hnsw_budget_ratio);
        assert!(plan_g.graph_budget_ratio > plan_v.graph_budget_ratio);

        // Both should still have safety floors
        assert!(plan_v.graph_budget_ratio >= 0.05);
        assert!(plan_g.hnsw_budget_ratio >= 0.10);
    }

    #[test]
    fn test_apply_plan_updates_domain_limits() {
        let budget = ResourceBudget {
            memory_limit: 1_000_000,
            cpu_cycle_limit: u64::MAX,
        };
        let tracker = Arc::new(ResourceTracker::new(budget));
        let allocator = AdaptiveAllocator::new(Arc::clone(&tracker), 100);

        // Reine Vector-Last → HNSW bekommt maximalen Anteil.
        for _ in 0..100 {
            allocator.record_query(QueryType::Vector);
        }

        let plan = allocator.rebalance();
        allocator.apply_plan(&plan);

        let expected_hnsw = (budget.memory_limit as f32 * plan.hnsw_budget_ratio) as u64;
        let actual_hnsw = tracker.domain_limit(Domain::Hnsw);

        // Erlaubt Rundungsfehler von ±1 Byte.
        assert!(
            actual_hnsw.abs_diff(expected_hnsw) <= 1,
            "domain_limit(Hnsw) sollte ~{} sein, war {}",
            expected_hnsw,
            actual_hnsw
        );
    }
}
