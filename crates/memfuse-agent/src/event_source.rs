//! Continuous event source abstractions for background telemetry and triggers.
//!
//! Provides `EventSource` trait and concrete implementations (`PollingDocumentEventSource`, `VecEventSource`).

use async_trait::async_trait;
use memfuse_core::{Result, StorageEngine};
use memfuse_db::Collection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

/// Background telemetry/trigger event delivered to the agent context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundEvent {
    pub payload: serde_json::Value,
    pub source: String,
    pub observed_at_seq: u64,
}

impl BackgroundEvent {
    pub fn new(payload: serde_json::Value, source: impl Into<String>, observed_at_seq: u64) -> Self {
        Self {
            payload,
            source: source.into(),
            observed_at_seq,
        }
    }
}

/// Dynamic event source delivering continuous background events.
#[async_trait]
pub trait EventSource: Send + Sync {
    /// Fetches the next available background event.
    /// Returns `Ok(Some(event))` when an event is ready, `Ok(None)` if no event is currently pending.
    async fn next_event(&mut self) -> Result<Option<BackgroundEvent>>;

    /// Indicates whether the event source is permanently exhausted.
    /// Always returns `false` for continuous/always-on producers.
    fn is_exhausted(&self) -> bool {
        false
    }
}

/// Concrete `EventSource` that periodically polls `Collection` storage sequence numbers,
/// using `scan_prefix_at` for snapshot delta calculation to emit document changes.
pub struct PollingDocumentEventSource<S: StorageEngine> {
    collection: Arc<Collection<S>>,
    last_seen_seq: u64,
    poll_interval: Duration,
    pending_events: VecDeque<BackgroundEvent>,
}

impl<S: StorageEngine> PollingDocumentEventSource<S> {
    pub fn new(collection: Arc<Collection<S>>, poll_interval: Duration) -> Self {
        Self {
            collection,
            last_seen_seq: 0,
            poll_interval,
            pending_events: VecDeque::new(),
        }
    }

    pub fn with_last_seen_seq(mut self, last_seen_seq: u64) -> Self {
        self.last_seen_seq = last_seen_seq;
        self
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

#[async_trait]
impl<S: StorageEngine> EventSource for PollingDocumentEventSource<S> {
    async fn next_event(&mut self) -> Result<Option<BackgroundEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }

        let current_seq = self.collection.storage().last_seq_no().await?;

        if current_seq > self.last_seen_seq {
            let prefix = self.collection.user_key_prefix();
            let current_entries = self.collection.storage().scan_prefix_at(&prefix, current_seq).await?;

            let previous_entries: HashMap<Vec<u8>, Vec<u8>> = if self.last_seen_seq > 0 {
                self.collection
                    .storage()
                    .scan_prefix_at(&prefix, self.last_seen_seq)
                    .await?
                    .into_iter()
                    .collect()
            } else {
                HashMap::new()
            };

            for (key, val) in current_entries {
                if previous_entries.get(&key) != Some(&val) {
                    let payload = serde_json::from_slice(&val).unwrap_or_else(|_| {
                        serde_json::json!({
                            "raw": String::from_utf8_lossy(&val),
                            "key": String::from_utf8_lossy(&key)
                        })
                    });
                    self.pending_events.push_back(BackgroundEvent {
                        payload,
                        source: format!("collection:{}", self.collection.name()),
                        observed_at_seq: current_seq,
                    });
                }
            }

            self.last_seen_seq = current_seq;

            if let Some(event) = self.pending_events.pop_front() {
                return Ok(Some(event));
            }
        }

        Ok(None)
    }
}

/// Trivial mock/static `EventSource` that reads from a fixed list of `BackgroundEvent`s.
pub struct VecEventSource {
    events: VecDeque<BackgroundEvent>,
}

impl VecEventSource {
    pub fn new(events: Vec<BackgroundEvent>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

#[async_trait]
impl EventSource for VecEventSource {
    async fn next_event(&mut self) -> Result<Option<BackgroundEvent>> {
        Ok(self.events.pop_front())
    }

    fn is_exhausted(&self) -> bool {
        self.events.is_empty()
    }
}
