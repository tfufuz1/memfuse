// FILE-CONTEXT
// ZWECK: P95 retrieval latency feedback-regulated PID controller and hard deadline management (Feature F-08 & P11 requirement).
// INVARIANTEN: k_min >= 50 hard floor (arXiv:2604.01733 T2-RAGBench recall floor); integral windup bounded [-100.0, 100.0].
// STAND: TS:2026-08-30T22:00:00Z

//! PID Controller and Deadline Management for Retrieval Latency Control.
//!
//! # Scientific Foundation
//! arXiv:2604.01733 (T2-RAGBench) demonstrates the non-linear relationship between candidate pool size (`k_pool`) and retrieval Recall@5:
//! - `k_pool = 20` → Recall@5 = 0.458
//! - `k_pool = 50` → Recall@5 = 0.826 (Quality Knee / Knee Point)
//! - `k_pool = 100` → Recall@5 = 0.888
//!
//! Therefore, `k_min = 50` is enforced as a strict hard lower limit (`k_min >= 50`). Even under extreme latency overflow,
//! the PID controller will never regulate `k_pool` below 50 to prevent severe Recall degraded states.

use std::time::{Duration, Instant};

/// P95 Latency-feedback PID Controller for dynamically tuning candidate pool sizes.
#[derive(Debug, Clone)]
pub struct RerankPidController {
    /// Proportional gain coefficient (default: 0.5).
    pub kp: f32,
    /// Integral gain coefficient (default: 0.05).
    pub ki: f32,
    /// Derivative gain coefficient (default: 0.1).
    pub kd: f32,
    /// Target P95 retrieval/reranking latency in milliseconds (default: 150.0 ms).
    pub target_p95_latency_ms: f32,
    /// Accumulated integral error (clamped to prevent windup).
    pub integral: f32,
    /// Error recorded during the previous update cycle.
    pub last_error: f32,
    /// Current regulated candidate pool size.
    pub k_pool: usize,
    /// Strict minimum candidate pool size (hard limit >= 50 per arXiv:2604.01733).
    pub k_min: usize,
    /// Maximum candidate pool size (default: 200).
    pub k_max: usize,
}

impl Default for RerankPidController {
    fn default() -> Self {
        Self::new(150.0, 50, 200, 100)
    }
}

impl RerankPidController {
    /// Creates a new `RerankPidController` with default PID coefficients and configured parameters.
    ///
    /// `k_min` is strictly enforced to be at least 50 (`k_min = k_min.max(50)`).
    pub fn new(
        target_p95_latency_ms: f32,
        k_min: usize,
        k_max: usize,
        initial_pool: usize,
    ) -> Self {
        let k_min = k_min.max(50);
        let k_max = k_max.max(k_min);
        let initial_pool = initial_pool.clamp(k_min, k_max);
        Self {
            kp: 0.5,
            ki: 0.05,
            kd: 0.1,
            target_p95_latency_ms,
            integral: 0.0,
            last_error: 0.0,
            k_pool: initial_pool,
            k_min,
            k_max,
        }
    }

    /// Updates the controller state with the latest observed P95 latency measurement
    /// and returns the updated `k_pool` candidate pool size.
    ///
    /// # Control Logic
    /// - Error: `error = target - observed`
    /// - Integral: `integral = (integral + error).clamp(-100.0, 100.0)` (Windup Protection)
    /// - Derivative: `derivative = error - last_error`
    /// - PID Output: `output = kp * error + ki * integral + kd * derivative`
    /// - New Pool Size: `k_pool = (k_pool + output).round().clamp(k_min, k_max)`
    ///
    /// When observed latency exceeds the target (`observed > target`), `error` and `output` become negative,
    /// causing `k_pool` to decrease (reducing computational load). Conversely, when observed latency is well below
    /// the target, `k_pool` increases up to `k_max`.
    pub fn update(&mut self, observed_p95_latency_ms: f32) -> usize {
        let error = self.target_p95_latency_ms - observed_p95_latency_ms;
        self.integral = (self.integral + error).clamp(-100.0, 100.0);
        let derivative = error - self.last_error;
        let output = self.kp * error + self.ki * self.integral + self.kd * derivative;
        self.last_error = error;

        let new_pool_f32 = (self.k_pool as f32 + output).round();
        let clamped = new_pool_f32.clamp(self.k_min as f32, self.k_max as f32);
        self.k_pool = clamped as usize;
        self.k_pool
    }
}

/// Free function to update the PID controller with observed latency and return the regulated candidate pool size.
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
    fn test_high_latency_causes_monotonic_decrease_bounded_by_k_min() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let mut prev_pool = controller.k_pool;

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
            controller.k_pool, 50,
            "Pool size should settle at hard minimum k_min=50"
        );
    }

    #[test]
    fn test_low_latency_causes_increase_bounded_by_k_max() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let mut prev_pool = controller.k_pool;

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
            controller.k_pool, 200,
            "Pool size should settle at maximum k_max=200"
        );
    }

    #[test]
    fn test_oscillating_latency_anti_windup_clamp() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);

        // Extreme positive and negative spikes
        for i in 0..20 {
            let observed = if i % 2 == 0 { 1000.0 } else { 1.0 };
            controller.update(observed);
            assert!(
                controller.integral >= -100.0 && controller.integral <= 100.0,
                "Integral windup protection failed: integral = {}",
                controller.integral
            );
        }
    }

    #[test]
    fn test_hard_k_min_floor_never_below_50() {
        // Attempt to create a controller with invalid k_min=10 (below scientific floor 50)
        let mut controller = RerankPidController::new(150.0, 10, 200, 100);
        assert_eq!(
            controller.k_min, 50,
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
    fn test_pid_regulated_candidate_pool_free_function() {
        let mut controller = RerankPidController::new(150.0, 50, 200, 100);
        let pool = pid_regulated_candidate_pool(&mut controller, 200.0);
        assert!(pool < 100);
        assert_eq!(pool, controller.k_pool);
    }
}
