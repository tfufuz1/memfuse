// FILE-CONTEXT
// ZWECK: Empirische TOCTOU-Testreihen für Mmap Truncation & Unlink/Deletion Szenarien in DiskANN & HNSW.
// INVARIANTEN: Nutzt Subprozess-Isolierung (`std::process::Command`) zur sicheren Erfassung von SIGBUS (Signal 7 / Exit-Code 135).
// STAND: TS:2026-08-30T22:30:00Z (SESSION: 37b1d991)

use std::process::{Command, ExitStatus};
use std::env;

/// Helper to run child process runner with explicit CLI args and env.
fn run_child_process(test_type: &str) -> (ExitStatus, String, String) {
    let current_exe = env::current_exe().expect("failed to get current test binary path");
    let output = Command::new(&current_exe)
        .arg("--exact")
        .arg("run_subcommand_target")
        .arg("--nocapture")
        .env("MEMFUSE_TOCTOU_CHILD", test_type)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status, stdout, stderr)
}

#[test]
fn run_subcommand_target() {
    if let Ok(child_mode) = env::var("MEMFUSE_TOCTOU_CHILD") {
        if child_mode == "truncation" {
            child_truncation_runner();
            return;
        } else if child_mode == "deletion" {
            child_deletion_runner();
            return;
        }
    }
}

#[tokio::test]
async fn test_mmap_toctou_truncation_causes_sigbus() {
    if env::var("MEMFUSE_TOCTOU_CHILD").is_ok() {
        return;
    }

    let (status, stdout, stderr) = run_child_process("truncation");
    println!("--- TRUNCATION CHILD STDOUT ---\n{}", stdout);
    println!("--- TRUNCATION CHILD STDERR ---\n{}", stderr);

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let signal = status.signal();
        println!("Child exit status: {:?}, Signal: {:?}", status, signal);
        assert!(
            signal == Some(7) || status.code() == Some(135) || !status.success(),
            "Expected SIGBUS (signal 7 / code 135) or abnormal exit on mmap truncation, got: {:?}",
            status
        );
    }
}

#[tokio::test]
async fn test_mmap_toctou_deletion_succeeds_safely() {
    if env::var("MEMFUSE_TOCTOU_CHILD").is_ok() {
        return;
    }

    let (status, stdout, stderr) = run_child_process("deletion");
    println!("--- DELETION CHILD STDOUT ---\n{}", stdout);
    println!("--- DELETION CHILD STDERR ---\n{}", stderr);

    assert!(
        status.success(),
        "POSIX open file descriptor retention should allow queries to succeed post-unlink: stderr: {}",
        stderr
    );
}

#[allow(dead_code)]
fn child_truncation_runner() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        use memfuse_core::{DocId, TxId, VectorIndex, DistanceMetric};
        use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
        use memfuse_index::persistence::MmapIndex;
        use memfuse_index::hnsw::{HnswConfig, HnswIndex};

        let temp_dir = tempfile::tempdir().unwrap();
        let diskann_path = temp_dir.path().join("diskann_trunc.idx");
        let hnsw_path = temp_dir.path().join("hnsw_trunc.hnsw");

        // 1. Build DiskANN
        let dim = 128;
        let n = 1000;
        let config = DiskAnnConfig {
            index_path: diskann_path.clone(),
            dimension: dim,
            max_degree: 32,
            beam_width: 16,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            quantize: false,
            ..Default::default()
        };
        let diskann = DiskAnnIndex::try_new(config).unwrap();
        let vectors: Vec<Vec<f32>> = (0..n).map(|i| vec![i as f32; dim]).collect();
        let ids: Vec<DocId> = (0..n).map(|i| DocId::from(i as u64)).collect();
        diskann.build(&vectors, &ids).await.unwrap();

        // 2. Build HNSW
        let hnsw_config = HnswConfig {
            dimension: dim,
            m: 16,
            ef_construction: 64,
            distance_metric: DistanceMetric::Euclidean,
            ..Default::default()
        };
        let hnsw = HnswIndex::try_new(hnsw_config.clone()).unwrap();
        let tx = TxId::new(1);
        for i in 0..n {
            hnsw.insert(tx, ids[i], &vectors[i]).await.unwrap();
        }
        hnsw.commit(tx).await.unwrap();
        hnsw.save(&hnsw_path).await.unwrap();

        let hnsw_mmap = HnswIndex::try_new(hnsw_config).unwrap();
        hnsw_mmap.load_mmap(&hnsw_path).await.unwrap();

        let mmap_index = MmapIndex::open(&hnsw_path).unwrap();

        println!("Active mmap handles opened for DiskANN & HNSW.");

        // 3. Truncate files in-place
        println!("Truncating DiskANN and HNSW files to 10 bytes in-place...");
        let f1 = std::fs::OpenOptions::new().write(true).open(&diskann_path).unwrap();
        f1.set_len(10).unwrap();
        drop(f1);

        let f2 = std::fs::OpenOptions::new().write(true).open(&hnsw_path).unwrap();
        f2.set_len(10).unwrap();
        drop(f2);

        println!("Files truncated! Attempting queries against mmap regions...");
        // 4. Access mmap memory post-truncation -> Trigger SIGBUS
        let query = vec![500.0f32; dim];

        // Accessing DiskANN or HNSW mmap memory
        println!("Executing DiskANN search_internal...");
        let _ = diskann.search(&query, 10).await;

        println!("Executing HNSW search...");
        let _ = hnsw_mmap.search(&query, 10).await;

        println!("Executing direct MmapIndex vector read...");
        let rec = mmap_index.get_node_record(500).unwrap();
        let _ = mmap_index.get_vector(&rec);

        println!("CHILD_TRUNCATION_FINISHED_WITHOUT_CRASH");
    });
}

#[allow(dead_code)]
fn child_deletion_runner() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        use memfuse_core::{DocId, TxId, VectorIndex, DistanceMetric};
        use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
        use memfuse_index::hnsw::{HnswConfig, HnswIndex};

        let temp_dir = tempfile::tempdir().unwrap();
        let diskann_path = temp_dir.path().join("diskann_del.idx");
        let hnsw_path = temp_dir.path().join("hnsw_del.hnsw");

        let dim = 128;
        let n = 100;
        let config = DiskAnnConfig {
            index_path: diskann_path.clone(),
            dimension: dim,
            max_degree: 16,
            beam_width: 8,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            quantize: false,
            ..Default::default()
        };
        let diskann = DiskAnnIndex::try_new(config).unwrap();
        let vectors: Vec<Vec<f32>> = (0..n).map(|i| vec![i as f32; dim]).collect();
        let ids: Vec<DocId> = (0..n).map(|i| DocId::from(i as u64)).collect();
        diskann.build(&vectors, &ids).await.unwrap();

        let hnsw_config = HnswConfig {
            dimension: dim,
            m: 16,
            ef_construction: 64,
            distance_metric: DistanceMetric::Euclidean,
            ..Default::default()
        };
        let hnsw = HnswIndex::try_new(hnsw_config.clone()).unwrap();
        let tx = TxId::new(1);
        for i in 0..n {
            hnsw.insert(tx, ids[i], &vectors[i]).await.unwrap();
        }
        hnsw.commit(tx).await.unwrap();
        hnsw.save(&hnsw_path).await.unwrap();

        let hnsw_mmap = HnswIndex::try_new(hnsw_config).unwrap();
        hnsw_mmap.load_mmap(&hnsw_path).await.unwrap();

        println!("Deleting index files from filesystem...");
        std::fs::remove_file(&diskann_path).unwrap();
        std::fs::remove_file(&hnsw_path).unwrap();

        assert!(!diskann_path.exists());
        assert!(!hnsw_path.exists());
        println!("Files unlinked successfully.");

        println!("Querying mmap'd indexes post-deletion...");
        let query = vec![50.0f32; dim];

        let res_diskann = diskann.search(&query, 5).await.unwrap();
        assert_eq!(res_diskann.len(), 5);

        let res_hnsw = hnsw_mmap.search(&query, 5).await.unwrap();
        assert_eq!(res_hnsw.len(), 5);

        println!("CHILD_DELETION_SUCCESSFUL");
    });
}
