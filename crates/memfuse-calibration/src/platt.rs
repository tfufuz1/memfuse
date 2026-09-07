//! Platt-Scaling (Logistic Calibration): `sigmoid(A * logit + B)`.
//!
//! Passt rohe Scores/Logits via Maximum-Likelihood-Schätzung und
//! Target-Smoothing (Platt, 1999) an eine kalibrierte Wahrscheinlichkeitsverteilung an.

use memfuse_core::ConfigFingerprint;

/// Platt-Scaler für parametrische Logit/Score-Kalibrierung.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlattScaler {
    a: f32,
    b: f32,
    fingerprint: Option<ConfigFingerprint>,
}

impl Default for PlattScaler {
    fn default() -> Self {
        Self::identity()
    }
}

impl PlattScaler {
    /// Erstellt eine neue `PlattScaler`-Instanz mit angegebenen Parametern `a` und `b`.
    pub fn new(a: f32, b: f32) -> Self {
        Self {
            a,
            b,
            fingerprint: None,
        }
    }

    /// Unkalibrierter Fallback (Identität: $A=1.0, B=0.0$).
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            fingerprint: None,
        }
    }

    /// Prüft, ob diese Instanz dem unkalibrierten Default (`identity()`, $A=1.0, B=0.0$) entspricht.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < f32::EPSILON && self.b.abs() < f32::EPSILON
    }

    /// Prüft, ob das Modell gefittet wurde (d.h. nicht identisch mit unkalibriertem Default ist).
    pub fn is_fitted(&self) -> bool {
        !self.is_identity()
    }

    /// Gibt die aktuellen Parameter `(a, b)` zurück.
    pub fn params(&self) -> (f32, f32) {
        (self.a, self.b)
    }

    /// Berechnet die kalibrierte Wahrscheinlichkeit für einen Roh-Score.
    pub fn predict(&self, raw_score: f32) -> f32 {
        self.transform(raw_score)
    }

    /// Wendet das Platt-Scaling `sigmoid(A * logit + B)` an.
    pub fn transform(&self, logit: f32) -> f32 {
        if logit.is_nan() {
            return 0.5;
        }
        let z = self.a * logit + self.b;
        1.0 / (1.0 + (-z).exp())
    }

    /// P8-PFLICHT: Vollständiger Reset auf Identität bei Fingerprint-Änderung.
    pub fn invalidate_on_config_change(&mut self, new_fingerprint: ConfigFingerprint) {
        if self.fingerprint.as_ref() != Some(&new_fingerprint) {
            tracing::warn!(
                old_fp = ?self.fingerprint,
                new_fp = ?new_fingerprint,
                "PlattScaler: ConfigFingerprint changed — resetting to identity (P8)"
            );
            self.a = 1.0;
            self.b = 0.0;
            self.fingerprint = Some(new_fingerprint);
        }
    }

    /// Fittet Parameter `A` und `B` via Negative-Log-Likelihood-Minimierung mit L2-Regularisierung
    /// und Target-Smoothing (Platt, 1999) auf gelabelten `(logit, is_relevant)`-Beobachtungen.
    pub fn fit(observations: &[(f32, bool)]) -> Self {
        let valid_obs: Vec<(f32, bool)> = observations
            .iter()
            .copied()
            .filter(|(logit, _)| logit.is_finite())
            .collect();

        if valid_obs.is_empty() {
            return Self::identity();
        }

        let pos_count = valid_obs.iter().filter(|(_, is_rel)| *is_rel).count();
        let neg_count = valid_obs.len() - pos_count;

        // Platt Target-Smoothing (Platt, 1999)
        let t_pos = (pos_count as f32 + 1.0) / (pos_count as f32 + 2.0);
        let t_neg = 1.0 / (neg_count as f32 + 2.0);

        let mut a = 1.0f32;
        let mut b = 0.0f32;
        let mut lr = 0.05f32;
        let iterations = 300;
        let l2_reg = 0.001f32;

        for _ in 0..iterations {
            let mut grad_a = 0.0f32;
            let mut grad_b = 0.0f32;

            for &(logit, is_rel) in &valid_obs {
                let target = if is_rel { t_pos } else { t_neg };
                let z = a * logit + b;
                let p = 1.0 / (1.0 + (-z).exp());
                let err = p - target;

                grad_a += err * logit;
                grad_b += err;
            }

            let n = valid_obs.len() as f32;
            grad_a = grad_a / n + l2_reg * (a - 1.0);
            grad_b = grad_b / n + l2_reg * b;

            // Gradient Clipping für numerische Stabilität
            let grad_norm = (grad_a * grad_a + grad_b * grad_b).sqrt();
            if grad_norm > 10.0 {
                grad_a = (grad_a / grad_norm) * 10.0;
                grad_b = (grad_b / grad_norm) * 10.0;
            }

            a -= lr * grad_a;
            b -= lr * grad_b;

            lr *= 0.995;
        }

        if !a.is_finite() || !b.is_finite() {
            return Self::identity();
        }

        Self {
            a,
            b,
            fingerprint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platt_scaler_identity() {
        let default_scaler = PlattScaler::default();
        assert!(default_scaler.is_identity());
        assert!(!default_scaler.is_fitted());
        assert_eq!(default_scaler.params(), (1.0, 0.0));

        let score = 0.5f32;
        let expected = 1.0 / (1.0 + (-0.5f32).exp());
        assert!((default_scaler.predict(score) - expected).abs() < 1e-6);
    }

    #[test]
    fn test_platt_scaler_fit_separable() {
        let mut obs = Vec::new();
        for i in -10..=10 {
            let logit = i as f32 * 0.1;
            let is_rel = logit > 0.0;
            obs.push((logit, is_rel));
        }

        let fitted = PlattScaler::fit(&obs);
        assert!(fitted.is_fitted());
        assert!(fitted.params().0 > 0.0);

        let prob_pos = fitted.predict(1.0);
        let prob_neg = fitted.predict(-1.0);
        assert!(prob_pos > 0.80);
        assert!(prob_neg < 0.20);
    }

    #[test]
    fn test_platt_scaler_non_finite_and_empty() {
        let empty_scaler = PlattScaler::fit(&[]);
        assert!(empty_scaler.is_identity());

        let non_finite_obs = vec![
            (f32::NAN, true),
            (f32::INFINITY, false),
            (f32::NEG_INFINITY, true),
        ];
        let non_finite_scaler = PlattScaler::fit(&non_finite_obs);
        assert!(non_finite_scaler.is_identity());
        assert_eq!(non_finite_scaler.predict(f32::NAN), 0.5);
    }

    #[test]
    fn test_platt_scaler_invalidate() {
        let mut scaler = PlattScaler::new(2.5, -0.5);
        assert!(!scaler.is_identity());

        let fp = ConfigFingerprint::new("m", "Q4", "t", 0.7);
        scaler.invalidate_on_config_change(fp.clone());
        assert!(scaler.is_identity());

        // Same fingerprint -> no reset
        scaler.a = 2.0;
        scaler.invalidate_on_config_change(fp);
        assert_eq!(scaler.params().0, 2.0);
    }
}
