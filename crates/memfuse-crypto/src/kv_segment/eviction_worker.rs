// FILE-CONTEXT
// ZWECK: Eviction-Worker (nicht-blockierender Hot-Path LRU) und emergency_wipe (synchroner Notfall).
// STAND: TS:2026-09-09T13:20:00Z (SESSION: 5665b844)

//! # Eviction-Architektur
//!
//! Dieses Modul trennt strikt zwischen zwei Eviction-Pfaden:
//!
//! 1. **`EvictionWorker`**: Für den regelmäßigen Hot-Path (z.B. VRAM > 80%-Trigger).
//!    Arbeitet auf einem **dedizierten OS-Thread** via `std::sync::mpsc`. Reicht
//!    Löschbefehle asynchron ein, damit synchrone `Zeroize`-Operationen NIEMALS
//!    den Tokio-Async-Executor blockieren.
//!
//! 2. **`emergency_wipe()`**: Für seltene Notfall-Löschungen (z.B. Prozess-Shutdown,
//!    Sicherheits-Alarm). Synchron, blockierend, garantiert vor Rückkehr
//!    vollständig abgeschlossen — bewusst anders als der reguläre Worker-Pfad.

use super::store::TenantIsolatedKvStore;
use std::sync::mpsc;
use std::sync::Arc;

enum EvictionCommand {
    EvictLru { target_free_bytes: usize },
    Shutdown,
}

/// Regelmäßiger Eviction-Pfad (VRAM > 80%-Trigger). NICHT im Async-Executor,
/// da regelmäßige Zeroize-Operationen den Tokio-Scheduler blockieren würden.
// AI-TAG[CONCURRENCY][MAJOR][RESOLVED] EvictionWorker is Sync via Mutex protection of sender and handle (ID: AGT-CRYPTO-edaee52e) (TS: 2026-09-09T13:17:00Z) (SESSION: a413a598)
pub struct EvictionWorker {
    sender: parking_lot::Mutex<mpsc::Sender<EvictionCommand>>,
    handle: parking_lot::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl EvictionWorker {
    /// Spawnt den Eviction-Worker auf einem dedizierten OS-Thread.
    pub fn spawn(store: Arc<TenantIsolatedKvStore>) -> Self {
        let (sender, receiver) = mpsc::channel::<EvictionCommand>();
        let handle = std::thread::Builder::new()
            .name("kv-eviction-worker".into())
            .spawn(move || {
                while let Ok(cmd) = receiver.recv() {
                    match cmd {
                        EvictionCommand::EvictLru { target_free_bytes } => {
                            let freed = store.evict_lru_global(target_free_bytes);
                            tracing::debug!(
                                freed_bytes = freed,
                                "KV eviction worker: LRU evict done"
                            );
                        }
                        EvictionCommand::Shutdown => break,
                    }
                }
            })
            .expect("failed to spawn kv-eviction-worker thread");

        Self {
            sender: parking_lot::Mutex::new(sender),
            handle: parking_lot::Mutex::new(Some(handle)),
        }
    }

    /// Nicht-blockierender Trigger vom Hot-Path aus.
    pub fn trigger_eviction(&self, target_free_bytes: usize) {
        let _ = self
            .sender
            .lock()
            .send(EvictionCommand::EvictLru { target_free_bytes });
    }

    /// Beendet den Worker-Thread geordnet.
    pub fn shutdown(&self) {
        let _ = self.sender.lock().send(EvictionCommand::Shutdown);
        if let Some(handle) = self.handle.lock().take() {
            let _ = handle.join();
        }
    }
}

impl Drop for EvictionWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// SEPARATER Pfad für Notfall-Löschung (z.B. Prozess-Shutdown, expliziter
/// Sicherheits-Trigger). Synchron, blockierend, garantiert vor Rückkehr abgeschlossen --
/// bewusst ANDERS als der reguläre Worker-Pfad.
pub fn emergency_wipe(store: &TenantIsolatedKvStore) {
    store.clear_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv_segment::segment::KvSegment;
    use memfuse_core::TenantId;
    use std::time::Duration;

    #[test]
    fn test_eviction_worker_nonblocking_trigger() {
        let store = Arc::new(TenantIsolatedKvStore::new());
        let tenant = TenantId::try_new(1).unwrap();
        let store = Arc::new(TenantIsolatedKvStore::new());
        store.insert_segment(tenant, KvSegment::new(tenant, 1, vec![0x11; 512]));
        store.insert_segment(tenant, KvSegment::new(tenant, 2, vec![0x22; 512]));

        let store = Arc::new(TenantIsolatedKvStore::new());
        store.insert_segment(tenant, seg1);
        store.insert_segment(tenant, seg2);

        let worker = EvictionWorker::spawn(Arc::clone(&store));

        // Trigger eviction of 500 bytes (should evict seg1 at index 0)
        let start = std::time::Instant::now();
        worker.trigger_eviction(500);
        let elapsed = start.elapsed();

        // Trigger must return immediately (non-blocking)
        assert!(
            elapsed < Duration::from_millis(50),
            "trigger_eviction must be non-blocking"
        );

        // Wait for worker thread to process command
        let mut freed = false;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(10));
            if store.get_tenant_segment_len(tenant) == 1 {
                freed = true;
                break;
            }
        }

        assert!(freed, "Worker should have evicted 1 segment in background");
        assert_eq!(store.get_segments(tenant), vec![2]);
    }

    #[test]
    fn test_lru_eviction_order_not_fifo() {
        let store = Arc::new(TenantIsolatedKvStore::new());
        let tenant = TenantId::try_new(1).unwrap();
        let store = Arc::new(TenantIsolatedKvStore::new());

        // Insert A, B, C
        store.insert_segment(tenant, KvSegment::new(tenant, 10, vec![0x11; 512]));
        std::thread::sleep(Duration::from_millis(1));
        store.insert_segment(tenant, KvSegment::new(tenant, 20, vec![0x22; 512]));
        std::thread::sleep(Duration::from_millis(1));
        store.insert_segment(tenant, KvSegment::new(tenant, 30, vec![0x33; 512]));

        // Touch A so it becomes most recently used
        let _ = store.get_segment_bytes(tenant, 10);

        let store = Arc::new(TenantIsolatedKvStore::new());
        store.insert_segment(tenant, seg_a);
        store.insert_segment(tenant, seg_b);
        store.insert_segment(tenant, seg_c);

        let worker = EvictionWorker::spawn(Arc::clone(&store));

        // Trigger eviction of 500 bytes (requires evicting 1 segment)
        worker.trigger_eviction(500);

        // Wait for worker thread to process command
        let mut freed = false;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(10));
            if store.get_tenant_segment_len(tenant) == 2 {
                freed = true;
                break;
            }
        }

        assert!(freed, "Worker should have evicted 1 segment");

        let remaining_ids = store.get_segments(tenant);

        // Under LRU: Segment B (clock 2, least recently used) was evicted.
        // Segment A (clock 4, most recently used) MUST be retained.
        assert!(
            remaining_ids.contains(&10),
            "Segment A (most recently used) must NOT be evicted"
        );
        assert!(
            !remaining_ids.contains(&20),
            "Segment B (least recently used) must be evicted"
        );
        assert!(remaining_ids.contains(&30), "Segment C must be retained");
    }

    #[test]
    fn test_emergency_wipe_synchronous_completion() {
        let store = TenantIsolatedKvStore::new();
        let tenant = TenantId::try_new(1).unwrap();
        let store = TenantIsolatedKvStore::new();
        store.insert_segment(tenant, KvSegment::new(tenant, 1, vec![0x11; 512]));
        store.insert_segment(tenant, KvSegment::new(tenant, 2, vec![0x22; 512]));

        let store = TenantIsolatedKvStore::new();
        store.insert_segment(tenant, seg1);
        store.insert_segment(tenant, seg2);

        assert_eq!(store.get_tenant_segment_len(tenant), 2);

        // Synchronous emergency wipe
        emergency_wipe(&store);

        // Immediately after return, all segments MUST be gone
        assert_eq!(
            store.get_tenant_segment_len(tenant),
            0,
            "emergency_wipe must immediately clear all segments"
        );
    }

    #[test]
    fn test_eviction_worker_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EvictionWorker>();
    }
}
