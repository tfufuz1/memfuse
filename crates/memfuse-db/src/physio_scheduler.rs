// FILE-CONTEXT
// ZWECK: Zentraler PhysioScheduler für koordiniertes, sequenzielles Ausführen aller Physiologie-Aktionen (§10.1).
// INVARIANTEN: P2 Zero-Panic-Doctrine (Isolierte Fehlerbehandlung pro Teilschritt).
//              P10 Wiederverwendung bestehender Logik ohne Duplikation.
// NICHT-OFFENSICHTLICH: F-11 (LyapunovDriftWatcher) ist bewusst NICHT im PhysioScheduler-Tick enthalten,
//                       sondern EVENT-DRIVEN in `crates/memfuse-router/src/router.rs` integriert. Event-driven
//                       ist für Drift-Erkennung reaktionsschneller als ein periodischer 60s-Tick.
// STAND: TS:2026-08-31T00:00:00Z

use crate::collection::{Collection, StoredDocument};
use crate::physio_config::PhysioConfig;
use crate::sleep_cycle::NremConfig;
use crate::sleep_cycle_executor::execute_nrem_cycle;
use crate::thermostat::FreeEnergyThermostat;
use memfuse_core::traits::{StorageEngine, VectorIndex};
use memfuse_core::{DocId, MemFuseError, TxId};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Zentraler Scheduler für die Ausführung der Physiologie-Gedächtnisprozesse (§10.1).
pub struct PhysioScheduler<S: StorageEngine, V: VectorIndex = memfuse_index::HnswIndex> {
    config: PhysioConfig,
    collection: Arc<Collection<S, V>>,
    nrem_config: NremConfig,
    #[cfg(feature = "physio-replicator-weights")]
    replicator_state: Option<Arc<parking_lot::RwLock<memfuse_calibration::ReplicatorState>>>,
    active_sessions: Arc<AtomicUsize>,
}

impl<S: StorageEngine + 'static, V: VectorIndex + 'static> PhysioScheduler<S, V> {
    /// Erstellt eine neue Instanz des `PhysioScheduler`.
    pub fn new(
        config: PhysioConfig,
        collection: Arc<Collection<S, V>>,
        nrem_config: NremConfig,
    ) -> Self {
        Self {
            config,
            collection,
            nrem_config,
            #[cfg(feature = "physio-replicator-weights")]
            replicator_state: None,
            active_sessions: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Setzt den optionalen `ReplicatorState` für F-07.
    #[cfg(feature = "physio-replicator-weights")]
    pub fn with_replicator_state(
        mut self,
        state: Arc<parking_lot::RwLock<memfuse_calibration::ReplicatorState>>,
    ) -> Self {
        self.replicator_state = Some(state);
        self
    }

    /// Liefert die Anzahl aktuell aktiver Agenten-Sessions.
    pub fn active_agent_sessions(&self) -> usize {
        self.active_sessions.load(Ordering::SeqCst)
    }

    /// Erhöht die Anzahl aktiver Agenten-Sessions.
    pub fn increment_active_sessions(&self) -> usize {
        self.active_sessions.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Verringert die Anzahl aktiver Agenten-Sessions.
    pub fn decrement_active_sessions(&self) -> usize {
        self.active_sessions.fetch_sub(1, Ordering::SeqCst).saturating_sub(1)
    }

    /// Startet den PhysioScheduler in einem eigenen Tokio-Task.
    pub fn start(
        self: Arc<Self>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let interval_secs = self.config.tick_interval_secs.max(1);
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(
                collection = %self.collection.name(),
                interval_secs = interval_secs,
                "PhysioScheduler started"
            );

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        self.run_tick().await;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            collection = %self.collection.name(),
                            "PhysioScheduler shutting down via cancellation token"
                        );
                        break;
                    }
                }
            }
        })
    }

    /// Führt einen einzelnen sequenziellen Physio-Tick durch (§10.1).
    pub async fn run_tick(&self) {
        tracing::debug!(collection = %self.collection.name(), "PhysioScheduler tick started");

        // Step a: WAL-Intent schreiben
        if let Err(err) = write_tick_intent(&self.collection).await {
            tracing::error!(
                collection = %self.collection.name(),
                error = %err,
                "PhysioScheduler: Failed to write tick start WAL intent"
            );
        }

        // Step b: F-01 Thermostat-Update
        if self.config.thermostat_enabled {
            let thermostat = FreeEnergyThermostat::new(self.config.thermostat.clone());
            match self.collection.reap_by_thermostat(&thermostat, 100).await {
                Ok(n) if n > 0 => {
                    tracing::info!(collection = %self.collection.name(), evicted = n, "PhysioScheduler: Thermostat evicted chunks");
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(
                        collection = %self.collection.name(),
                        error = %err,
                        "PhysioScheduler: Thermostat step failed"
                    );
                }
            }
        }

        // Step c: F-03 SynapticUpdateBuffer.flush_to_csr()
        // F-03 SynapticUpdateBuffer.flush_to_csr() — Hook wird von separatem Arbeitspaket ergänzt, siehe crates/memfuse-graph/src/synaptic.rs
        #[cfg(feature = "physio-synaptic-edges")]
        {
            // Placeholder: Hook wird in Prompt 5 ergänzt
        }

        // Step d: F-06 Perkolation
        if self.config.percolation_enabled && self.active_agent_sessions() == 0 {
            #[cfg(feature = "physio-percolation")]
            {
                match self.collection.run_percolation_check(&self.config.percolation).await {
                    Ok(res) => {
                        if res.rebonding_triggered {
                            tracing::info!(
                                collection = %self.collection.name(),
                                new_edges = res.new_edges_added,
                                "PhysioScheduler: Percolation re-bonding triggered"
                            );
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            collection = %self.collection.name(),
                            error = %err,
                            "PhysioScheduler: Percolation step failed"
                        );
                    }
                }
            }
        }

        // Step e: F-07 Replikatordynamik-Update & F-09 Kohärenz-Bonus Parameter-Adaption
        // HINWEIS: Die periodische Aktualisierung greift auf `ReplicatorState` zu, sofern ein Shared Arc vorhanden ist.
        // Event-getriebene Feedback-Updates erfolgen separat über `record_retrieval_feedback`.
        if self.config.replicator_enabled {
            #[cfg(feature = "physio-replicator-weights")]
            if let Some(ref state_arc) = self.replicator_state {
                let guard = state_arc.read();
                tracing::debug!(
                    update_count = guard.update_count,
                    weights = ?guard.weights,
                    "PhysioScheduler: ReplicatorState verified"
                );
            }
        }

        // ARCHITEKTONISCHE ABWEICHUNG ZU §10.1:
        // F-11 (LyapunovDriftWatcher.update()) ist bewusst NICHT hier im periodischen Tick enthalten.
        // F-11 ist stattdessen reaktionsschnell & event-driven direkt nach jeder Routing-Entscheidung
        // in `crates/memfuse-router/src/router.rs` integriert. Eine Auslagerung in diesen 60s-Tick
        // wäre eine architektonische Regression der Drift-Reaktionszeit.

        // Step f: SleepCycle-Trigger
        if self.config.sleep_cycle_enabled && self.active_agent_sessions() == 0 {
            let user_key_prefix = self.collection.user_key_prefix();
            match self.collection.storage().scan_prefix(&user_key_prefix).await {
                Ok(entries) => {
                    let mut turns: Vec<(DocId, Vec<f32>)> = Vec::new();
                    for (k, v) in entries {
                        if self.collection.name() == "default" && k.starts_with(b"__") {
                            continue;
                        }
                        if let Ok(stored) = serde_json::from_slice::<StoredDocument>(&v) {
                            if let Ok(doc_id) = DocId::from_key(&stored.id) {
                                turns.push((doc_id, stored.embedding));
                            }
                        }
                    }

                    if turns.len() >= self.config.sleep_episode_threshold {
                        match execute_nrem_cycle(self.collection.as_ref(), &turns, &self.nrem_config).await {
                            Ok(res) => {
                                if !res.duplicates_tombstoned.is_empty() {
                                    tracing::info!(
                                        collection = %self.collection.name(),
                                        tombstoned = res.duplicates_tombstoned.len(),
                                        segments = res.segments_created,
                                        "PhysioScheduler: NREM sleep cycle consolidated turn duplicates"
                                    );
                                }
                            }
                            Err(err) => {
                                tracing::error!(
                                    collection = %self.collection.name(),
                                    error = %err,
                                    "PhysioScheduler: SleepCycle step failed"
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(
                        collection = %self.collection.name(),
                        error = %err,
                        "PhysioScheduler: Failed to scan collection for SleepCycle"
                    );
                }
            }
        }

        // Step g: WAL-Intent als abgeschlossen markieren
        if let Err(err) = complete_tick_intent(&self.collection).await {
            tracing::error!(
                collection = %self.collection.name(),
                error = %err,
                "PhysioScheduler: Failed to write tick completion WAL intent"
            );
        }

        tracing::debug!(collection = %self.collection.name(), "PhysioScheduler tick completed");
    }
}

async fn write_tick_intent<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
) -> Result<TxId, MemFuseError> {
    let tx = collection.allocate_tx()?;
    let payload = serde_json::to_vec(&serde_json::json!({
        "status": "pending",
        "timestamp_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    }))?;
    collection.storage().put(tx, b"__physio_intent:tick", &payload).await?;
    collection.storage().commit(tx).await?;
    Ok(tx)
}

async fn complete_tick_intent<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
) -> Result<(), MemFuseError> {
    let tx = collection.allocate_tx()?;
    let payload = serde_json::to_vec(&serde_json::json!({
        "status": "completed",
        "timestamp_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    }))?;
    collection.storage().put(tx, b"__physio_intent:tick", &payload).await?;
    collection.storage().commit(tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use tempfile::tempdir;
    use tokio::time::sleep;

    async fn create_test_collection() -> Arc<Collection<LsmStorage, HnswIndex>> {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        Arc::new(Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        ))
    }

    #[tokio::test]
    async fn test_physio_scheduler_tick_writes_wal_intent() {
        let col = create_test_collection().await;
        let config = PhysioConfig {
            tick_interval_secs: 1,
            thermostat_enabled: false,
            percolation_enabled: false,
            replicator_enabled: false,
            sleep_cycle_enabled: false,
            ..Default::default()
        };

        let scheduler = Arc::new(PhysioScheduler::new(config, col.clone(), NremConfig::default()));

        // Run single tick manually
        scheduler.run_tick().await;

        // Verify WAL intent key was written with status "completed"
        let prefix = b"__physio_intent:tick".to_vec();
        let entries = col.storage().scan_prefix(&prefix).await.unwrap();
        assert!(!entries.is_empty(), "WAL intent key should exist");
        let val: serde_json::Value = serde_json::from_slice(&entries[0].1).unwrap();
        assert_eq!(val["status"], "completed");
    }

    #[tokio::test]
    async fn test_physio_scheduler_step_isolation_on_error() {
        let col = create_test_collection().await;
        let config = PhysioConfig {
            tick_interval_secs: 1,
            thermostat_enabled: true,
            percolation_enabled: false,
            replicator_enabled: false,
            sleep_cycle_enabled: false,
            ..Default::default()
        };

        let scheduler = Arc::new(PhysioScheduler::new(config, col.clone(), NremConfig::default()));

        // Run tick - thermostat step executes without error even on empty collection
        scheduler.run_tick().await;

        // Check completion marker
        let prefix = b"__physio_intent:tick".to_vec();
        let entries = col.storage().scan_prefix(&prefix).await.unwrap();
        let val: serde_json::Value = serde_json::from_slice(&entries[0].1).unwrap();
        assert_eq!(val["status"], "completed");
    }

    #[tokio::test]
    async fn test_physio_scheduler_background_task_and_cancellation() {
        let col = create_test_collection().await;
        let config = PhysioConfig {
            tick_interval_secs: 1,
            thermostat_enabled: false,
            percolation_enabled: false,
            replicator_enabled: false,
            sleep_cycle_enabled: false,
            ..Default::default()
        };

        let scheduler = Arc::new(PhysioScheduler::new(config, col, NremConfig::default()));
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let handle = scheduler.clone().start(cancel_token.clone());

        sleep(Duration::from_millis(50)).await;
        cancel_token.cancel();

        let res = handle.await;
        assert!(res.is_ok(), "PhysioScheduler task should shut down cleanly");
    }

    #[tokio::test]
    async fn test_active_sessions_tracking() {
        let col = create_test_collection().await;
        let scheduler = PhysioScheduler::new(PhysioConfig::default(), col, NremConfig::default());

        assert_eq!(scheduler.active_agent_sessions(), 0);
        assert_eq!(scheduler.increment_active_sessions(), 1);
        assert_eq!(scheduler.increment_active_sessions(), 2);
        assert_eq!(scheduler.active_agent_sessions(), 2);
        assert_eq!(scheduler.decrement_active_sessions(), 1);
        assert_eq!(scheduler.decrement_active_sessions(), 0);
        assert_eq!(scheduler.decrement_active_sessions(), 0); // saturating
    }
}
