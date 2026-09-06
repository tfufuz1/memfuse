use memfuse_checkpoint::{global_orphan_registry, PinnedSeqNoOrphan, StateCheckpoint};
use memfuse_core::TxId;
use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_multi_instance_orphan_registry_physical_path_and_gc_isolation() {
    let tmp1 = TempDir::new().expect("temp dir 1");
    let tmp2 = TempDir::new().expect("temp dir 2");

    let db1 = MemFuse::open(tmp1.path()).await.expect("open db1");
    let db2 = MemFuse::open(tmp2.path()).await.expect("open db2");

    // 1. Verify that physical persistence paths of orphan registries are distinct and inside their respective db_path
    let reg1 = db1.orphan_registry();
    let reg2 = db2.orphan_registry();

    let expected_path1 = tmp1.path().join(".orphan_registry.json");
    let expected_path2 = tmp2.path().join(".orphan_registry.json");

    assert_eq!(
        reg1.get_orphan_pins(),
        Vec::<PinnedSeqNoOrphan>::new()
    );
    assert_eq!(
        reg2.get_orphan_pins(),
        Vec::<PinnedSeqNoOrphan>::new()
    );

    // 2. Register an orphan pin in Instance 1 and an orphan checkpoint in Instance 2
    let pin_orphan1 = PinnedSeqNoOrphan {
        seq_no: 10001,
        timestamp_ms: 12345678,
    };
    reg1.register_orphan_sync(pin_orphan1.clone());

    let cp_orphan2 = StateCheckpoint {
        tx_id: TxId::new(20002),
        timestamp_ms: 87654321,
        namespace: Some("default".to_string()),
    };
    reg2.register_checkpoint_sync(cp_orphan2.clone());

    // Verify physical persistence files exist and are distinct
    assert!(
        expected_path1.exists(),
        "Persistence file for DB1 must exist at {}",
        expected_path1.display()
    );
    assert!(
        expected_path2.exists(),
        "Persistence file for DB2 must exist at {}",
        expected_path2.display()
    );

    // 3. Verify Isolation: DB1 sees pin_orphan1 but NO checkpoints; DB2 sees cp_orphan2 but NO pins
    assert_eq!(reg1.get_orphan_pins(), vec![pin_orphan1.clone()]);
    assert!(reg1.get_orphaned_checkpoints().is_empty());

    assert!(reg2.get_orphan_pins().is_empty());
    assert_eq!(reg2.get_orphaned_checkpoints(), vec![cp_orphan2.clone()]);

    // 4. Perform GC / Clear / Drain on Instance 1
    reg1.clear_all();

    // Verify Instance 1 is cleared
    assert!(reg1.get_orphan_pins().is_empty());

    // Verify Instance 2 is completely unaffected by Instance 1's GC
    assert_eq!(reg2.get_orphaned_checkpoints(), vec![cp_orphan2]);
}

#[tokio::test]
async fn test_memfuse_open_does_not_touch_global_orphan_registry() {
    #[allow(deprecated)]
    {
        // Clear global registry prior to test
        memfuse_checkpoint::clear_all_orphaned_checkpoints();
    }

    let tmp1 = TempDir::new().expect("temp dir 1");
    let tmp2 = TempDir::new().expect("temp dir 2");

    // Open two independent MemFuse instances
    let db1 = MemFuse::open(tmp1.path()).await.expect("open db1");
    let db2 = MemFuse::open(tmp2.path()).await.expect("open db2");

    // Mutate and register orphans on both instances
    db1.orphan_registry().register_orphan_sync(PinnedSeqNoOrphan {
        seq_no: 777,
        timestamp_ms: 1000,
    });
    db2.orphan_registry().register_checkpoint_sync(StateCheckpoint {
        tx_id: TxId::new(888),
        timestamp_ms: 2000,
        namespace: Some("test".to_string()),
    });

    #[allow(deprecated)]
    {
        // Assert global deprecated orphan registry remains uninitialized/empty
        let global_pins = global_orphan_registry().get_orphans();
        let global_cps = memfuse_checkpoint::get_orphaned_checkpoints();

        assert!(
            global_pins.is_empty(),
            "Global orphan registry pins must remain empty after opening MemFuse instances"
        );
        assert!(
            global_cps.is_empty(),
            "Global orphan registry checkpoints must remain empty after opening MemFuse instances"
        );
    }
}

#[tokio::test]
async fn test_custom_orphan_registry_path_config() {
    let tmp = TempDir::new().expect("temp dir");
    let custom_orphan_file = tmp.path().join("custom_orphan_location.json");

    let config = MemFuseConfig {
        dimension: 4,
        orphan_registry_path: Some(custom_orphan_file.clone()),
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open with custom orphan registry path");

    db.orphan_registry().register_orphan_sync(PinnedSeqNoOrphan {
        seq_no: 999,
        timestamp_ms: 5000,
    });

    assert!(
        custom_orphan_file.exists(),
        "Custom orphan registry file must be created at specified path"
    );
}
