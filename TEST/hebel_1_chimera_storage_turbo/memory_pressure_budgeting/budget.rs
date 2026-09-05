//! SPEC-032: Resource Budget Enforcement
//!
//! Provides strict memory and CPU cycle tracking for indices and queries.
//! Enforcement is atomic and non-blocking using `std::sync::atomic`.
//!
//! Mission: implement strict memory and CPU budget tracking for all queries and indices.
//!
//! [DETERMINISM]: Lock-free atomic enforcement.
//! [SAFETY]: Zero-panic, no unwraps. Returns `ChimeraError::BudgetExceeded`.

use crate::error::{ChimeraError, Result};
use std::sync::atomic::{AtomicU64, Ordering};

/// Budget constraints for a specific operation or component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    /// Maximum memory allowed in bytes.
    pub memory_limit: u64,
    /// Maximum CPU cycles allowed.
    pub cpu_cycle_limit: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            memory_limit: 1024 * 1024 * 1024, // 1GB default
            cpu_cycle_limit: 1_000_000_000,   // 1G cycles default
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    Hnsw = 0,
    Spatial = 1,
    Metadata = 2,
    Sparse = 3,
    Storage = 4,
    Graph = 5,
    /// SPEC-041: Tensor engine memory (GGUF model weights + inference buffers).
    /// Default 0 — activated at startup when chimera-compute feature is enabled.
    Compute = 6,
}

impl Domain {
    pub const COUNT: usize = 7;

    pub fn all() -> [Self; Self::COUNT] {
        [
            Self::Hnsw,
            Self::Spatial,
            Self::Metadata,
            Self::Sparse,
            Self::Storage,
            Self::Graph,
            Self::Compute,
        ]
    }
}

/// Status of the resource budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Within normal operating bounds (< 80%).
    Normal,
    /// Approaching limits (80% - 95%), recommended to stall/throttle writes.
    Stall,
    /// At or near limits (> 95%), should reject new non-critical allocations.
    Reject,
}

/// Atomic tracker for resource consumption.
///
/// Guaranteed to never exceed the budget limits under concurrent access.
/// Uses lock-free CAS (Compare-And-Swap) for enforcement.
#[derive(Debug)]
pub struct ResourceTracker {
    budget: ResourceBudget,
    memory_used: AtomicU64,
    memory_peak: AtomicU64,
    cpu_cycles_used: AtomicU64,
    /// Domain-specific memory limits, dynamically rebalanced by AdaptiveAllocator.
    domain_limits: [AtomicU64; Domain::COUNT],
}

impl ResourceTracker {
    /// Creates a new tracker with the given budget.
    pub fn new(budget: ResourceBudget) -> Self {
        let domain_limits = [
            AtomicU64::new(budget.memory_limit / 5), // Hnsw (20%)
            AtomicU64::new(budget.memory_limit / 5), // Spatial (20%)
            AtomicU64::new(budget.memory_limit / 5), // Metadata (20%)
            AtomicU64::new(budget.memory_limit / 5), // Sparse (20%)
            AtomicU64::new(budget.memory_limit.saturating_mul(15) / 100), // Storage (15%)
            AtomicU64::new(budget.memory_limit.saturating_mul(5) / 100), // Graph (5%)
            // SPEC-041: Compute starts at 0 (no budget until chimera-compute activates).
            // chimera-compute::init() calls set_budget(Domain::Compute, model_size_bytes).
            AtomicU64::new(0), // Compute (0% default)
        ];

        Self {
            budget,
            memory_used: AtomicU64::new(0),
            memory_peak: AtomicU64::new(0),
            cpu_cycles_used: AtomicU64::new(0),
            domain_limits,
        }
    }

    /// Creates an unlimited tracker (u64::MAX limits).
    pub fn unlimited() -> Self {
        let mut tracker = Self::new(ResourceBudget {
            memory_limit: u64::MAX,
            cpu_cycle_limit: u64::MAX,
        });
        for limit in &mut tracker.domain_limits {
            limit.store(u64::MAX, Ordering::Release);
        }
        tracker
    }

    /// Sets the budget for a specific domain.
    pub fn set_budget(&self, domain: Domain, limit_bytes: u64) {
        self.domain_limits[domain as usize].store(limit_bytes, Ordering::Release);
    }

    /// Returns the total memory budget in bytes.
    pub fn total_budget_bytes(&self) -> u64 {
        self.budget.memory_limit
    }

    /// Returns the current limit for a specific domain.
    pub fn domain_limit(&self, domain: Domain) -> u64 {
        self.domain_limits[domain as usize].load(Ordering::Acquire)
    }

    /// Attempts to consume `bytes` of memory.
    ///
    /// # Errors
    /// Returns `Err(ChimeraError::BudgetExceeded)` if the limit would be reached.
    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }

        let mut current = self.memory_used.load(Ordering::Acquire);
        loop {
            let next = current
                .checked_add(bytes)
                .ok_or(ChimeraError::BudgetExceeded {
                    resource: "memory (overflow)",
                    used: current,
                    limit: self.budget.memory_limit,
                })?;

            if next > self.budget.memory_limit {
                return Err(ChimeraError::BudgetExceeded {
                    resource: "memory",
                    used: next,
                    limit: self.budget.memory_limit,
                });
            }

            match self.memory_used.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst, // Stronger ordering for safety
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Update peak memory usage
                    let mut peak = self.memory_peak.load(Ordering::Acquire);
                    while next > peak {
                        match self.memory_peak.compare_exchange_weak(
                            peak,
                            next,
                            Ordering::SeqCst,
                            Ordering::Acquire,
                        ) {
                            Ok(_) => break,
                            Err(actual) => peak = actual,
                        }
                    }
                    return Ok(());
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Releases `bytes` of memory.
    ///
    /// Guaranteed to not underflow the counter (stays at 0).
    pub fn release_memory(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }

        let mut current = self.memory_used.load(Ordering::Acquire);
        loop {
            let next = current.saturating_sub(bytes);
            match self.memory_used.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Attempts to consume `cycles` of compute.
    ///
    /// # Errors
    /// Returns `Err(ChimeraError::BudgetExceeded)` if the limit would be reached.
    pub fn consume_cpu(&self, cycles: u64) -> Result<()> {
        if cycles == 0 {
            return Ok(());
        }

        let mut current = self.cpu_cycles_used.load(Ordering::Acquire);
        loop {
            let next = current
                .checked_add(cycles)
                .ok_or(ChimeraError::BudgetExceeded {
                    resource: "cpu_cycles (overflow)",
                    used: current,
                    limit: self.budget.cpu_cycle_limit,
                })?;

            if next > self.budget.cpu_cycle_limit {
                return Err(ChimeraError::BudgetExceeded {
                    resource: "cpu_cycles",
                    used: next,
                    limit: self.budget.cpu_cycle_limit,
                });
            }

            match self.cpu_cycles_used.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }

    /// Returns the current budget status for memory.
    pub fn status(&self) -> BudgetStatus {
        let limit = self.budget.memory_limit;
        if limit == 0 {
            return BudgetStatus::Normal;
        }
        let current = self.memory_used.load(Ordering::Acquire);

        // Integer-based ratio calculation to avoid floats in hot paths
        // status() might be called frequently by query loops.
        // limit * 80 / 100
        if current > (limit.saturating_mul(95) / 100) {
            BudgetStatus::Reject
        } else if current > (limit.saturating_mul(80) / 100) {
            BudgetStatus::Stall
        } else {
            BudgetStatus::Normal
        }
    }

    /// Current memory usage in bytes.
    pub fn memory_used(&self) -> u64 {
        self.memory_used.load(Ordering::Acquire)
    }

    /// Peak memory usage in bytes since creation.
    pub fn memory_peak(&self) -> u64 {
        self.memory_peak.load(Ordering::Acquire)
    }

    /// Current CPU cycles consumed.
    pub fn cpu_cycles_used(&self) -> u64 {
        self.cpu_cycles_used.load(Ordering::Acquire)
    }

    /// Returns the budget limits.
    pub fn budget(&self) -> ResourceBudget {
        self.budget
    }

    /// Resets the tracker to zero usage.
    pub fn reset(&self) {
        self.memory_used.store(0, Ordering::Release);
        self.cpu_cycles_used.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_budget_enforcement() {
        let budget = ResourceBudget {
            memory_limit: 100,
            cpu_cycle_limit: 100,
        };
        let tracker = ResourceTracker::new(budget);

        assert!(tracker.consume_memory(50).is_ok());
        assert_eq!(tracker.memory_peak(), 50);
        assert!(tracker.consume_memory(51).is_err());
        assert_eq!(tracker.memory_used(), 50);

        tracker.release_memory(20);
        assert_eq!(tracker.memory_used(), 30);
        assert_eq!(tracker.memory_peak(), 50);

        assert!(tracker.consume_memory(50).is_ok());
        assert_eq!(tracker.memory_used(), 80);
        assert_eq!(tracker.memory_peak(), 80);
    }

    #[test]
    fn test_cpu_budget() {
        let budget = ResourceBudget {
            memory_limit: 1000,
            cpu_cycle_limit: 100,
        };
        let tracker = ResourceTracker::new(budget);

        assert!(tracker.consume_cpu(90).is_ok());
        assert!(tracker.consume_cpu(11).is_err());
        assert_eq!(tracker.cpu_cycles_used(), 90);
    }

    #[test]
    fn test_release_underflow_protection() {
        let tracker = ResourceTracker::unlimited();
        tracker
            .consume_memory(10)
            .expect("unlimited tracker should not fail");
        tracker.release_memory(15);
        assert_eq!(tracker.memory_used(), 0);
    }

    proptest! {
        #[test]
        fn prop_concurrent_memory_budget(
            thread_count in 2..8usize,
            allocs_per_thread in 10..50u64,
            max_bytes_per_alloc in 1..20u64,
        ) {
            let total_potential = thread_count as u64 * allocs_per_thread * max_bytes_per_alloc;
            let limit = total_potential / 2;

            let budget = ResourceBudget {
                memory_limit: limit,
                cpu_cycle_limit: u64::MAX,
            };
            let tracker = Arc::new(ResourceTracker::new(budget));
            let mut handles = vec![];

            for _ in 0..thread_count {
                let t = Arc::clone(&tracker);
                handles.push(thread::spawn(move || {
                    let mut success_count = 0;
                    let mut total_bytes = 0;
                    for i in 0..allocs_per_thread {
                        let bytes = (i % max_bytes_per_alloc) + 1;
                        if t.consume_memory(bytes).is_ok() {
                            success_count += 1;
                            total_bytes += bytes;
                        }
                    }
                    (success_count, total_bytes)
                }));
            }

            let results: Vec<_> = handles.into_iter().map(|h| h.join().expect("thread panic")).collect();
            let total_success_bytes: u64 = results.iter().map(|(_, b)| b).sum();
            let final_usage = tracker.memory_used();

            prop_assert!(final_usage <= limit);
            prop_assert_eq!(final_usage, total_success_bytes);
            prop_assert!(tracker.memory_peak() >= final_usage);
        }

        #[test]
        fn prop_concurrent_cpu_budget(
            thread_count in 2..8usize,
            ops_per_thread in 10..50u64,
            cycles_per_op in 1..1000u64,
        ) {
            let total_potential = thread_count as u64 * ops_per_thread * cycles_per_op;
            let limit = total_potential / 3;

            let budget = ResourceBudget {
                memory_limit: u64::MAX,
                cpu_cycle_limit: limit,
            };
            let tracker = Arc::new(ResourceTracker::new(budget));
            let mut handles = vec![];

            for _ in 0..thread_count {
                let t = Arc::clone(&tracker);
                handles.push(thread::spawn(move || {
                    let mut total_cycles = 0;
                    for _ in 0..ops_per_thread {
                        if t.consume_cpu(cycles_per_op).is_ok() {
                            total_cycles += cycles_per_op;
                        }
                    }
                    total_cycles
                }));
            }

            let total_success_cycles: u64 = handles.into_iter().map(|h| h.join().expect("thread panic")).sum();
            let final_usage = tracker.cpu_cycles_used();

            prop_assert!(final_usage <= limit);
            prop_assert_eq!(final_usage, total_success_cycles);
        }

        #[test]
        fn prop_concurrent_consume_release(
            thread_count in 2..4usize,
            ops_per_thread in 100..200u64,
        ) {
            let budget = ResourceBudget {
                memory_limit: 1000,
                cpu_cycle_limit: u64::MAX,
            };
            let tracker = Arc::new(ResourceTracker::new(budget));
            let mut handles = vec![];

            for _ in 0..thread_count {
                let t = Arc::clone(&tracker);
                handles.push(thread::spawn(move || {
                    let mut success_bytes = 0;
                    for i in 0..ops_per_thread {
                        let bytes = (i % 10) + 1;
                        if t.consume_memory(bytes).is_ok() {
                            success_bytes += bytes;
                            t.release_memory(bytes);
                            success_bytes -= bytes;
                        }
                    }
                    success_bytes
                }));
            }

            for h in handles {
                let res = h.join().expect("thread panic");
                prop_assert_eq!(res, 0);
            }

            prop_assert_eq!(tracker.memory_used(), 0);
        }
    }
}
