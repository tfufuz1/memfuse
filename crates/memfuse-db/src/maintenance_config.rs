// FILE-CONTEXT
// ZWECK: Konfiguration des MaintenanceSchedulers für Background-Maintenance-Layer.
// INVARIANTEN: P10: Wiederverwendung bestehender Konfigurationsobjekte (DecayControllerConfig, PercolationConfig) ohne Duplikation.
// STAND: TS:2026-08-31T00:00:00Z

use crate::decay_controller::DecayControllerConfig;

#[cfg(feature = "graph-connectivity-health")]
use memfuse_graph::percolation::PercolationConfig;

#[cfg(not(feature = "graph-connectivity-health"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PercolationConfig {
    pub critical_threshold: f32,
    pub rebonding_similarity: f32,
    pub max_new_edges_per_pass: usize,
}

#[cfg(not(feature = "graph-connectivity-health"))]
impl Default for PercolationConfig {
    fn default() -> Self {
        Self {
            critical_threshold: 0.7,
            rebonding_similarity: 0.85,
            max_new_edges_per_pass: 100,
        }
    }
}

/// Zentrale Konfiguration für den `MaintenanceScheduler` (§10.2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MaintenanceConfig {
    /// Intervall zwischen Ticks in Sekunden. Default: 60s.
    pub tick_interval_secs: u64,

    // --- F-01 Adaptive Decay Controller ---
    /// Ob Decay-Eviction aktiviert ist. Default: true.
    #[serde(alias = "thermostat_enabled")]
    pub decay_enabled: bool,
    /// DecayController-Konfiguration (κ, base_half_life_tx, eviction_threshold).
    #[serde(flatten, alias = "thermostat")]
    pub decay_config: DecayControllerConfig,

    // --- F-03 Edge Reinforcement ---
    /// Edge-Reinforcement-Konfiguration (η, δ, W_max, ρ, Q, α).
    #[cfg(feature = "edge-reinforcement-learning")]
    pub edge_reinforcement: memfuse_graph::EdgeReinforcementConfig,

    // --- F-06 Perkolation ---
    /// Ob Perkolations-Gesundheitsprüfungen aktiviert sind. Default: true.
    pub percolation_enabled: bool,
    /// Perkolations-Konfiguration (critical_threshold, rebonding_similarity, max_new_edges_per_pass).
    #[serde(flatten)]
    pub percolation: PercolationConfig,

    // --- F-07 Replikatordynamik ---
    /// Ob Replikatordynamik-Updates aktiviert sind. Default: true.
    pub replicator_enabled: bool,
    /// Lernrate η für Replikatordynamik. Default: 0.05.
    pub replicator_lr: f32,

    // --- F-09 Kohärenz-Bonus ---
    /// Kohärenz-Bonus Parameter β. Default: 0.15.
    pub coherence_bonus_beta: f32,

    // --- MemoryConsolidation ---
    /// Ob Hintergrund-Konsolidierung aktiviert ist. Default: false (erfordert LLM).
    #[serde(alias = "sleep_cycle_enabled")]
    pub background_consolidation_enabled: bool,
    /// Schwellenwert der Episoden für Konsolidierungs-Triggern. Default: 50.
    #[serde(alias = "sleep_episode_threshold")]
    pub background_consolidation_episode_threshold: usize,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 60,
            decay_enabled: true,
            decay_config: DecayControllerConfig::default(),
            #[cfg(feature = "edge-reinforcement-learning")]
            edge_reinforcement: memfuse_graph::EdgeReinforcementConfig::default(),
            percolation_enabled: true,
            percolation: PercolationConfig::default(),
            replicator_enabled: true,
            replicator_lr: 0.05,
            coherence_bonus_beta: 0.15,
            background_consolidation_enabled: false,
            background_consolidation_episode_threshold: 50,
        }
    }
}
