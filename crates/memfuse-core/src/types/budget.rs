//! Memory Budget Management for MemFuse.
// ANCHOR:ARCH:BUDGET-001 — Memory budget tracking.
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: MemoryBudget (memfuse-core/traits.rs)
// INVARIANTE: Alle großen Allokationen müssen über den ResourceTracker laufen.

use crate::{MemFuseError, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Configuration for memory budgets.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    pub memory_limit: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            memory_limit: 1024 * 1024 * 1024, // 1GB
        }
    }
}

/// Tracks memory usage and enforces budgets.
#[derive(Debug, Clone)]
pub struct ResourceTracker {
    limit: u64,
    used: Arc<AtomicU64>,
}

impl ResourceTracker {
    /// Creates a new ResourceTracker with the given budget.
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            limit: budget.memory_limit,
            used: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Attempts to consume memory. Returns error if budget exceeded.
    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        let mut current = self.used.load(Ordering::SeqCst);
        loop {
            let next = current + bytes;
            if next > self.limit {
                return Err(MemFuseError::MemoryBudgetExceeded {
                    used_mb: current / (1024 * 1024),
                    limit_mb: self.limit / (1024 * 1024),
                });
            }

            match self
                .used
                .compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return Ok(()),
                Err(updated) => current = updated,
            }
        }
    }

    /// Releases consumed memory.
    pub fn release_memory(&self, bytes: u64) {
        self.used.fetch_sub(bytes, Ordering::SeqCst);
    }

    /// Returns currently used memory in bytes.
    pub fn memory_used(&self) -> u64 {
        self.used.load(Ordering::SeqCst)
    }

    /// Returns memory limit in bytes.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Checks if there is capacity for more memory.
    pub fn has_memory_capacity(&self) -> bool {
        self.memory_used() < self.limit
    }

    /// Applies backpressure if needed (no-op for now).
    pub async fn apply_backpressure(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_budget_enforcement() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);
        assert!(tracker.consume_memory(500).is_ok());
        assert_eq!(tracker.memory_used(), 500);

        assert!(tracker.consume_memory(600).is_err());
        assert_eq!(tracker.memory_used(), 500);

        tracker.release_memory(200);
        assert_eq!(tracker.memory_used(), 300);
        assert!(tracker.consume_memory(600).is_ok());
    }

    #[test]
    fn test_budget_error_details() {
        // limit = 1024 bytes (0 MB)
        let budget = ResourceBudget { memory_limit: 1024 };
        let tracker = ResourceTracker::new(budget);
        tracker.consume_memory(900).expect("should consume");
        let result = tracker.consume_memory(200);

        assert!(result.is_err());
        if let Err(MemFuseError::MemoryBudgetExceeded { limit_mb, .. }) = result {
            assert_eq!(limit_mb, 0);
        } else {
            panic!("Expected MemoryBudgetExceeded error");
        }
    }

    #[test]
    fn test_budget_tracker_thread_safe() {
        let budget = ResourceBudget {
            memory_limit: 10000,
        };
        let tracker = ResourceTracker::new(budget);
        let tracker = Arc::new(tracker);
        let mut handlers = vec![];

        for _ in 0..10 {
            let t = tracker.clone();
            handlers.push(thread::spawn(move || {
                for _ in 0..100 {
                    t.consume_memory(10).expect("should succeed");
                }
            }));
        }

        for h in handlers {
            h.join().unwrap(); // unwrap
        }

        assert_eq!(tracker.memory_used(), 10000);
    }
}
