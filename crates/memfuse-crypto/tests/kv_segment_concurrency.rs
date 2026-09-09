// FILE-CONTEXT
// ZWECK: Concurrency Stress Test für TenantIsolatedKvStore und EvictionWorker unter hoher Parallellast.
// STAND: TS:2026-09-09T13:17:00Z (SESSION: a413a598)

use memfuse_core::TenantId;
use memfuse_security::kv_segment::{
    emergency_wipe, EvictionWorker, KvSegment, TenantIsolatedKvStore,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_concurrent_tenant_store_read_write() {
    let store = Arc::new(TenantIsolatedKvStore::new());
    let num_threads = 8;
    let ops_per_thread = 100;

    let handles: Vec<_> = (0..num_threads)
        .map(|t_idx| {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                let tenant = TenantId::try_new(t_idx as u64 + 1).unwrap();
                for op in 0..ops_per_thread {
                    let seg_id = (t_idx * ops_per_thread + op) as u64;
                    let seg = KvSegment::new(tenant, seg_id, vec![t_idx as u8; 128]);
                    store.insert_segment(tenant, seg);

                    let segs = store.get_segments(tenant);
                    assert!(!segs.is_empty());

                    let count = store.get_tenant_segment_len(tenant);
                    assert!(count > 0);

                    let raw_bytes = store.get_segment_bytes(tenant, seg_id);
                    assert!(raw_bytes.is_some());
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should not panic");
    }

    // Verify isolation and total count after concurrent execution
    for t_idx in 0..num_threads {
        let tenant = TenantId::try_new(t_idx as u64 + 1).unwrap();
        assert_eq!(
            store.get_tenant_segment_len(tenant),
            ops_per_thread as usize
        );
        assert_eq!(store.get_segments(tenant).len(), ops_per_thread as usize);
    }
}

#[test]
fn test_concurrent_eviction_worker_triggers() {
    let store = Arc::new(TenantIsolatedKvStore::new());
    let tenant = TenantId::try_new(1).unwrap();

    // Populate store with 50 segments
    for i in 0..50 {
        store.insert_segment(tenant, KvSegment::new(tenant, i, vec![0xFF; 256]));
    }

    let worker = Arc::new(EvictionWorker::spawn(Arc::clone(&store)));
    let num_trigger_threads = 4;

    let handles: Vec<_> = (0..num_trigger_threads)
        .map(|_| {
            let worker = Arc::clone(&worker);
            thread::spawn(move || {
                for _ in 0..10 {
                    worker.trigger_eviction(256);
                    thread::sleep(Duration::from_millis(2));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Trigger thread should not panic");
    }

    // Wait for worker queue to settle
    thread::sleep(Duration::from_millis(100));

    // Segments should have been evicted down
    let remaining = store.get_tenant_segment_len(tenant);
    assert!(
        remaining < 50,
        "Segments should have been evicted by worker thread"
    );
}

#[test]
fn test_concurrent_emergency_wipe_race() {
    let store = Arc::new(TenantIsolatedKvStore::new());
    let tenant = TenantId::try_new(1).unwrap();

    for i in 0..100 {
        store.insert_segment(tenant, KvSegment::new(tenant, i, vec![0x11; 64]));
    }

    let segs_ref1 = Arc::clone(&store);
    let segs_ref2 = Arc::clone(&store);

    let handle1 = thread::spawn(move || {
        emergency_wipe(&store_ref1);
    });

    let handle2 = thread::spawn(move || {
        emergency_wipe(&store_ref2);
    });

    handle1.join().unwrap();
    handle2.join().unwrap();

    assert_eq!(
        store.get_tenant_segment_len(tenant),
        0,
        "Store must be completely empty after emergency wipe"
    );
}
