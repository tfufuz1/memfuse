//! Module for budget and resource tracking.
use crate::MemFuseError;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    pub memory_limit: u64,
}

pub struct ResourceTracker {
    budget: ResourceBudget,
    memory_used: std::sync::atomic::AtomicU64,
}

impl ResourceTracker {
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            budget,
            memory_used: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn consume_memory(&self, bytes: u64) -> Result<(), MemFuseError> {
        let current = self.memory_used.load(Ordering::SeqCst);
        if current + bytes > self.budget.memory_limit {
            return Err(MemFuseError::MemoryBudgetExceeded {
                limit_mb: self.budget.memory_limit / (1024 * 1024),
                used_mb: (current + bytes) / (1024 * 1024),
            });
        }
        self.memory_used.fetch_add(bytes, Ordering::SeqCst);
        Ok(())
    }

    pub fn release_memory(&self, bytes: u64) {
        self.memory_used.fetch_sub(bytes, Ordering::SeqCst);
    }

    pub fn memory_used(&self) -> u64 {
        self.memory_used.load(Ordering::SeqCst)
    }

    pub fn has_memory_capacity(&self) -> bool {
        let current = self.memory_used.load(Ordering::SeqCst);
        current < (self.budget.memory_limit * 95 / 100)
    }

    pub async fn apply_backpressure(&self) {
        while !self.has_memory_capacity() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracker_basic() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        assert_eq!(tracker.memory_used(), 0);
        assert!(tracker.has_memory_capacity());

        tracker.consume_memory(500).expect("should consume"); // unwrap allowed
        assert_eq!(tracker.memory_used(), 500);
        assert!(tracker.has_memory_capacity()); // 50% < 95%

        tracker.release_memory(200);
        assert_eq!(tracker.memory_used(), 300);
    }

    #[test]
    fn test_budget_exceeded() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(900).expect("should consume"); // unwrap allowed
        let result = tracker.consume_memory(200);

        assert!(result.is_err());
        let err = result.err().unwrap(); // unwrap allowed
        match err {
            MemFuseError::MemoryBudgetExceeded { limit_mb, .. } => {
                // used_mb = (900 + 200) / 1024*1024 = 0 in this case because limit is tiny
                assert_eq!(limit_mb, 0);
            }
            _ => panic!("Expected MemoryBudgetExceeded error"),
        }
    }

    #[test]
    fn test_memory_capacity_threshold() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(949).expect("ok"); // unwrap allowed
        assert!(tracker.has_memory_capacity());

        tracker.consume_memory(1).expect("ok"); // unwrap allowed
        assert!(!tracker.has_memory_capacity()); // 950/1000 = 95% -> not below 95%
    }

    #[test]
    fn test_concurrent_consumption() {
        let budget = ResourceBudget {
            memory_limit: 10000,
        };
        let tracker = std::sync::Arc::new(ResourceTracker::new(budget));

        let mut handlers = Vec::new();
        for _ in 0..10 {
            let t = tracker.clone();
            handlers.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    t.consume_memory(10).expect("consume"); // unwrap allowed
                }
            }));
        }

        for h in handlers {
            h.join().unwrap(); // unwrap allowed
        }

        assert_eq!(tracker.memory_used(), 10000);
        assert!(tracker.consume_memory(1).is_err());
    }
}
