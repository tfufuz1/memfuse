use memfuse_tauri_lib::ingestion::{
    IngestProgressBatch, IngestProgressConfig, IngestProgressThrottler, IngestReport,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn test_large_folder_ingestion_throttling_20k_files() {
    let emitted_events = Arc::new(Mutex::new(Vec::new()));
    let emitted_clone = Arc::clone(&emitted_events);

    let emitter = move |batch: &IngestProgressBatch| {
        emitted_clone
            .lock()
            .expect("lock vector")
            .push(batch.clone());
    };

    let config = IngestProgressConfig {
        min_interval_ms: 100,
        max_batch_size: 50,
    };

    let mut throttler = IngestProgressThrottler::new(&emitter, config);

    const FILE_COUNT: usize = 20_000;
    const CHUNKS_PER_FILE: usize = 3;

    for i in 1..=FILE_COUNT {
        throttler.add_report(&IngestReport {
            file_path: format!("/mock/path/file_{i}.txt"),
            chunks_created: CHUNKS_PER_FILE,
            errors: vec![],
            skipped_as_duplicate: false,
        });
    }

    throttler.finish();

    let events = emitted_events.lock().expect("lock vector");

    // With 20,000 files and batch size 50, 400 batch events + 1 final event are emitted
    assert!(
        events.len() < FILE_COUNT,
        "Emitted event count ({}) must be significantly below 20,000 file count!",
        events.len()
    );

    assert!(
        events.len() <= 401,
        "Expected at most 401 events for 20,000 files with batch size 50, got {}",
        events.len()
    );

    // Verify final event state
    let final_event = events
        .last()
        .expect("At least one progress event must be emitted");
    assert!(
        final_event.is_final,
        "The final emitted progress event must have is_final = true"
    );
    assert_eq!(
        final_event.total_files_processed, FILE_COUNT,
        "Final progress event must report total 20,000 files processed"
    );
    assert_eq!(
        final_event.total_chunks_created,
        FILE_COUNT * CHUNKS_PER_FILE,
        "Final progress event must report total created chunks"
    );
}

#[test]
fn test_small_folder_ingestion_timely_feedback() {
    let emitted_events = Arc::new(Mutex::new(Vec::new()));
    let emitted_clone = Arc::clone(&emitted_events);

    let emitter = move |batch: &IngestProgressBatch| {
        emitted_clone
            .lock()
            .expect("lock vector")
            .push(batch.clone());
    };

    let config = IngestProgressConfig {
        min_interval_ms: 100,
        max_batch_size: 50,
    };

    let mut throttler = IngestProgressThrottler::new(&emitter, config);

    const FILE_COUNT: usize = 5;

    for i in 1..=FILE_COUNT {
        throttler.add_report(&IngestReport {
            file_path: format!("/small/folder/doc_{i}.md"),
            chunks_created: 2,
            errors: vec![],
            skipped_as_duplicate: false,
        });
    }

    throttler.finish();

    let events = emitted_events.lock().expect("lock vector");

    assert!(
        !events.is_empty(),
        "Small folder ingestion must emit progress feedback"
    );

    let final_event = events.last().expect("Final event must be present");
    assert!(final_event.is_final, "Final event must indicate completion");
    assert_eq!(
        final_event.total_files_processed, FILE_COUNT,
        "Final event must accurately reflect all 5 files"
    );
    assert_eq!(
        final_event.total_chunks_created, 10,
        "Final event must accurately reflect total 10 chunks created"
    );
}

#[test]
fn test_configurable_interval_and_batch_size() {
    let emitted_events = Arc::new(Mutex::new(Vec::new()));
    let emitted_clone = Arc::clone(&emitted_events);

    let emitter = move |batch: &IngestProgressBatch| {
        emitted_clone
            .lock()
            .expect("lock vector")
            .push(batch.clone());
    };

    // Custom configuration: batch size 100
    let config = IngestProgressConfig {
        min_interval_ms: 1000,
        max_batch_size: 100,
    };

    let mut throttler = IngestProgressThrottler::new(&emitter, config);

    for i in 1..=250 {
        throttler.add_report(&IngestReport {
            file_path: format!("/config_test/item_{i}.txt"),
            chunks_created: 1,
            errors: vec![],
            skipped_as_duplicate: false,
        });
    }

    throttler.finish();

    let events = emitted_events.lock().expect("lock vector");

    // 250 items with batch size 100 => 2 batches of 100 + 1 final batch of 50 = 3 events
    assert_eq!(
        events.len(),
        3,
        "With batch_size=100 and 250 items, expected 3 events"
    );
    assert_eq!(events[0].batch_files_processed, 100);
    assert_eq!(events[1].batch_files_processed, 100);
    assert_eq!(events[2].batch_files_processed, 50);
    assert!(events[2].is_final);
    assert_eq!(events[2].total_files_processed, 250);
}

#[test]
fn test_throttling_time_interval_trigger() {
    let emitted_events = Arc::new(Mutex::new(Vec::new()));
    let emitted_clone = Arc::clone(&emitted_events);

    let emitter = move |batch: &IngestProgressBatch| {
        emitted_clone
            .lock()
            .expect("lock vector")
            .push(batch.clone());
    };

    // Min interval 10ms, large batch size 1000
    let config = IngestProgressConfig {
        min_interval_ms: 10,
        max_batch_size: 1000,
    };

    let mut throttler = IngestProgressThrottler::new(&emitter, config);

    throttler.add_report(&IngestReport {
        file_path: "/test/file1.txt".into(),
        chunks_created: 1,
        errors: vec![],
        skipped_as_duplicate: false,
    });

    // Sleep to exceed 10ms interval
    std::thread::sleep(Duration::from_millis(15));

    throttler.add_report(&IngestReport {
        file_path: "/test/file2.txt".into(),
        chunks_created: 1,
        errors: vec![],
        skipped_as_duplicate: false,
    });

    throttler.finish();

    let events = emitted_events.lock().expect("lock vector");
    // Should have emitted at least one batch triggered by time interval, plus final
    assert!(events.len() >= 2);
    assert!(events.last().unwrap().is_final);
}
