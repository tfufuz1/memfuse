//! Continuous event source abstractions for background telemetry and triggers.
//!
//! Provides `EventSource` trait and concrete implementations (`PollingDocumentEventSource`, `VecEventSource`).

use crate::context::MAX_ID_LEN;
use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, StorageEngine};
use memfuse_db::Collection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

/// Maximum allowed events in pending event buffers to prevent memory exhaustion.
pub const MAX_EVENT_SOURCE_CAPACITY: usize = 10_000;

/// Background telemetry/trigger event delivered to the agent context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundEvent {
    pub payload: serde_json::Value,
    pub source: String,
    pub observed_at_seq: u64,
}

impl BackgroundEvent {
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: AGT-AGENT-e0571b01 — Fallible BackgroundEvent::try_new enforces non-empty, length-bounded, null-free source strings. (TS: 2026-08-30T15:36:46Z) (SESSION: 761f1346)
    /// Attempts to construct a `BackgroundEvent` with boundary validation on `source`.
    pub fn try_new(
        payload: serde_json::Value,
        source: impl Into<String>,
        observed_at_seq: u64,
    ) -> Result<Self> {
        let source_str = source.into();
        if source_str.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Event source cannot be empty".to_string(),
            ));
        }
        if source_str.len() > MAX_ID_LEN {
            return Err(MemFuseError::InvalidInput(format!(
                "Event source length {} exceeds maximum allowed length of {}",
                source_str.len(),
                MAX_ID_LEN
            )));
        }
        if source_str.contains('\0') {
            return Err(MemFuseError::InvalidInput(
                "Event source cannot contain null bytes".to_string(),
            ));
        }
        Ok(Self {
            payload,
            source: source_str,
            observed_at_seq,
        })
    }

    /// Constructs a `BackgroundEvent`, panicking if `source` is invalid.
    pub fn new(
        payload: serde_json::Value,
        source: impl Into<String>,
        observed_at_seq: u64,
    ) -> Self {
        Self::try_new(payload, source, observed_at_seq)
            .expect("Invalid parameters in BackgroundEvent::new")
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
            let current_entries = self
                .collection
                .storage()
                .scan_prefix_at(&prefix, current_seq)
                .await?;

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
                    if self.pending_events.len() >= MAX_EVENT_SOURCE_CAPACITY {
                        tracing::warn!("PollingDocumentEventSource: Pending events queue capacity limit ({}) reached, dropping remaining events", MAX_EVENT_SOURCE_CAPACITY);
                        break;
                    }

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
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: AGT-AGENT-f4c28e99 — Fallible VecEventSource::try_new caps buffer capacity to MAX_EVENT_SOURCE_CAPACITY preventing OOM. (TS: 2026-08-30T15:36:46Z) (SESSION: 761f1346)
    /// Attempts to construct a `VecEventSource` with a capacity check.
    pub fn try_new(events: Vec<BackgroundEvent>) -> Result<Self> {
        if events.len() > MAX_EVENT_SOURCE_CAPACITY {
            return Err(MemFuseError::MemoryBudgetExceeded {
                used_mb: ((events.len() * std::mem::size_of::<BackgroundEvent>()) / (1024 * 1024))
                    as u64,
                limit_mb: MAX_EVENT_SOURCE_CAPACITY as u64,
            });
        }
        Ok(Self {
            events: events.into(),
        })
    }

    /// Constructs a `VecEventSource`, panicking if event count exceeds maximum capacity.
    pub fn new(events: Vec<BackgroundEvent>) -> Self {
        Self::try_new(events)
            .expect("Event count exceeds MAX_EVENT_SOURCE_CAPACITY in VecEventSource::new")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_event_validation() {
        assert!(BackgroundEvent::try_new(serde_json::json!({}), "valid_source", 1).is_ok());

        assert!(matches!(
            BackgroundEvent::try_new(serde_json::json!({}), "", 1),
            Err(MemFuseError::InvalidInput(_))
        ));

        assert!(matches!(
            BackgroundEvent::try_new(serde_json::json!({}), "source\0null", 1),
            Err(MemFuseError::InvalidInput(_))
        ));

        let huge_source = "s".repeat(300);
        assert!(matches!(
            BackgroundEvent::try_new(serde_json::json!({}), &huge_source, 1),
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_vec_event_source_capacity_limit() {
        let mut events = Vec::new();
        for i in 0..10_001 {
            events.push(BackgroundEvent::new(serde_json::json!({}), "source", i));
        }

        assert!(matches!(
            VecEventSource::try_new(events),
            Err(MemFuseError::MemoryBudgetExceeded { .. })
        ));
    }
}
