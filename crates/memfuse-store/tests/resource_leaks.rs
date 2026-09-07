mod support;

use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::TempDir;

#[tokio::test]
async fn test_file_descriptor_leak_on_repeated_open_close() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    // Warm-up open/close
    {
        let storage = LsmStorage::new(config.clone()).await?;
        let tx = TxId::new(1);
        storage.put(tx, b"init_key", b"init_val").await?;
        storage.commit(tx).await?;
        storage.force_flush().await?;
    }

    let baseline_fds = support::get_open_fd_count();
    println!("Baseline Open File Descriptors: {}", baseline_fds);

    // 1000 Open / Read / Close cycles
    for i in 2..=1000 {
        let storage = LsmStorage::new(config.clone()).await?;
        let val = storage.get(b"init_key").await?;
        assert_eq!(val, Some(b"init_val".to_vec()));

        if i % 100 == 0 {
            let tx = TxId::new(i);
            storage.put(tx, b"k", b"v").await?;
            storage.commit(tx).await?;
        }
        storage.wait_shutdown().await;
        drop(storage);
    }

    let final_fds = support::get_open_fd_count();
    println!(
        "Final Open File Descriptors after 1000 cycles: {}",
        final_fds
    );

    if baseline_fds > 0 {
        let diff = (final_fds as isize - baseline_fds as isize).abs();
        assert!(
            diff <= 5,
            "File descriptor leak detected! Baseline: {}, Final: {}, Diff: {}",
            baseline_fds,
            final_fds,
            diff
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_memory_rss_footprint_under_continuous_load() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 64 * 1024, // 64 KB limit
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    let initial_rss = support::get_rss_bytes();
    println!("Initial RSS: {} MB", initial_rss / (1024 * 1024));

    // Sustained load: 10,000 writes with frequent flushes
    for i in 1..=10_000 {
        let tx = TxId::new(i);
        let k = format!("rss_key_{:05}", i);
        let v = format!("rss_val_{:05}_data_payload", i);
        storage.put(tx, k.as_bytes(), v.as_bytes()).await?;
        storage.commit(tx).await?;

        if i % 2000 == 0 {
            storage.force_flush().await?;
            storage.maybe_compact().await?;
        }
    }

    let final_rss = support::get_rss_bytes();
    println!("Final RSS after 10k ops: {} MB", final_rss / (1024 * 1024));

    if initial_rss > 0 {
        let rss_increase_mb = (final_rss.saturating_sub(initial_rss)) / (1024 * 1024);
        println!("RSS Increase: {} MB", rss_increase_mb);
        assert!(
            rss_increase_mb <= 150,
            "Memory growth exceeds 150 MB under sustained load! Growth: {} MB",
            rss_increase_mb
        );
    }

    Ok(())
}
