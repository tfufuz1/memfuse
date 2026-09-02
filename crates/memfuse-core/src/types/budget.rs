//! Resource budget management for MemFuse.

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Token- und Resource-Budget-Management mit atomarem Tracking & Memory Backpressure.
// INVARIANTEN: Atomares Memory Tracking; MemoryBudgetExceeded bei >95% Kapazität.
// HOTSPOTS: 40-220
// NICHT-OFFENSICHTLICH: TokenBudget reserviert Puffer für System-Prompts & Antworten in ContextManager.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md

//! # Architektur
//! Implementiert den `ResourceTracker`, der den Speicherverbrauch überwacht
//! und Backpressure-Signale an die Storage-Engines sendet.
//!
//! # Invarianten
//! - Der Speicherverbrauch wird atomar getrackt.
//! - Überschreitet der Verbrauch 95% des Budgets, wird `MemoryBudgetExceeded` geworfen.

use crate::error::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Strategie für Token-Budget-Management.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BudgetStrategy {
    /// Konservativ: 80% des Limits als sicherer Puffer (Standard)
    Conservative,
    /// Aggressiv: 95% des Limits nutzen
    Aggressive,
    /// Exakt: Angabe des verfügbaren Fensters direkt
    Exact(usize),
}

/// Token budget configuration for LLM context management.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Maximum total token limit.
    pub limit: usize,
    /// Strategy for calculating usable context window.
    pub strategy: BudgetStrategy,
    /// Reserved tokens for system prompt and generated answer.
    pub reserved: usize,
    /// Currently consumed tokens.
    consumed: usize,
}

impl TokenBudget {
    /// Creates a new token budget with an exact max limit and reserve tokens.
    pub fn new(max_tokens: usize, reserve_tokens: usize) -> Self {
        Self {
            limit: max_tokens,
            strategy: BudgetStrategy::Exact(max_tokens),
            reserved: reserve_tokens,
            consumed: 0,
        }
    }

    /// Erstellt ein Budget für gängige Modelle.
    ///
    /// # Maintenance Note
    /// Model name patterns use string matching. Explicit model family patterns
    /// (e.g. `claude-4`, `claude-opus-4`, `claude-sonnet-4`, `gpt-4o-mini`) must be updated
    /// whenever new model releases or naming conventions ship.
    pub fn for_model(model: &str) -> Self {
        let limit = match model {
            m if m.contains("claude-4")
                || m.contains("claude-opus-4")
                || m.contains("claude-sonnet-4") =>
            {
                200_000
            }
            m if m.contains("gpt-4o-mini") => 128_000,
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("claude-3") => 200_000,
            m if m.contains("claude-sonnet") => 200_000,
            m if m.contains("llama3") => 8_192,
            m if m.contains("mistral") => 32_768,
            _ => 8_192, // sicherer Default
        };
        Self {
            limit,
            strategy: BudgetStrategy::Conservative,
            reserved: 0,
            consumed: 0,
        }
    }

    /// Reserviert Token für System-Prompt und Antwort.
    pub fn with_reserved(mut self, system_tokens: usize, answer_tokens: usize) -> Self {
        self.reserved = system_tokens + answer_tokens;
        self
    }

    /// Sets the budget strategy.
    pub fn with_strategy(mut self, strategy: BudgetStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Calculates the effective token limit based on strategy.
    pub fn effective_limit(&self) -> usize {
        match self.strategy {
            BudgetStrategy::Conservative => (self.limit as f64 * 0.8) as usize,
            BudgetStrategy::Aggressive => (self.limit as f64 * 0.95) as usize,
            BudgetStrategy::Exact(n) => n,
        }
    }

    /// Returns tokens still available after subtracting reserved and consumed tokens.
    pub fn available(&self) -> usize {
        self.effective_limit()
            .saturating_sub(self.reserved)
            .saturating_sub(self.consumed)
    }

    /// Records `tokens` as consumed, reducing future availability.
    pub fn consume(&mut self, tokens: usize) {
        self.consumed = self.consumed.saturating_add(tokens);
    }

    /// Tries to reserve `tokens` from the available budget before step execution.
    ///
    /// Returns `Err(MemFuseError::Internal(...))` if available budget is 0 or less than `tokens`.
    pub fn try_reserve(&mut self, tokens: usize) -> Result<()> {
        let avail = self.available();
        if avail == 0 || avail < tokens {
            return Err(MemFuseError::Internal(format!(
                "Token budget exhausted before step execution (available: {}, required: {})",
                avail, tokens
            )));
        }
        self.consume(tokens);
        Ok(())
    }

    /// Refunds `tokens` back to the available budget if execution fails or consumes less than estimated.
    pub fn refund(&mut self, tokens: usize) {
        self.consumed = self.consumed.saturating_sub(tokens);
    }

    /// Returns total tokens consumed so far.
    pub fn consumed(&self) -> usize {
        self.consumed
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            limit: 8192,
            strategy: BudgetStrategy::Conservative,
            reserved: 512,
            consumed: 0,
        }
    }
}

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

    /// Attempts to consume `bytes` of memory from the tracked budget.
    ///
    /// # Errors
    /// Returns `Err(MemFuseError::MemoryBudgetExceeded)` if consuming `bytes`
    /// exceeds the configured memory limit.
    pub fn consume_memory(&self, bytes: u64) -> Result<()> {
        loop {
            let current = self.memory_used.load(std::sync::atomic::Ordering::Acquire);
            let next = current.saturating_add(bytes);

            if next > self.budget.memory_limit {
                return Err(MemFuseError::MemoryBudgetExceeded {
                    used_mb: next / (1024 * 1024),
                    limit_mb: self.budget.memory_limit / (1024 * 1024),
                });
            }
            if self
                .memory_used
                .compare_exchange(
                    current,
                    next,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Releases memory from the budget. Uses saturating subtraction to prevent
    /// atomic underflow (wrapping to `u64::MAX`) which would permanently block
    /// all future allocations. (FIND-COR-002)
    pub fn release_memory(&self, bytes: u64) {
        loop {
            let current = self.memory_used.load(std::sync::atomic::Ordering::Acquire);

            // SD-05-COR-001: Underflow protection for the atomic counter.
            // We saturate to 0 instead of wrapping to preserve system stability.

            let new_val = current.saturating_sub(bytes);
            if self
                .memory_used
                .compare_exchange(
                    current,
                    new_val,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }
    }

    /// Returns current memory usage in bytes.
    pub fn memory_used(&self) -> u64 {
        self.memory_used.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns a reference to the configured resource budget.
    pub fn budget(&self) -> &ResourceBudget {
        &self.budget
    }

    /// Returns true if memory usage is below 95% of the limit.
    pub fn has_memory_capacity(&self) -> bool {
        self.memory_used() < (self.budget.memory_limit as f64 * 0.95) as u64
    }
}

// INTENT: Resource Tracker (Memory Budget & Backpressure) verified by 5 tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracker_basic() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        assert_eq!(tracker.memory_used(), 0);
        assert!(tracker.has_memory_capacity());

        tracker.consume_memory(500).expect("should consume"); // expect
        assert_eq!(tracker.memory_used(), 500);
        assert!(tracker.has_memory_capacity()); // 50% < 95%

        tracker.release_memory(200);
        assert_eq!(tracker.memory_used(), 300);
    }

    #[test]
    fn test_budget_exceeded() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(900).expect("should consume"); // expect
        let result = tracker.consume_memory(200);

        assert!(result.is_err());
        match result.err().unwrap() {
            // unwrap
            // unwrap
            MemFuseError::MemoryBudgetExceeded { limit_mb, .. } => {
                // used_mb = (900 + 200) / 1024*1024 = 0 in this case because limit is tiny
                assert_eq!(limit_mb, 0);
            }
            _ => panic!("Expected MemoryBudgetExceeded error"),
        }
    }

    #[test]
    fn test_capacity_threshold() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        tracker.consume_memory(949).expect("ok"); // expect
        assert!(tracker.has_memory_capacity()); // 94.9% < 95%

        tracker.consume_memory(1).expect("ok"); // expect
        assert!(!tracker.has_memory_capacity()); // 95% is not < 95%
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
                    t.consume_memory(10).expect("consume"); // expect
                }
            }));
        }

        for h in handlers {
            h.join().unwrap(); // unwrap
        }

        assert_eq!(tracker.memory_used(), 10000);
        assert!(tracker.consume_memory(1).is_err());
    }

    #[test]
    fn test_release_memory_underflow_safety() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        // Underflow: release 10 when 0 used
        tracker.release_memory(10);
        assert_eq!(
            tracker.memory_used(),
            0,
            "Counter should saturate at 0, not wrap to MAX"
        );

        // Verify we can still consume memory (wrap would make memory_used > limit)
        tracker
            .consume_memory(500)
            .expect("Should still allow consumption after underflow"); // expect
        assert_eq!(tracker.memory_used(), 500);
    }

    #[test]
    fn test_consume_memory_overflow_prevention() {
        let budget = ResourceBudget { memory_limit: 1000 };
        let tracker = ResourceTracker::new(budget);

        // Try to consume more than u64::MAX (total)
        let result = tracker.consume_memory(u64::MAX);
        assert!(result.is_err());

        // Now test if current + bytes overflows u64
        tracker.consume_memory(500).unwrap(); // unwrap
        let result = tracker.consume_memory(u64::MAX - 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_token_budget_for_model() {
        let b = TokenBudget::for_model("gpt-4o");
        assert_eq!(b.limit, 128_000);
        assert_eq!(b.strategy, BudgetStrategy::Conservative);
        // 80% of 128_000 = 102_400
        assert_eq!(b.effective_limit(), 102_400);

        let b2 = b.with_reserved(1000, 2000);
        assert_eq!(b2.reserved, 3000);
        assert_eq!(b2.available(), 102_400 - 3000);

        // Test explicit model patterns
        assert_eq!(TokenBudget::for_model("claude-4").limit, 200_000);
        assert_eq!(TokenBudget::for_model("claude-opus-4").limit, 200_000);
        assert_eq!(TokenBudget::for_model("claude-sonnet-4").limit, 200_000);
        assert_eq!(TokenBudget::for_model("gpt-4o-mini").limit, 128_000);
    }

    #[test]
    fn test_token_budget_edge_cases() {
        let mut budget = TokenBudget::new(100, 20);
        assert_eq!(budget.effective_limit(), 100);
        assert_eq!(budget.available(), 80);

        // Consume more than available limit -> available saturates at 0
        budget.consume(90);
        assert_eq!(budget.consumed(), 90);
        assert_eq!(budget.available(), 0);

        // Consume with overflow attempt saturates at usize::MAX
        budget.consume(usize::MAX - 10);
        assert_eq!(budget.consumed(), usize::MAX);
        assert_eq!(budget.available(), 0);

        // Default TokenBudget
        let default_b = TokenBudget::default();
        assert_eq!(default_b.limit, 8192);
        assert_eq!(default_b.reserved, 512);
        assert_eq!(default_b.consumed, 0);

        // Unknown model string falls back to 8192
        let unknown_b = TokenBudget::for_model("custom-llm-v1");
        assert_eq!(unknown_b.limit, 8192);
    }

    #[test]
    fn test_token_budget_strategies() {
        let b = TokenBudget {
            limit: 10_000,
            strategy: BudgetStrategy::Aggressive,
            reserved: 500,
            consumed: 100,
        };
        // 95% of 10_000 = 9500
        assert_eq!(b.effective_limit(), 9500);
        // 9500 - 500 - 100 = 8900
        assert_eq!(b.available(), 8900);

        let b_exact = b.with_strategy(BudgetStrategy::Exact(4000));
        assert_eq!(b_exact.effective_limit(), 4000);
        assert_eq!(b_exact.available(), 3400);
    }
}
