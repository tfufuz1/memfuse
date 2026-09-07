//! F-08: PID-Regler für Reranking-Kandidatenpool-Größe.
//!
//! Hält Reranking-Latenz auf `target_latency_ms` durch adaptive Pool-Größe.
//! ANTI-WINDUP: Integral-Term wird auf [-max_integral, +max_integral] geclipped.

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
    /// Minimale zulässige Pool-Größe.
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
        Self {
            kp: 2.0,
            ki: 0.1,
            kd: 0.5,
            target_latency_ms: 200.0,
            min_pool_size: 10,
            max_pool_size: 500,
            current_pool_size: None,
            integral: 0.0,
            prev_error: 0.0,
            max_integral: 100.0,
        }
    }
}

impl PidController {
    /// Verarbeitet eine neue Latenz-Messung und gibt die neue Pool-Größe zurück.
    ///
    /// ANTI-WINDUP: Integral wird auf [-max_integral, max_integral] geclipped.
    pub fn update(&mut self, current_pool_size: usize, measured_latency_ms: f32) -> usize {
        let error = self.target_latency_ms - measured_latency_ms;
        self.integral = (self.integral + error).clamp(-self.max_integral, self.max_integral);
        let derivative = error - self.prev_error;
        let u = self.kp * error + self.ki * self.integral + self.kd * derivative;
        self.prev_error = error;

        let new_size = (current_pool_size as f32 + u).round() as isize;
        let clamped = new_size.clamp(self.min_pool_size as isize, self.max_pool_size as isize) as usize;
        self.current_pool_size = Some(clamped);
        clamped
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
        let new_pool = pid.update(initial_pool, 200.0); // measured_latency = target_latency_ms
        assert_eq!(new_pool, initial_pool);
        assert_eq!(pid.current_pool_size, Some(initial_pool));
    }

    #[test]
    fn test_pid_latency_too_high_reduces_pool() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let new_pool = pid.update(initial_pool, 300.0); // measured_latency (300) > target (200)
        assert!(new_pool < initial_pool);
        assert_eq!(pid.current_pool_size, Some(new_pool));
    }

    #[test]
    fn test_pid_latency_too_low_increases_pool() {
        let mut pid = PidController::default();
        let initial_pool = 100;
        let new_pool = pid.update(initial_pool, 100.0); // measured_latency (100) < target (200)
        assert!(new_pool > initial_pool);
        assert_eq!(pid.current_pool_size, Some(new_pool));
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
        let mut pid = PidController {
            min_pool_size: 20,
            max_pool_size: 150,
            ..Default::default()
        };

        // Force drastic reduction below min
        let res_min = pid.update(10, 10000.0);
        assert_eq!(res_min, 20);
        assert_eq!(pid.current_pool_size, Some(20));

        // Force drastic increase above max
        let res_max = pid.update(200, 0.0);
        assert_eq!(res_max, 150);
        assert_eq!(pid.current_pool_size, Some(150));
    }
}
