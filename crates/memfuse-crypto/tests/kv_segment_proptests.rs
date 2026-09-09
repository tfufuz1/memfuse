// FILE-CONTEXT
// ZWECK: Property-based Tests für KvSegment, TenantIsolatedKvStore und LRU-Eviction.
// STAND: TS:2026-09-09T13:17:00Z (SESSION: a413a598)

use memfuse_core::TenantId;
use memfuse_security::kv_segment::{KvSegment, TenantIsolatedKvStore};
use proptest::prelude::*;
use std::collections::HashSet;
use std::mem::ManuallyDrop;
use zeroize::Zeroize;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_kv_segment_creation_and_clock_monotonicity(
        tenant_num in 1u64..10000u64,
        segment_id in 1u64..100000u64,
        data in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let tenant = TenantId::try_new(tenant_num).unwrap();
        let seg1 = KvSegment::new(tenant, segment_id, data.clone());
        let seg2 = KvSegment::new(tenant, segment_id + 1, data.clone());

        prop_assert_eq!(seg1.tenant_id, tenant);
        prop_assert_eq!(seg1.segment_id, segment_id);
        prop_assert_eq!(seg1.len(), data.len());
        prop_assert_eq!(seg1.is_empty(), data.is_empty());
        prop_assert_eq!(seg1.as_bytes(), data.as_slice());

        // Logical clock of seg2 must be strictly greater than seg1
        prop_assert!(seg2.last_accessed() > seg1.last_accessed());

        // Touching seg1 updates its logical clock beyond seg2
        seg1.touch();
        prop_assert!(seg1.last_accessed() > seg2.last_accessed());
    }

    #[test]
    fn prop_tenant_isolation_strictness(
        tenant_a_num in 1u64..5000u64,
        tenant_b_num in 5001u64..10000u64,
        seg_ids_a in prop::collection::vec(1u64..10000u64, 1..20),
        seg_ids_b in prop::collection::vec(10001u64..20000u64, 1..20),
    ) {
        let store = TenantIsolatedKvStore::new();
        let tenant_a = TenantId::try_new(tenant_a_num).unwrap();
        let tenant_b = TenantId::try_new(tenant_b_num).unwrap();

        for &id in &seg_ids_a {
            let seg = KvSegment::new(tenant_a, id, vec![0xAA; 32]);
            store.insert_segment(tenant_a, seg);
        }

        for &id in &seg_ids_b {
            let seg = KvSegment::new(tenant_b, id, vec![0xBB; 32]);
            store.insert_segment(tenant_b, seg);
        }

        let retrieved_a = store.get_segments(tenant_a);
        let retrieved_b = store.get_segments(tenant_b);

        let set_a: HashSet<_> = retrieved_a.into_iter().collect();
        let set_b: HashSet<_> = retrieved_b.into_iter().collect();

        // No cross-tenant leakage allowed
        prop_assert!(set_a.intersection(&set_b).next().is_none());

        for id in &seg_ids_a {
            prop_assert!(set_a.contains(id));
            prop_assert!(!set_b.contains(id));
        }

        for id in &seg_ids_b {
            prop_assert!(set_b.contains(id));
            prop_assert!(!set_a.contains(id));
        }
    }

    #[test]
    fn prop_segment_zeroize_wipes_all_bytes(
        tenant_num in 1u64..10000u64,
        segment_id in 1u64..100000u64,
        data in prop::collection::vec(1u8..=255u8, 1..512),
    ) {
        let tenant = TenantId::try_new(tenant_num).unwrap();
        let mut segment = ManuallyDrop::new(KvSegment::new(tenant, segment_id, data.clone()));
        let ptr = segment.as_bytes().as_ptr();
        let len = segment.len();

        // Perform zeroize in place
        Zeroize::zeroize(&mut *segment);

        let expected = vec![0u8; len];
        // SAFETY: Pointer is valid as segment buffer is kept allocated inside ManuallyDrop
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, len);
            prop_assert_eq!(slice, expected.as_slice());
        }
    }
}
