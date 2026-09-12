// FILE-CONTEXT
// STAND: 2026-09-11T00:00:00Z
// ZWECK: Adaptive Reranking candidate pool-size regulation via PID latency control (F-08 & P11).
// INVARIANTEN: Pool size bounded by [min_pool_size, max_pool_size], min_pool_size >= 50 hard floor, non-finite latency measurements ignored.
// NICHT-OFFENSICHTLICH: Anti-windup integral clamping prevents overshoot under sustained latency spikes.
// SIEHE AUCH: crates/memfuse-calibration/src/lib.rs, crates/memfuse-db/src/homeostat.rs

//! F-08: PID-Regler für Reranking-Kandidatenpool-Größe.
//!
//! Hält Reranking-Latenz auf `target_latency_ms` durch adaptive Pool-Größe.
//! ANTI-WINDUP: Integral-Term wird auf [-max_integral, +max_integral] geclipped.
//!
//! # Scientific Foundation
//! arXiv:2604.01733 (T2-RAGBench) demonstrate the non-linear relationship between candidate pool size (`k_pool`) and retrieval Recall@5:
//! - `k_pool = 20` → Recall@5 = 0.458
//! - `k_pool = 50` → Recall@5 = 0.826 (Quality Knee / Knee Point)
//! - `k_pool = 100` → Recall@5 = 0.888
//!
//! While Recall@5 reaches 0.888 at pool >= 100, `PID_MIN_POOL_SIZE_DEFAULT = 50` is enforced as the strict hard floor (`min_pool_size >= 50`).
//! Under extreme latency spikes, pool size may contract down to 50 (accepting a bounded recall drop to 0.826 at the Quality Knee),
//! but will NEVER drop to 20 or below where Recall collapses catastrophically (0.458).

/// Minimale Kandidaten-Pool-Größe (Hard Floor = 50, Quality Knee per arXiv:2604.01733).
pub const PID_MIN_POOL_SIZE_DEFAULT: usize = 50;

/// Maximale Kandidaten-Pool-Größe.
pub const PID_MAX_POOL_SIZE_DEFAULT: usize = 200;

/// PID-Regler zur dynamischen Steuerung der Reranking-Kandidatenpool-Größe basierend auf Latenzmessungen.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PidController {
    /// Proportionaler Gewichtsbeiwert $K_p$.
    pub kp: f32,
    /// Integraler Gewichtsbeiwert $K_i$.
    pub ki: f32,
    /// Differentieller Gewichtsbeiwert $K_d$.
    pub kd: f32,
    /// Ziel-Reranking-Latenz in Millisekunden.
    pub target_latency_ms: f32,
    /// Minimale zulässige Pool-Größe (stets >= 50).
    pub min_pool_size: usize,
    /// Maximale zulässige Pool-Größe.
    pub max_pool_size: usize,
    /// Aktuell empfohlene Pool-Größe.
    pub current_pool_size: Option<usize>,
    integral: f32,
    prev_error: f32,
    max_integral: f32, // Anti-windup
}

impl Default for PidController {
    fn default() -> Self {
        Self::new(
            150.0,
            PID_MIN_POOL_SIZE_DEFAULT,
            PID_MAX_POOL_SIZE_DEFAULT,
            None,
        )
    }
}

impl PidController {
    /// Erstellt einen neuen `PidController` mit expliziten Parametern.
    ///
    /// `min_pool_size` wird strikt auf mindestens 50 erzwungen (`min_pool_size.max(50)`).
    pub fn new(
        target_latency_ms: f32,
        min_pool_size: usize,
        max_pool_size: usize,
        initial_pool: Option<usize>,
    ) -> Self {
        let min_pool_size = min_pool_size.max(PID_MIN_POOL_SIZE_DEFAULT);
        let max_pool_size = max_pool_size.max(min_pool_size);
        let current_pool_size = initial_pool.map(|p| p.clamp(min_pool_size, max_pool_size));
        Self {
            kp: 0.5,
            ki: 0.05,
            kd: 0.1,
            target_latency_ms,
            min_pool_size,
            max_pool_size,
            current_pool_size,
            integral: 0.0,
            prev_error: 0.0,
            max_integral: 100.0,
        }
    }

    /// Verarbeitet eine neue Latenz-Messung und gibt die neue Pool-Größe zurück.
    ///
    /// ANTI-WINDUP: Integral wird auf [-max_integral, max_integral] geclipped.
    pub fn update(&mut self, current_pool_size: usize, measured_latency_ms: f32) -> usize {
        if !measured_latency_ms.is_finite() {
            return self.current_pool_size.unwrap_or(current_pool_size);
        }
        let error = self.target_latency_ms - measured_latency_ms;
        self.integral = (self.integral + error).clamp(-self.max_integral, self.max_integral);
        let derivative = error - self.prev_error;
        let u = self.kp * error + self.ki * self.integral + self.kd * derivative;
        self.prev_error = error;

        let new_size = (current_pool_size as f32 + u).round() as isize;
        let clamped =
            new_size.clamp(self.min_pool_size as isize, self.max_pool_size as isize) as usize;
        self.current_pool_size = Some(clamped);
        clamped
    }

    /// Gibt die aktuell empfohlene Pool-Größe zurück.
    pub fn current_pool_size(&self) -> Option<usize> {
        self.current_pool_size
    }

    /// Setzt Integral und Derivative auf 0 zurück (z.B. bei Konfigurationsänderung).
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
        self.current_pool_size = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_stable_at_target() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let new_pool = pid.update(initial_pool, 150.0); // measured_latency = target_latency_ms
        assert_eq!(new_pool, initial_pool);
        assert_eq!(pid.current_pool_size, Some(initial_pool));
    }

    #[test]
    fn test_pid_latency_too_high_reduces_pool() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let new_pool = pid.update(initial_pool, 300.0); // measured_latency (300) > target (150)
        assert!(new_pool < initial_pool);
        assert_eq!(pid.current_pool_size, Some(new_pool));
    }

    #[test]
    fn test_pid_latency_too_low_increases_pool() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let new_pool = pid.update(initial_pool, 50.0); // measured_latency (50) < target (150)
        assert!(new_pool > initial_pool);
        assert_eq!(pid.current_pool_size, Some(new_pool));
    }

    #[test]
    fn test_pid_non_finite_latency_ignored() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let _ = pid.update(initial_pool, 150.0);
        let integral_before = pid.integral;
        let prev_error_before = pid.prev_error;

        // Test NAN
        let size_nan = pid.update(100, f32::NAN);
        assert_eq!(size_nan, 100);
        assert_eq!(pid.integral, integral_before);
        assert_eq!(pid.prev_error, prev_error_before);

        // Test INFINITY
        let size_inf = pid.update(100, f32::INFINITY);
        assert_eq!(size_inf, 100);
        assert_eq!(pid.integral, integral_before);
        assert_eq!(pid.prev_error, prev_error_before);
    }

    #[test]
    fn test_pid_anti_windup_prevents_overflow() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        // Perform 1000 updates with large error
        for _ in 0..1000 {
            pid.update(initial_pool, 1000.0);
        }
        assert_eq!(pid.integral, -pid.max_integral);

        pid.reset();
        assert_eq!(pid.current_pool_size, None);
        for _ in 0..1000 {
            pid.update(initial_pool, 0.0);
        }
        assert_eq!(pid.integral, pid.max_integral);
    }

    #[test]
    fn test_pid_clamps_to_min_max() {
        let mut pid = PidController::new(150.0, 50, 150, None);

        // Force drastic reduction below min
        let res_min = pid.update(10, 10000.0);
        assert_eq!(res_min, 50);
        assert_eq!(pid.current_pool_size, Some(50));

        // Force drastic increase above max
        let res_max = pid.update(200, 0.0);
        assert_eq!(res_max, 150);
        assert_eq!(pid.current_pool_size, Some(150));
    }

    #[test]
    fn test_high_latency_causes_monotonic_decrease_bounded_by_k_min() {
        let mut pid = PidController::new(150.0, 50, 200, Some(100));
        let mut prev_pool = pid.current_pool_size.unwrap();

        // Simulate 10 iterations of severe latency overflow (300ms vs 150ms target)
        for _ in 0..10 {
            let new_pool = pid.update(prev_pool, 300.0);
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
            pid.current_pool_size,
            Some(50),
            "Pool size should settle at hard minimum k_min=50"
        );
    }

    #[test]
    fn test_low_latency_causes_increase_bounded_by_k_max() {
        let mut pid = PidController::new(150.0, 50, 200, Some(100));
        let mut prev_pool = pid.current_pool_size.unwrap();

        // Simulate 10 iterations of low latency (50ms vs 150ms target)
        for _ in 0..10 {
            let new_pool = pid.update(prev_pool, 50.0);
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
            pid.current_pool_size,
            Some(200),
            "Pool size should settle at maximum k_max=200"
        );
    }

    #[test]
    fn test_oscillating_latency_anti_windup_clamp() {
        let mut pid = PidController::new(150.0, 50, 200, Some(100));
        let mut pool = 100;

        // Extreme positive and negative spikes
        for i in 0..20 {
            let observed = if i % 2 == 0 { 1000.0 } else { 1.0 };
            pool = pid.update(pool, observed);
            assert!(
                pid.integral >= -100.0 && pid.integral <= 100.0,
                "Integral windup protection failed: integral = {}",
                pid.integral
            );
        }
    }

    #[test]
    fn test_hard_k_min_floor_never_below_50() {
        // Attempt to create a controller with invalid min_pool_size=10 (below scientific floor 50)
        let mut pid = PidController::new(150.0, 10, 200, Some(100));
        assert_eq!(
            pid.min_pool_size, 50,
            "Constructor must enforce hard floor min_pool_size >= 50"
        );

        let mut pool = 100;
        // Drive latency extremely high
        for _ in 0..50 {
            pool = pid.update(pool, 10_000.0);
            assert!(
                pool >= 50,
                "pool must NEVER drop below 50 regardless of control input"
            );
        }
    }

    #[test]
    fn test_pid_nan_latency_preserves_state_and_returns_current_pool() {
        let mut pid = PidController::default();
        pid.update(100, 250.0);

        let ref_integral = pid.integral;
        let ref_prev_error = pid.prev_error;
        let ref_current_pool_size = pid.current_pool_size;
        let expected_pool = ref_current_pool_size.expect("current_pool_size should be set");

        let result = pid.update(999, f32::NAN);

        assert_eq!(result, expected_pool);
        assert_eq!(pid.integral, ref_integral);
        assert_eq!(pid.prev_error, ref_prev_error);
        assert_eq!(pid.current_pool_size, ref_current_pool_size);
    }

    #[test]
    fn test_pid_positive_infinity_latency_preserves_state() {
        let mut pid = PidController::default();
        pid.update(100, 250.0);

        let ref_integral = pid.integral;
        let ref_prev_error = pid.prev_error;
        let ref_current_pool_size = pid.current_pool_size;
        let expected_pool = ref_current_pool_size.expect("current_pool_size should be set");

        let result = pid.update(999, f32::INFINITY);

        assert_eq!(result, expected_pool);
        assert_eq!(pid.integral, ref_integral);
        assert_eq!(pid.prev_error, ref_prev_error);
        assert_eq!(pid.current_pool_size, ref_current_pool_size);
    }

    #[test]
    fn test_pid_negative_infinity_latency_preserves_state() {
        let mut pid = PidController::default();
        pid.update(100, 250.0);

        let ref_integral = pid.integral;
        let ref_prev_error = pid.prev_error;
        let ref_current_pool_size = pid.current_pool_size;
        let expected_pool = ref_current_pool_size.expect("current_pool_size should be set");

        let result = pid.update(999, f32::NEG_INFINITY);

        assert_eq!(result, expected_pool);
        assert_eq!(pid.integral, ref_integral);
        assert_eq!(pid.prev_error, ref_prev_error);
        assert_eq!(pid.current_pool_size, ref_current_pool_size);
    }

    #[test]
    fn test_pid_nan_on_fresh_controller_returns_input_pool_size() {
        let mut pid = PidController::default();
        assert_eq!(pid.current_pool_size, None);

        let result = pid.update(77, f32::NAN);

        assert_eq!(result, 77);
        assert_eq!(pid.integral, 0.0);
        assert_eq!(pid.prev_error, 0.0);
        assert_eq!(pid.current_pool_size, None);
    }

    #[test]
    fn test_pid_nan_latency_returns_unchanged_pool_size() {
        let mut pid = PidController::default();
        let pool_after_valid = pid.update(100, 250.0);
        assert_ne!(pool_after_valid, 100);

        let pool_after_nan = pid.update(999, f32::NAN);
        assert_eq!(pool_after_nan, pool_after_valid);
    }

    #[test]
    fn test_pid_positive_infinity_latency_returns_unchanged_pool_size() {
        let mut pid = PidController::default();
        let pool_after_valid = pid.update(100, 250.0);
        assert_ne!(pool_after_valid, 100);

        let pool_after_inf = pid.update(999, f32::INFINITY);
        assert_eq!(pool_after_inf, pool_after_valid);
    }

    #[test]
    fn test_pid_negative_infinity_latency_returns_unchanged_pool_size() {
        let mut pid = PidController::default();
        let pool_after_valid = pid.update(100, 250.0);
        assert_ne!(pool_after_valid, 100);

        let pool_after_neg_inf = pid.update(999, f32::NEG_INFINITY);
        assert_eq!(pool_after_neg_inf, pool_after_valid);
    }

    #[test]
    fn test_pid_state_unchanged_after_nan_input() {
        let mut pid = PidController::default();
        pid.update(100, 250.0);

        let integral_before = pid.integral;
        let prev_error_before = pid.prev_error;

        pid.update(999, f32::NAN);

        assert_eq!(pid.integral, integral_before);
        assert_eq!(pid.prev_error, prev_error_before);
    }

    #[test]
    fn test_pid_recovers_correctly_after_nan_then_valid_input() {
        let mut pid_with_nan = PidController::default();
        let pool_nan_1 = pid_with_nan.update(100, 250.0);
        pid_with_nan.update(999, f32::NAN);
        let pool_nan_3 = pid_with_nan.update(pool_nan_1, 180.0);

        let mut pid_without_nan = PidController::default();
        let pool_no_nan_1 = pid_without_nan.update(100, 250.0);
        let pool_no_nan_2 = pid_without_nan.update(pool_no_nan_1, 180.0);

        assert_eq!(pool_nan_3, pool_no_nan_2);
        assert_eq!(pid_with_nan.integral, pid_without_nan.integral);
        assert_eq!(pid_with_nan.prev_error, pid_without_nan.prev_error);
    }

    #[test]
    fn test_pid_multiple_consecutive_nan_calls_stable() {
        let mut pid = PidController::default();
        let initial_pool = pid.update(100, 250.0);

        let res1 = pid.update(999, f32::NAN);
        let res2 = pid.update(888, f32::NAN);
        let res3 = pid.update(777, f32::NAN);

        assert_eq!(res1, initial_pool);
        assert_eq!(res2, initial_pool);
        assert_eq!(res3, initial_pool);
        assert_eq!(pid.current_pool_size, Some(initial_pool));
    }
}
