use crate::ingestion::pipeline::IngestReport;
use std::time::{Duration, Instant};

/// Konfiguration für das Throttling/Batching von Ingestion-Fortschrittsevents.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct IngestProgressConfig {
    /// Mindestabstand zwischen emittierten Progress-Events in Millisekunden (Default: 100 ms).
    pub min_interval_ms: u64,
    /// Maximale Anzahl verarbeiteter Dateien vor erzwungener Event-Emission (Default: 50).
    pub max_batch_size: usize,
}

impl Default for IngestProgressConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: 100,
            max_batch_size: 50,
        }
    }
}

/// Gebatchtes Fortschrittsevent zur Emission an das Webview-Frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct IngestProgressBatch {
    /// Anzahl in diesem Batch verarbeiteter Dateien.
    pub batch_files_processed: usize,
    /// Summe erstellter Chunks in diesem Batch.
    pub batch_chunks_created: usize,
    /// Fehler, die in diesem Batch aufgetreten sind.
    pub batch_errors: Vec<String>,
    /// Kumulative Anzahl verarbeiteter Dateien insgesamt.
    pub total_files_processed: usize,
    /// Kumulative Summe erstellter Chunks insgesamt.
    pub total_chunks_created: usize,
    /// Kennzeichnet, ob dies das finale Event der Ingestion ist.
    pub is_final: bool,
    /// Pfad der zuletzt verarbeiteten Datei in diesem Batch.
    pub last_file_path: Option<String>,
}

/// Trait zur Abstraktion der Event-Emission (z.B. für `tauri::AppHandle` oder Test-Mocks).
pub trait ProgressEmitter: Send + Sync {
    fn emit_progress(&self, batch: &IngestProgressBatch);
}

impl ProgressEmitter for tauri::AppHandle {
    fn emit_progress(&self, batch: &IngestProgressBatch) {
        use tauri::Emitter;
        let _ = self.emit("ingest-progress", batch);
    }
}

impl<F> ProgressEmitter for F
where
    F: Fn(&IngestProgressBatch) + Send + Sync,
{
    fn emit_progress(&self, batch: &IngestProgressBatch) {
        self(batch);
    }
}

/// Akkumulator und Drosselungslogik für Ingestion-Fortschrittsberichte.
pub struct IngestProgressThrottler<'a, E: ProgressEmitter> {
    emitter: &'a E,
    config: IngestProgressConfig,
    batch_files_processed: usize,
    batch_chunks_created: usize,
    batch_errors: Vec<String>,
    total_files_processed: usize,
    total_chunks_created: usize,
    last_file_path: Option<String>,
    last_emit_time: Instant,
}

impl<'a, E: ProgressEmitter> IngestProgressThrottler<'a, E> {
    pub fn new(emitter: &'a E, config: IngestProgressConfig) -> Self {
        Self {
            emitter,
            config,
            batch_files_processed: 0,
            batch_chunks_created: 0,
            batch_errors: Vec::new(),
            total_files_processed: 0,
            total_chunks_created: 0,
            last_file_path: None,
            last_emit_time: Instant::now(),
        }
    }

    /// Fügt den Ergebnisbericht einer verarbeiteten Datei hinzu und prüft,
    /// ob ein gebatchtes Event emittiert werden muss.
    pub fn add_report(&mut self, report: &IngestReport) {
        self.batch_files_processed += 1;
        self.batch_chunks_created += report.chunks_created;
        self.batch_errors.extend(report.errors.clone());
        self.total_files_processed += 1;
        self.total_chunks_created += report.chunks_created;
        self.last_file_path = Some(report.file_path.clone());

        let min_interval = Duration::from_millis(self.config.min_interval_ms);
        let time_elapsed = self.last_emit_time.elapsed() >= min_interval;
        let batch_full = self.batch_files_processed >= self.config.max_batch_size;

        if batch_full || time_elapsed {
            self.flush_batch(false);
        }
    }

    /// Schließt die Ingestion ab und garantiert ein finales Event (`is_final = true`).
    pub fn finish(mut self) {
        self.flush_batch(true);
    }

    fn flush_batch(&mut self, is_final: bool) {
        // Zwischen-Events nur emittieren, wenn tatsächlich neue Dateien im Batch verarbeitet wurden.
        // Finale Events immer emittieren, um den Abschluss zu signalisieren.
        if self.batch_files_processed == 0 && !is_final {
            return;
        }

        let batch = IngestProgressBatch {
            batch_files_processed: self.batch_files_processed,
            batch_chunks_created: self.batch_chunks_created,
            batch_errors: std::mem::take(&mut self.batch_errors),
            total_files_processed: self.total_files_processed,
            total_chunks_created: self.total_chunks_created,
            is_final,
            last_file_path: self.last_file_path.clone(),
        };

        self.emitter.emit_progress(&batch);
        self.batch_files_processed = 0;
        self.batch_chunks_created = 0;
        self.last_emit_time = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_throttler_batch_size_trigger() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let emitter = move |batch: &IngestProgressBatch| {
            events_clone.lock().expect("lock").push(batch.clone());
        };

        let config = IngestProgressConfig {
            min_interval_ms: 10_000, // Very high interval so only batch size triggers emission
            max_batch_size: 10,
        };

        let mut throttler = IngestProgressThrottler::new(&emitter, config);

        for i in 1..=25 {
            throttler.add_report(&IngestReport {
                file_path: format!("/path/file_{i}.txt"),
                chunks_created: 2,
                errors: vec![],
            });
        }

        {
            let emitted = events.lock().expect("lock");
            assert_eq!(emitted.len(), 2, "Should have emitted 2 batches of 10 items");
            assert_eq!(emitted[0].batch_files_processed, 10);
            assert_eq!(emitted[0].total_files_processed, 10);
            assert_eq!(emitted[0].batch_chunks_created, 20);
            assert!(!emitted[0].is_final);

            assert_eq!(emitted[1].batch_files_processed, 10);
            assert_eq!(emitted[1].total_files_processed, 20);
            assert_eq!(emitted[1].batch_chunks_created, 20);
            assert!(!emitted[1].is_final);
        }

        throttler.finish();

        {
            let emitted = events.lock().expect("lock");
            assert_eq!(emitted.len(), 3, "Final batch should emit remaining 5 items");
            assert_eq!(emitted[2].batch_files_processed, 5);
            assert_eq!(emitted[2].total_files_processed, 25);
            assert_eq!(emitted[2].batch_chunks_created, 10);
            assert!(emitted[2].is_final);
        }
    }

    #[test]
    fn test_throttler_always_emits_final_event() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let emitter = move |batch: &IngestProgressBatch| {
            events_clone.lock().expect("lock").push(batch.clone());
        };

        let config = IngestProgressConfig {
            min_interval_ms: 10_000,
            max_batch_size: 5,
        };

        let mut throttler = IngestProgressThrottler::new(&emitter, config);

        // Process exactly 10 files (matching batch size 5 twice)
        for i in 1..=10 {
            throttler.add_report(&IngestReport {
                file_path: format!("/path/file_{i}.txt"),
                chunks_created: 1,
                errors: vec![],
            });
        }

        // Finish should still emit a final event with is_final = true
        throttler.finish();

        let emitted = events.lock().expect("lock");
        assert_eq!(emitted.len(), 3);
        assert!(!emitted[0].is_final);
        assert!(!emitted[1].is_final);
        assert!(emitted[2].is_final);
        assert_eq!(emitted[2].total_files_processed, 10);
        assert_eq!(emitted[2].batch_files_processed, 0);
    }

    #[test]
    fn test_throttler_aggregates_errors() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let emitter = move |batch: &IngestProgressBatch| {
            events_clone.lock().expect("lock").push(batch.clone());
        };

        let config = IngestProgressConfig {
            min_interval_ms: 10_000,
            max_batch_size: 2,
        };

        let mut throttler = IngestProgressThrottler::new(&emitter, config);

        throttler.add_report(&IngestReport {
            file_path: "file1.txt".into(),
            chunks_created: 1,
            errors: vec!["Err 1".into()],
        });
        throttler.add_report(&IngestReport {
            file_path: "file2.txt".into(),
            chunks_created: 0,
            errors: vec!["Err 2a".into(), "Err 2b".into()],
        });

        throttler.finish();

        let emitted = events.lock().expect("lock");
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].batch_errors, vec!["Err 1", "Err 2a", "Err 2b"]);
    }
}
