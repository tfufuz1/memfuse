use crate::error::{MemFuseError, Result};

/// Resource budget for memory management.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Maximum allowed memory usage in bytes.
    pub memory_limit: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            memory_limit: 2 * 1024 * 1024 * 1024, // 2GB
        }
    }
}

/// Tracks resource usage against a budget.
#[derive(Debug)]
pub struct ResourceTracker {
    /// The configured budget.
    budget: ResourceBudget,
    /// Current memory usage in bytes.
    memory_used: std::sync::atomic::AtomicU64,
}

impl Default for ResourceTracker {
    fn default() -> Self {
        Self::new(ResourceBudget::default())
    }
}

impl ResourceTracker {
    /// Creates a new ResourceTracker with the given budget.
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            budget,
            memory_used: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        loop {
            let current = self.memory_used.load(std::sync::atomic::Ordering::Acquire);
            if current + bytes > self.budget.memory_limit {
                return Err(MemFuseError::MemoryBudgetExceeded {
                    used_mb: (current + bytes) / (1024 * 1024),
                    limit_mb: self.budget.memory_limit / (1024 * 1024),
                });
            }
            if self
                .memory_used
                .compare_exchange(
                    current,
                    current + bytes,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    pub fn release_memory(&self, bytes: u64) {
        self.memory_used
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |v| Some(v.saturating_sub(bytes)),
            )
            .ok();
    }

    pub fn memory_used(&self) -> u64 {
        self.memory_used.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn budget(&self) -> &ResourceBudget {
        &self.budget
    }

    /// Returns true if memory usage is below 95% of the limit.
    pub fn has_memory_capacity(&self) -> bool {
        self.memory_used() < (self.budget.memory_limit as f64 * 0.95) as u64
    }

    /// Suspends execution briefly if memory usage exceeds 80% to apply backpressure.
    pub async fn apply_backpressure(&self) {
        if self.memory_used() >= (self.budget.memory_limit as f64 * 0.80) as u64 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
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
        tracker.consume_memory(500).expect("ok");
        assert_eq!(tracker.memory_used(), 500);
        tracker.release_memory(200);
        assert_eq!(tracker.memory_used(), 300);
    }

    #[test]
    fn test_budget_exceeded() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);
        tracker.consume_memory(900).expect("ok");
        let result = tracker.consume_memory(200);
        assert!(result.is_err());
        match result.err().unwrap() { // unwrap
            MemFuseError::MemoryBudgetExceeded { .. } => {}
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn test_concurrent_consumption() {
        let budget = ResourceBudget { memory_limit: 10000 };
        let tracker = std::sync::Arc::new(ResourceTracker::new(budget));
        let mut handlers = Vec::new();
        for _ in 0..10 {
            let t = tracker.clone();
            handlers.push(std::thread::spawn(move || {
                for _ in 0..100 { t.consume_memory(10).expect("ok"); }
            }));
        }
        for h in handlers { h.join().unwrap(); } // unwrap
        assert_eq!(tracker.memory_used(), 10000);
    }
}
