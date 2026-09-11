//! INTEGRATION GUIDE (post-JULES-01-merge):
//! 1. In LsmStorage::new(): spawn SystemPressureMonitor::run() als background task.
//! 2. In Collection::insert(): pressure_rx.borrow().pressure_level != Critical prüfen;
//!    bei Critical: tokio::time::sleep(backpressure_delay).await vor dem Insert.
//! 3. In memfuse-embed/TextEmbedder: pressure_rx subscriben; bei Critical:
//!    embed()-Calls mit Timeout versehen oder in Warteschlange einreihen.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const WAL_QUEUE_CRITICAL_THRESHOLD: usize = 500;
pub const WAL_QUEUE_ELEVATED_THRESHOLD: usize = 100;
pub const BLOCKING_UTIL_CRITICAL: f32 = 0.85;
pub const BLOCKING_UTIL_ELEVATED: f32 = 0.60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Normal,
    Elevated, // Warnung: System unter Last
    Critical, // Backpressure aktiv: neue Inserts verlangsamen
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemPressure {
    pub wal_queue_depth: usize,
    pub blocking_thread_utilization: f32, // 0.0–1.0
    pub embedding_queue_depth: usize,     // via Semaphore-Permits
    pub pressure_level: PressureLevel,
}

pub struct SystemPressureMonitor {
    pressure_tx: tokio::sync::watch::Sender<SystemPressure>,
    pub pressure_rx: tokio::sync::watch::Receiver<SystemPressure>,
    sampling_interval: Duration,
}

impl SystemPressureMonitor {
    pub fn new(sampling_interval: Duration) -> Self {
        let default_pressure = SystemPressure {
            wal_queue_depth: 0,
            blocking_thread_utilization: 0.0,
            embedding_queue_depth: 0,
            pressure_level: PressureLevel::Normal,
        };
        let (pressure_tx, pressure_rx) = tokio::sync::watch::channel(default_pressure);
        Self {
            pressure_tx,
            pressure_rx,
            sampling_interval,
        }
    }

    pub fn compute_pressure(
        &self,
        wal_depth: usize,
        blocking_util: f32,
        embedding_queue: usize,
        max_permits: usize,
    ) -> SystemPressure {
        let level = if blocking_util > BLOCKING_UTIL_CRITICAL
            || wal_depth > WAL_QUEUE_CRITICAL_THRESHOLD
            || (max_permits > 0 && embedding_queue == 0)
        {
            PressureLevel::Critical
        } else if blocking_util > BLOCKING_UTIL_ELEVATED || wal_depth > WAL_QUEUE_ELEVATED_THRESHOLD
        {
            PressureLevel::Elevated
        } else {
            PressureLevel::Normal
        };

        SystemPressure {
            wal_queue_depth: wal_depth,
            blocking_thread_utilization: blocking_util,
            embedding_queue_depth: embedding_queue,
            pressure_level: level,
        }
    }

    pub async fn run(
        self,
        cancellation: CancellationToken,
        wal_queue_depth_fn: impl Fn() -> usize + Send + 'static,
        embedding_permits_fn: impl Fn() -> usize + Send + 'static,
        max_embedding_permits: usize,
    ) {
        let mut last_level = PressureLevel::Normal;
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = tokio::time::sleep(self.sampling_interval) => {
                    let metrics = tokio::runtime::Handle::current().metrics();
                    let active_tasks = metrics.num_workers() as f32;
                    let queue_depth = metrics.global_queue_depth() as f32;
                    let blocking_util = if queue_depth > 0.0 {
                        (queue_depth / (active_tasks + queue_depth)).min(1.0)
                    } else {
                        0.0
                    };
                    let pressure = self.compute_pressure(
                        wal_queue_depth_fn(),
                        blocking_util,
                        embedding_permits_fn(),
                        max_embedding_permits,
                    );

                    if pressure.pressure_level != last_level {
                        tracing::info!(
                            previous_level = ?last_level,
                            new_level = ?pressure.pressure_level,
                            wal_depth = pressure.wal_queue_depth,
                            blocking_util = pressure.blocking_thread_utilization,
                            embedding_queue = pressure.embedding_queue_depth,
                            "SystemPressure level transitioned"
                        );
                        last_level = pressure.pressure_level;
                    }

                    let _ = self.pressure_tx.send(pressure);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_compute_pressure_thresholds() {
        let monitor = SystemPressureMonitor::new(Duration::from_millis(100));

        // Normal state
        let normal = monitor.compute_pressure(50, 0.2, 5, 10);
        assert_eq!(normal.pressure_level, PressureLevel::Normal);

        // Elevated states
        let elevated_wal = monitor.compute_pressure(150, 0.2, 5, 10);
        assert_eq!(elevated_wal.pressure_level, PressureLevel::Elevated);

        let elevated_util = monitor.compute_pressure(50, 0.7, 5, 10);
        assert_eq!(elevated_util.pressure_level, PressureLevel::Elevated);

        // Critical states
        let critical_wal = monitor.compute_pressure(501, 0.2, 5, 10);
        assert_eq!(critical_wal.pressure_level, PressureLevel::Critical);

        let critical_util = monitor.compute_pressure(50, 0.9, 5, 10);
        assert_eq!(critical_util.pressure_level, PressureLevel::Critical);

        let critical_permits = monitor.compute_pressure(50, 0.2, 0, 10);
        assert_eq!(critical_permits.pressure_level, PressureLevel::Critical);
    }

    #[tokio::test]
    async fn test_monitor_run_cancellation() {
        let monitor = SystemPressureMonitor::new(Duration::from_millis(50));
        let cancellation = CancellationToken::new();
        let cancel_token = cancellation.clone();

        let handle = tokio::spawn(async move {
            monitor.run(cancel_token, || 0, || 10, 10).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        cancellation.cancel();

        let res = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(
            res.is_ok(),
            "Monitor run loop should terminate cleanly on cancellation"
        );
    }

    #[tokio::test]
    async fn test_watch_receiver_updates() {
        let monitor = SystemPressureMonitor::new(Duration::from_millis(50));
        let mut rx = monitor.pressure_rx.clone();
        let cancellation = CancellationToken::new();
        let cancel_token = cancellation.clone();

        let wal_depth = Arc::new(AtomicUsize::new(10));
        let wal_depth_clone = Arc::clone(&wal_depth);

        let handle = tokio::spawn(async move {
            monitor
                .run(
                    cancel_token,
                    move || wal_depth_clone.load(Ordering::Relaxed),
                    || 10,
                    10,
                )
                .await;
        });

        // Initial state
        assert_eq!(rx.borrow().wal_queue_depth, 0);

        // Update wal depth to critical
        wal_depth.store(600, Ordering::Relaxed);

        // Wait for change on watch receiver
        rx.changed().await.unwrap();
        let updated = rx.borrow().clone();
        assert_eq!(updated.wal_queue_depth, 600);
        assert_eq!(updated.pressure_level, PressureLevel::Critical);

        cancellation.cancel();
        let _ = handle.await;
    }
}
