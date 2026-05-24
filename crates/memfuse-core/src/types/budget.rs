//! Resource budget tracking for memory and CPU.
//!
//! Implements backpressure and memory limit enforcement (WP-0.0).

use crate::error::{MemFuseError, Result};
use std::sync::atomic::{AtomicU64, Ordering};

/// Resource consumption limits.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Maximum allowed memory usage in bytes.
    pub memory_limit: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            memory_limit: 1024 * 1024 * 1024, // 1GB
        }
    }
}

/// Thread-safe tracker for resource consumption.
#[derive(Debug)]
pub struct ResourceTracker {
    budget: ResourceBudget,
    used_memory: AtomicU64,
}

impl ResourceTracker {
    /// Creates a new resource tracker with the given budget.
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            budget,
            used_memory: AtomicU64::new(0),
        }
    }

    /// Attempts to consume memory. Returns an error if the budget would be exceeded.
    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        let current = self.used_memory.load(Ordering::Relaxed);
        if current + bytes > self.budget.memory_limit {
            return Err(MemFuseError::MemoryBudgetExceeded {
                limit_mb: self.budget.memory_limit / (1024 * 1024),
                used_mb: (current + bytes) / (1024 * 1024),
            });
        }

        self.used_memory.fetch_add(bytes, Ordering::SeqCst);
        Ok(())
    }

    /// Releases consumed memory.
    pub fn release_memory(&self, bytes: u64) {
        self.used_memory
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                Some(current.saturating_sub(bytes))
            })
            .ok();
    }

    /// Returns the current memory usage in bytes.
    pub fn used_memory(&self) -> u64 {
        self.used_memory.load(Ordering::Acquire)
    }

    /// Checks if memory usage is within budget.
    pub fn is_within_budget(&self) -> bool {
        self.used_memory.load(Ordering::Acquire) <= self.budget.memory_limit
    }

    pub async fn apply_backpressure(&self) {
        let used = self.used_memory.load(Ordering::Acquire);
        if used > self.budget.memory_limit * 8 / 10 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub fn has_memory_capacity(&self) -> bool {
        self.used_memory.load(Ordering::Acquire) < self.budget.memory_limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracker_basic() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(500).expect("should consume");
        assert_eq!(tracker.used_memory(), 500);

        tracker.release_memory(200);
        assert_eq!(tracker.used_memory(), 300);
    }

    #[test]
    fn test_budget_exceeded() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(900).expect("should consume");
        let result = tracker.consume_memory(200);

        assert!(result.is_err());
        match result.err().unwrap() {
            // unwrap
            MemFuseError::MemoryBudgetExceeded { limit_mb, .. } => {
                assert_eq!(limit_mb, 0);
            }
            _ => panic!("Expected MemoryBudgetExceeded error"),
        }
    }

    #[test]
    fn test_capacity_threshold() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(1000).expect("limit is inclusive");
        assert!(tracker.is_within_budget());
    }

    #[test]
    fn test_concurrent_consumption() {
        let budget = ResourceBudget {
            memory_limit: 10000,
        };
        let tracker = std::sync::Arc::new(ResourceTracker::new(budget));
        let mut handles = vec![];

        for _ in 0..10 {
            let t = std::sync::Arc::clone(&tracker);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = t.consume_memory(1);
                }
            }));
        }

        for h in handles {
            h.join().unwrap(); // unwrap
        }

        assert!(tracker.used_memory() <= 1000);
    }

    #[test]
    fn test_backpressure_trigger() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(800).unwrap(); // unwrap
        assert!(tracker.is_within_budget());

        tracker.consume_memory(300).unwrap_err(); // unwrap
    }
}
