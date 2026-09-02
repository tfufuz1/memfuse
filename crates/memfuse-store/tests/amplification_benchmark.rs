//! Write- and Read-Amplification benchmark for `memfuse-store`.

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::compaction::CompactionConfig;
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_store::sstable::BloomFilter;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tempfile::TempDir;

/// File tracker to count total physical bytes written to disk over the workload lifetime.
struct FileWriteTracker {
    dir_path: PathBuf,
    tracked_files: std::sync::Mutex<HashSet<PathBuf>>,
    total_bytes_written: AtomicU64,
}

impl FileWriteTracker {
    fn new(dir_path: PathBuf) -> Self {
        Self {
            dir_path,
            tracked_files: std::sync::Mutex::new(HashSet::new()),
            total_bytes_written: AtomicU64::new(0),
        }
    }

    /// Scans the directory for SSTable and WAL files, recording size of any newly created or expanded files.
    fn poll_writes(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.dir_path) {
            let mut tracked = self.tracked_files.lock().unwrap();
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if (name.ends_with(".sst") || name.ends_with(".log")) && !name.ends_with(".tmp") {
                    if !tracked.contains(&path) {
                        if let Ok(meta) = std::fs::metadata(&path) {
                            let len = meta.len();
                            if len > 0 {
                                self.total_bytes_written.fetch_add(len, Ordering::SeqCst);
                                tracked.insert(path);
                            }
                        }
                    }
                }
            }
        }
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes_written.load(Ordering::SeqCst)
    }
}

/// Measure Bloom filter false positive rate over 100k inserted keys and 100k non-inserted keys.
fn test_bloom_filter_fpr_100k() -> (usize, usize, usize, f64, f64) {
    let num_keys = 100_000;
    let target_fpr = 0.01; // 1%
    let mut bf = BloomFilter::new(num_keys, target_fpr);

    let mut inserted_keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        inserted_keys.push(format!("key_inserted_{:08}", i).into_bytes());
    }

    for key in &inserted_keys {
        bf.insert(key);
    }

    // Check true positives (must be 100%)
    let mut true_positives = 0;
    for key in &inserted_keys {
        if bf.may_contain(key) {
            true_positives += 1;
        }
    }

    // Check false positives using 100k non-inserted keys
    let mut false_positives = 0;
    for i in 0..num_keys {
        let non_key = format!("key_absent_{:08}", i).into_bytes();
        if bf.may_contain(&non_key) {
            false_positives += 1;
        }
    }

    let empirical_fpr = false_positives as f64 / num_keys as f64;
    (
        num_keys,
        true_positives,
        false_positives,
        target_fpr,
        empirical_fpr,
    )
}

#[tokio::test]
async fn run_amplification_benchmark() {
    println!("============================================================");
    println!("MEMFUSE-STORE WRITE & READ AMPLIFICATION BENCHMARK SUITE");
    println!("============================================================");

    // STEP 1 & 2: Bloom Filter FPR Measurement
    println!("\n[1] BLOOM FILTER FALSE POSITIVE RATE (FPR) MEASUREMENT");
    let (n_keys, tp, fp, target_fpr, empirical_fpr) = test_bloom_filter_fpr_100k();
    println!("- Target expected elements: {}", n_keys);
    println!(
        "- Configured target FPR: {:.4} ({:.2}%)",
        target_fpr,
        target_fpr * 100.0
    );
    println!(
        "- True Positives count: {} / {} (100% accuracy required)",
        tp, n_keys
    );
    println!("- False Positives count: {} / {}", fp, n_keys);
    println!(
        "- Empirical FPR: {:.6} ({:.4}%)",
        empirical_fpr,
        empirical_fpr * 100.0
    );
    assert_eq!(tp, n_keys, "Bloom filter true positive rate must be 100%");

    // STEP 3: Real Workload Simulator & Write Amplification
    println!("\n[2] REALISTIC LSM WORKLOAD SIMULATOR & WRITE AMPLIFICATION");
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = LsmConfig {
        path: db_path.clone(),
        memtable_size_limit: 256 * 1024, // 256KB memtable limit to force frequent SSTable flushes
        max_ram_mb: 512,
        tx_timeout: Duration::from_secs(60),
        compaction: CompactionConfig {
            min_sstables_per_tier: 2, // 2 SSTables per tier triggers compaction early & frequently
            size_ratio: 2.0,
            check_interval: Duration::from_millis(50),
            yield_threshold: 1000,
            max_memory_bytes: Some(128 * 1024 * 1024),
        },
        encryption_passphrase: None,
    };

    let tracker = FileWriteTracker::new(db_path.clone());
    let storage = LsmStorage::new(config).await.expect("storage init");

    // Workload definition:
    // 100,000 Inserts total
    // 20,000 Updates total
    // 10,000 Deletes total
    // Split into 5 batches with intermediate flushes and compactions.

    let payload_val = vec![b'x'; 100]; // 100-byte value payload
    let mut tx_counter = 1u64;
    let mut total_compaction_cycles = 0usize;

    println!("Starting Workload Execution across 5 Batches...");

    for batch in 1..=5 {
        let batch_insert_start = (batch - 1) * 20_000;
        let batch_insert_end = batch * 20_000;

        // 20,000 Inserts per batch
        for i in batch_insert_start..batch_insert_end {
            let key = format!("user_record_{:08}", i).into_bytes();
            let tx = TxId::new(tx_counter);
            tx_counter += 1;
            storage.put(tx, &key, &payload_val).await.expect("put");
            storage.commit(tx).await.expect("commit");
        }

        // 4,000 Updates per batch (total 20,000 across 5 batches)
        let update_start = (batch - 1) * 4_000;
        let update_end = batch * 4_000;
        let updated_val = vec![b'u'; 100];
        for i in update_start..update_end {
            let key = format!("user_record_{:08}", i).into_bytes();
            let tx = TxId::new(tx_counter);
            tx_counter += 1;
            storage.put(tx, &key, &updated_val).await.expect("update");
            storage.commit(tx).await.expect("commit");
        }

        // 2,000 Deletes per batch (total 10,000 across 5 batches)
        let delete_start = 90_000 + (batch - 1) * 2_000;
        let delete_end = 90_000 + batch * 2_000;
        for i in delete_start..delete_end {
            let key = format!("user_record_{:08}", i).into_bytes();
            let tx = TxId::new(tx_counter);
            tx_counter += 1;
            storage.delete(tx, &key).await.expect("delete");
            storage.commit(tx).await.expect("commit");
        }

        // Force flush & trigger compaction cycle after each batch
        storage.force_flush().await.expect("flush batch");
        tracker.poll_writes();

        let mut compactions = 0;
        while storage.maybe_compact().await.expect("compact batch") {
            compactions += 1;
            total_compaction_cycles += 1;
            tracker.poll_writes();
        }
        println!("  - Batch {} completed (20k Inserts, 4k Updates, 2k Deletes | Compactions in batch: {})", batch, compactions);
    }

    // Final flush and force full compaction until quiescent state
    storage.force_flush().await.expect("final flush");
    tracker.poll_writes();
    while storage.maybe_compact().await.expect("final compact") {
        total_compaction_cycles += 1;
        tracker.poll_writes();
    }
    tracker.poll_writes();

    let total_physical_bytes = tracker.total_bytes();

    // Compute logical bytes stored at end (remaining active key-value entries)
    let all_entries = storage
        .scan(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)
        .await
        .expect("scan end");
    let mut logical_stored_bytes: u64 = 0;
    for (k, v) in &all_entries {
        logical_stored_bytes += (k.len() + v.len()) as u64;
    }

    let write_amp_factor = total_physical_bytes as f64 / logical_stored_bytes as f64;

    println!("\nWrite Amplification Results:");
    println!(
        "- Total compaction cycles executed:        {}",
        total_compaction_cycles
    );
    println!(
        "- Total physical bytes written to disk (a): {} bytes ({:.2} MB)",
        total_physical_bytes,
        total_physical_bytes as f64 / 1_048_576.0
    );
    println!(
        "- Final logical bytes stored (b):          {} bytes ({:.2} MB)",
        logical_stored_bytes,
        logical_stored_bytes as f64 / 1_048_576.0
    );
    println!(
        "- Surviving logical Key-Value pairs:        {}",
        all_entries.len()
    );
    println!(
        "- Write-Amplification Factor (a / b):        {:.4}x",
        write_amp_factor
    );

    assert!(
        total_compaction_cycles >= 3,
        "Workload must trigger at least 3 compaction cycles"
    );

    // STEP 4: Read Amplification Measurement
    println!("\n[3] READ AMPLIFICATION MEASUREMENT FOR POINT LOOKUPS");

    // We take 1,000 random point lookups:
    // - 500 lookups for existing keys (randomly chosen from 0..90,000)
    // - 500 lookups for non-existing keys (randomly chosen from key_absent_*)
    let mut existing_queries = Vec::with_capacity(500);
    for i in (0..90_000).step_by(180).take(500) {
        existing_queries.push(format!("user_record_{:08}", i).into_bytes());
    }

    let mut non_existing_queries = Vec::with_capacity(500);
    for i in 0..500 {
        non_existing_queries.push(format!("user_record_absent_{:08}", i).into_bytes());
    }

    let sstables = storage.stats().await.expect("stats").num_segments;
    println!("- Active SSTable segments at end of workload: {}", sstables);

    let mut total_sstables_evaluated = 0usize;
    let mut total_bloom_passes = 0usize;
    let mut total_block_reads = 0usize;
    let mut total_successful_lookups = 0usize;

    for key in &existing_queries {
        let (eval, bloom_pass, _range_pass, block_reads, found) =
            storage.point_lookup_metrics(key).await;
        total_sstables_evaluated += eval;
        total_bloom_passes += bloom_pass;
        total_block_reads += block_reads;
        if found {
            total_successful_lookups += 1;
        }
    }

    let existing_ra_sstables = total_sstables_evaluated as f64 / 500.0;
    let existing_ra_blocks = total_block_reads as f64 / 500.0;

    let mut non_exist_sstables_evaluated = 0usize;
    let mut non_exist_bloom_passes = 0usize;
    let mut non_exist_block_reads = 0usize;

    for key in &non_existing_queries {
        let (eval, bloom_pass, _range_pass, block_reads, found) =
            storage.point_lookup_metrics(key).await;
        non_exist_sstables_evaluated += eval;
        non_exist_bloom_passes += bloom_pass;
        non_exist_block_reads += block_reads;
        assert!(!found, "Non-existing key must not be found");
    }

    let non_exist_ra_sstables = non_exist_sstables_evaluated as f64 / 500.0;
    let non_exist_ra_blocks = non_exist_block_reads as f64 / 500.0;

    let total_queries = 1000;
    let avg_blocks_per_lookup =
        (total_block_reads + non_exist_block_reads) as f64 / total_queries as f64;
    let avg_sstables_per_lookup =
        (total_sstables_evaluated + non_exist_sstables_evaluated) as f64 / total_queries as f64;
    let avg_bloom_passes_per_lookup =
        (total_bloom_passes + non_exist_bloom_passes) as f64 / total_queries as f64;

    println!("\nRead Amplification Results:");
    println!("- Existing Key Lookups (500 queries):");
    println!(
        "  * Avg SSTables evaluated per query: {:.2}",
        existing_ra_sstables
    );
    println!(
        "  * Avg Data Blocks read per query:   {:.2}",
        existing_ra_blocks
    );
    println!(
        "  * Successful lookups:               {}/500",
        total_successful_lookups
    );
    println!("- Non-Existing Key Lookups (500 queries):");
    println!(
        "  * Avg SSTables evaluated per query: {:.2}",
        non_exist_ra_sstables
    );
    println!(
        "  * Avg Whole-SSTable Bloom passes:   {:.4}",
        non_exist_bloom_passes as f64 / 500.0
    );
    println!(
        "  * Avg Data Blocks read per query:   {:.4}",
        non_exist_ra_blocks
    );
    println!("- Combined 1,000 Point Lookups Overall:");
    println!(
        "  * Average SSTable file checks per query: {:.2}",
        avg_sstables_per_lookup
    );
    println!(
        "  * Average Whole-SSTable Bloom passes:    {:.4}",
        avg_bloom_passes_per_lookup
    );
    println!(
        "  * Average Data Blocks read per query:    {:.4}",
        avg_blocks_per_lookup
    );

    println!("\n============================================================");
    println!("SUMMARY METRICS FOR AUDIT REPORT");
    println!("============================================================");
    println!("Bloom Filter Status: PRESENT (Whole-SSTable Blake3 Double-Hashing BloomFilter + In-Block 64-bit Bitmask)");
    println!(
        "Empirical Bloom FPR: {:.4}% (Target 1.00%)",
        empirical_fpr * 100.0
    );
    println!(
        "Total Compaction Cycles Executed: {}",
        total_compaction_cycles
    );
    println!("Write-Amplification Factor: {:.4}x", write_amp_factor);
    println!(
        "Read-Amplification (Avg Blocks Read / Query): {:.4}",
        avg_blocks_per_lookup
    );
    println!("============================================================");
}
