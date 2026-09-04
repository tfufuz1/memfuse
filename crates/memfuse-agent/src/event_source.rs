// FILE-CONTEXT Header (Format v3)
// ZWECK: Continuous event stream abstractions delivering telemetry/trigger events to agents.
// INVARIANTEN: Enforces MAX_EVENT_SOURCE_CAPACITY (10,000) on pending queue and event list; validates event source string.
// NICHT-OFFENSICHTLICH: PollingDocumentEventSource performs delta scanning via scan_prefix_at; drops over-capacity events gracefully.
// HOTSPOTS: BackgroundEvent::try_new (ll. 30-60), PollingDocumentEventSource::next_event (ll. 120-175).
// STAND: TS:2026-09-01T23:11:04Z (SESSION: 5a38054a)

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
    /// Constructs a `BackgroundEvent` with input validation on the source identifier.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Validates non-empty event source to prevent silent telemetry attribution loss. (TS:2026-08-30T15:00:19Z) (SESSION: 283abf0f)
    pub fn try_new(
        payload: serde_json::Value,
        source: impl Into<String>,
        observed_at_seq: u64,
    ) -> Result<Self> {
        let source_str = source.into();
        if source_str.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "BackgroundEvent source must not be empty".to_string(),
            ));
        }
        if source_str.len() > MAX_ID_LEN {
            return Err(memfuse_core::MemFuseError::InvalidInput(format!(
                "BackgroundEvent source length {} exceeds maximum allowed length of {}",
                source_str.len(),
                MAX_ID_LEN
            )));
        }
        if source_str.contains('\0') {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "BackgroundEvent source cannot contain null bytes".to_string(),
            ));
        }
        Ok(Self {
            payload,
            source: source_str,
            observed_at_seq,
        })
    }

    #[deprecated(note = "Use try_new instead to handle validation errors without panicking")]
    pub fn new(
        payload: serde_json::Value,
        source: impl Into<String>,
        observed_at_seq: u64,
    ) -> Self {
        Self::try_new(payload, source, observed_at_seq)
            .unwrap_or_else(|e| panic!("Failed to construct BackgroundEvent: {e}"))
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

    /// Waits until an event may be available or source is exhausted.
    /// Default: immediately ready (polling sources, backward-compatible).
    async fn wait_until_ready(&mut self) {
        // Default: no-op (source is always ready to poll)
    }
}

/// Maximum capacity for pending background telemetry events queue before dropping or rejecting.
pub const MAX_PENDING_EVENTS_CAPACITY: usize = 10_000;

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
        Self::with_capacity(collection, poll_interval, MAX_PENDING_EVENTS_CAPACITY)
    }

    /// Creates a new `PollingDocumentEventSource` with specified maximum queue capacity bound.
    // AI-TAG[HARDENING][CRITICAL] RESOLVED: Enforces bounded event queue capacity to guard against unbounded memory growth. (TS:2026-08-30T15:00:19Z) (SESSION: 283abf0f)
    pub fn with_capacity(
        collection: Arc<Collection<S>>,
        poll_interval: Duration,
        _max_pending_capacity: usize,
    ) -> Self {
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

    async fn wait_until_ready(&mut self) {
        if self.pending_events.is_empty() {
            tokio::time::sleep(self.poll_interval).await;
        }
    }
}

/// Trivial mock/static `EventSource` that reads from a fixed list of `BackgroundEvent`s.
pub struct VecEventSource {
    events: VecDeque<BackgroundEvent>,
}

impl VecEventSource {
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

struct EphemeralState {
    events: VecDeque<BackgroundEvent>,
    closed: bool,
}

/// Producer handle for pushing events asynchronously to an `EphemeralEventSource`.
#[derive(Clone)]
pub struct EphemeralProducer {
    state: Arc<std::sync::Mutex<EphemeralState>>,
    notify: Arc<tokio::sync::Notify>,
}

impl EphemeralProducer {
    pub fn push(&self, event: BackgroundEvent) {
        let mut lock = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if lock.events.len() < MAX_EVENT_SOURCE_CAPACITY {
            lock.events.push_back(event);
            drop(lock);
            self.notify.notify_one();
        }
    }

    pub fn close(&self) {
        let mut lock = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        lock.closed = true;
        drop(lock);
        self.notify.notify_one();
    }
}

/// Dynamic push-based event source yielding `None` when empty and waiting via backpressure notification.
pub struct EphemeralEventSource {
    state: Arc<std::sync::Mutex<EphemeralState>>,
    notify: Arc<tokio::sync::Notify>,
    poll_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl EphemeralEventSource {
    pub fn new() -> (Self, EphemeralProducer) {
        let poll_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let state = Arc::new(std::sync::Mutex::new(EphemeralState {
            events: VecDeque::new(),
            closed: false,
        }));
        let notify = Arc::new(tokio::sync::Notify::new());

        let source = Self {
            state: Arc::clone(&state),
            notify: Arc::clone(&notify),
            poll_count,
        };

        let producer = EphemeralProducer { state, notify };

        (source, producer)
    }

    pub fn poll_count(&self) -> usize {
        self.poll_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn poll_count_handle(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        Arc::clone(&self.poll_count)
    }
}

impl Default for EphemeralEventSource {
    fn default() -> Self {
        Self::new().0
    }
}

#[async_trait]
impl EventSource for EphemeralEventSource {
    async fn next_event(&mut self) -> Result<Option<BackgroundEvent>> {
        self.poll_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut lock = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        Ok(lock.events.pop_front())
    }

    fn is_exhausted(&self) -> bool {
        let lock = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        lock.closed && lock.events.is_empty()
    }

    async fn wait_until_ready(&mut self) {
        loop {
            let (is_empty, is_closed) = {
                let lock = match self.state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                (lock.events.is_empty(), lock.closed)
            };
            if !is_empty || is_closed {
                break;
            }
            self.notify.notified().await;
        }
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
            if let Ok(event) = BackgroundEvent::try_new(serde_json::json!({}), "source", i) {
                events.push(event);
            }
        }

        assert!(matches!(
            VecEventSource::try_new(events),
            Err(MemFuseError::MemoryBudgetExceeded { .. })
        ));
    }
}
