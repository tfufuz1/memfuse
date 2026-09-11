// FILE-CONTEXT
// ZWECK: P95 retrieval latency feedback-regulated PID controller and hard deadline management (Feature F-08 & P11 requirement).
// INVARIANTEN: k_min >= 50 hard floor (arXiv:2604.01733 T2-RAGBench recall floor); integral windup bounded [-100.0, 100.0].
// STAND: TS:2026-09-11T00:00:00Z

//! PID Controller and Deadline Management for Retrieval Latency Control.
//!
//! NOTE: `RerankPidController` and `pid_regulated_candidate_pool` have been consolidated into `memfuse_calibration::PidController`.

use std::time::{Duration, Instant};

/// P95 Latency-feedback PID Controller for dynamically tuning candidate pool sizes.
///
/// DEPRECATED: Use [`memfuse_calibration::PidController`] instead.
#[deprecated(
    since = "0.1.0",
    note = "Consolidated into `memfuse_calibration::PidController`. Use `memfuse_calibration::PidController` directly."
)]
#[derive(Debug, Clone)]
pub struct RerankPidController {
    inner: memfuse_calibration::PidController,
}

#[allow(deprecated)]
impl Default for RerankPidController {
    fn default() -> Self {
        Self::new(150.0, 50, 200, 100)
    }
}

#[allow(deprecated)]
impl RerankPidController {
    /// Creates a new `RerankPidController` with default PID coefficients and configured parameters.
    pub fn new(
        target_p95_latency_ms: f32,
        k_min: usize,
        k_max: usize,
        initial_pool: usize,
    ) -> Self {
        Self {
            inner: memfuse_calibration::PidController::new(
                target_p95_latency_ms,
                k_min,
                k_max,
                Some(initial_pool),
            ),
        }
    }

    /// Accessor for `kp`.
    pub fn kp(&self) -> f32 {
        self.inner.kp
    }

    /// Accessor for `ki`.
    pub fn ki(&self) -> f32 {
        self.inner.ki
    }

    /// Accessor for `kd`.
    pub fn kd(&self) -> f32 {
        self.inner.kd
    }

    /// Accessor for target latency.
    pub fn target_p95_latency_ms(&self) -> f32 {
        self.inner.target_latency_ms
    }

    /// Accessor for current `k_pool`.
    pub fn k_pool(&self) -> usize {
        self.inner
            .current_pool_size
            .unwrap_or(self.inner.min_pool_size)
    }

    /// Accessor for `k_min`.
    pub fn k_min(&self) -> usize {
        self.inner.min_pool_size
    }

    /// Accessor for `k_max`.
    pub fn k_max(&self) -> usize {
        self.inner.max_pool_size
    }

    /// Updates the controller state with observed latency and returns new pool size.
    pub fn update(&mut self, observed_p95_latency_ms: f32) -> usize {
        let current = self.k_pool();
        self.inner.update(current, observed_p95_latency_ms)
    }
}

/// Free function to update the PID controller with observed latency and return the regulated candidate pool size.
#[deprecated(
    since = "0.1.0",
    note = "Consolidated into `memfuse_calibration::PidController`. Use `memfuse_calibration::PidController::update` directly."
)]
#[allow(deprecated)]
pub fn pid_regulated_candidate_pool(
    controller: &mut RerankPidController,
    observed_p95_latency_ms: f32,
) -> usize {
    controller.update(observed_p95_latency_ms)
}

/// Hard deadline manager for candidate retrieval and Cross-Encoder reranking phases (P11 requirement).
///
/// # Intended Integration Point
/// Evaluated before or during candidate hydration and Cross-Encoder batch inference steps in `Collection::hybrid_search_reranked`.
/// If `deadline_exceeded(started_at)` returns `true`, the search pipeline should immediately break out and return
/// the partially hydrated/reranked candidate results accumulated up to that point rather than blocking indefinitely.
#[derive(Debug, Clone, Copy)]
pub struct RerankDeadline {
    /// Maximum time budget allowed for reranking operations.
    pub budget: Duration,
}

impl RerankDeadline {
    /// Creates a new `RerankDeadline` with the specified duration budget.
    pub fn new(budget: Duration) -> Self {
        Self { budget }
    }

    /// Checks if the elapsed time since `started_at` equals or exceeds the deadline budget.
    pub fn deadline_exceeded(&self, started_at: Instant) -> bool {
        started_at.elapsed() >= self.budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_high_latency_causes_monotonic_decrease_bounded_by_k_min() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let mut prev_pool = controller.k_pool();

        // Simulate 10 iterations of severe latency overflow (300ms vs 150ms target)
        for _ in 0..10 {
            let new_pool = controller.update(300.0);
            assert!(
                new_pool <= prev_pool,
                "Pool size must monotonically decrease under high latency: prev={prev_pool}, new={new_pool}"
            );
            assert!(
                new_pool >= 50,
                "Pool size must never fall below hard floor k_min=50: got {new_pool}"
            );
            prev_pool = new_pool;
        }

        assert_eq!(
            controller.k_pool(),
            50,
            "Pool size should settle at hard minimum k_min=50"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_low_latency_causes_increase_bounded_by_k_max() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let mut prev_pool = controller.k_pool();

        // Simulate 10 iterations of low latency (50ms vs 150ms target)
        for _ in 0..10 {
            let new_pool = controller.update(50.0);
            assert!(
                new_pool >= prev_pool,
                "Pool size must increase under low latency: prev={prev_pool}, new={new_pool}"
            );
            assert!(
                new_pool <= 200,
                "Pool size must never exceed k_max=200: got {new_pool}"
            );
            prev_pool = new_pool;
        }

        assert_eq!(
            controller.k_pool(),
            200,
            "Pool size should settle at maximum k_max=200"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_hard_k_min_floor_never_below_50() {
        // Attempt to create a controller with invalid k_min=10 (below scientific floor 50)
        let mut controller = RerankPidController::new(150.0, 10, 200, 100);
        assert_eq!(
            controller.k_min(),
            50,
            "Constructor must enforce hard floor k_min >= 50"
        );

        // Drive latency extremely high
        for _ in 0..50 {
            let pool = controller.update(10_000.0);
            assert!(
                pool >= 50,
                "k_pool must NEVER drop below 50 regardless of control input"
            );
        }
    }

    #[test]
    fn test_rerank_deadline_exceeded() {
        let deadline = RerankDeadline::new(Duration::from_millis(50));
        let start = Instant::now();

        assert!(!deadline.deadline_exceeded(start));

        std::thread::sleep(Duration::from_millis(60));

        assert!(deadline.deadline_exceeded(start));
    }

    #[test]
    #[allow(deprecated)]
    fn test_pid_regulated_candidate_pool_free_function() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let pool = pid_regulated_candidate_pool(&mut controller, 200.0);
        assert!(pool < 100);
        assert_eq!(pool, controller.k_pool());
    }
}
