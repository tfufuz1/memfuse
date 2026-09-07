//! Freie-Energie-Thermostat (F-01) — Adaptiver Verfall.
//!
//! FEATURE-FLAG: `physio-features` (default-off in P1-safe Config).
//! KEINE neuen Infrastruktur-Abhängigkeiten — alle Inputs sind bereits vorhanden:
//! - tombstone_ratio: HnswIndex::tombstone_ratio()
//! - query_load_inverse: AtomicU64-Zähler in LsmStorage
//!
//! FORMEL:
//! T(t) = w1 * tombstone_ratio + w2 * query_load_inverse  ∈ [0,1]
//! half_life_eff = half_life_base * (1 + κ * (1 − T(t)))
//! effective_score = base_score * exp(−(ln2 / half_life_eff) * elapsed_tx)
//!
//! SEMANTIK:
//! Hohe T (voller Speicher) → kurze Half-Life → aggressives Vergessen.
//! Niedrige T (freier Speicher) → lange Half-Life → längeres Behalten.
//! ADR-016: TxId-Differenz statt SystemTime (deterministisch, replay-safe).

/// Thermostat-Inputs aus bestehenden System-Metriken.
#[derive(Debug, Clone, Copy)]
pub struct ThermostatInputs {
    /// HNSW-Tombstone-Ratio ∈ [0,1]. Quelle: HnswIndex::tombstone_ratio().
    pub tombstone_ratio: f32,
    /// Normalisierte inverse Query-Rate ∈ [0,1].
    /// Berechnung: 1 - (queries_in_window / max_queries_per_window).
    /// Hoher Wert = wenig Queries = System idle = konservativer Verfall.
    pub query_load_inverse: f32,
    pub w_tombstone: f32,
    pub w_query: f32,
}

impl Default for ThermostatInputs {
    fn default() -> Self {
        Self {
            tombstone_ratio: 0.0,
            query_load_inverse: 0.5,
            w_tombstone: 0.6,
            w_query: 0.4,
        }
    }
}

/// Konfiguration für den Thermostat (via PhysioConfig).
#[derive(Debug, Clone)]
pub struct ThermostatConfig {
    /// Verstärkungsfaktor κ. Default: 2.0.
    pub kappa: f32,
    /// Basis-Half-Life in TxId-Einheiten. Default: 10_000.
    pub base_half_life_tx: u64,
    /// Eviction-Schwelle: Scores darunter werden evicted. Default: 0.01.
    pub eviction_threshold: f32,
}

impl Default for ThermostatConfig {
    fn default() -> Self {
        Self {
            kappa: 2.0,
            base_half_life_tx: 10_000,
            eviction_threshold: 0.01,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FreeEnergyThermostat {
    config: ThermostatConfig,
}

impl FreeEnergyThermostat {
    pub fn new(config: ThermostatConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(ThermostatConfig::default())
    }

    /// Berechnet aktuelle Systemtemperatur T(t) ∈ [0,1].
    #[inline]
    pub fn system_temperature(&self, inputs: &ThermostatInputs) -> f32 {
        (inputs.w_tombstone * inputs.tombstone_ratio
            + inputs.w_query * inputs.query_load_inverse)
            .clamp(0.0, 1.0)
    }

    /// Effektive Half-Life unter aktueller Temperatur.
    #[inline]
    pub fn effective_half_life(&self, temperature: f32) -> f32 {
        self.config.base_half_life_tx as f32
            * (1.0 + self.config.kappa * (1.0 - temperature))
    }

    /// Temperatur-adjustierter Score für einen Chunk.
    ///
    /// `elapsed_tx`: TxId(now) - TxId(chunk_created). ADR-016: kein SystemTime.
    pub fn effective_score(
        &self,
        base_score: f32,
        elapsed_tx: u64,
        inputs: &ThermostatInputs,
    ) -> f32 {
        let temperature = self.system_temperature(inputs);
        let half_life = self.effective_half_life(temperature);
        let decay_exponent = -(std::f32::consts::LN_2 / half_life) * elapsed_tx as f32;
        // Verhindert NaN bei sehr großem elapsed_tx
        base_score * decay_exponent.exp().max(0.0)
    }

    /// Gibt true wenn Chunk unter Eviction-Schwelle.
    /// Wird von Reaper (reaper.rs) für TTL-Sweep verwendet.
    #[inline]
    pub fn should_evict(
        &self,
        base_score: f32,
        elapsed_tx: u64,
        inputs: &ThermostatInputs,
    ) -> bool {
        self.effective_score(base_score, elapsed_tx, inputs) < self.config.eviction_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_clamp() {
        let t = FreeEnergyThermostat::with_defaults();
        let inputs = ThermostatInputs {
            tombstone_ratio: 1.0,
            query_load_inverse: 1.0,
            ..Default::default()
        };
        let temp = t.system_temperature(&inputs);
        assert!(
            (0.0..=1.0).contains(&temp),
            "Temperatur muss in [0,1] sein: {temp}"
        );
    }

    #[test]
    fn test_high_temperature_shorter_half_life() {
        let t = FreeEnergyThermostat::with_defaults();
        let hot = ThermostatInputs {
            tombstone_ratio: 1.0,
            query_load_inverse: 1.0,
            ..Default::default()
        };
        let cold = ThermostatInputs {
            tombstone_ratio: 0.0,
            query_load_inverse: 0.0,
            ..Default::default()
        };

        let temp_hot = t.system_temperature(&hot);
        let temp_cold = t.system_temperature(&cold);

        assert!(
            t.effective_half_life(temp_hot) < t.effective_half_life(temp_cold),
            "Hohe Temperatur → kürzere Half-Life"
        );
    }

    #[test]
    fn test_effective_score_decays_over_time() {
        let t = FreeEnergyThermostat::with_defaults();
        let inputs = ThermostatInputs::default();
        let early = t.effective_score(1.0, 100, &inputs);
        let late = t.effective_score(1.0, 10_000, &inputs);
        assert!(
            early > late,
            "Score soll über Zeit fallen: {early} > {late}"
        );
    }

    #[test]
    fn test_effective_score_non_negative() {
        let t = FreeEnergyThermostat::with_defaults();
        let inputs = ThermostatInputs {
            tombstone_ratio: 1.0,
            ..Default::default()
        };
        // Sehr langer Zeitraum — kein negativer Score
        let score = t.effective_score(1.0, u64::MAX / 2, &inputs);
        assert!(score >= 0.0, "Score darf nie negativ sein: {score}");
    }

    #[test]
    fn test_should_evict_low_score_high_temp() {
        let t = FreeEnergyThermostat::with_defaults();
        let hot = ThermostatInputs {
            tombstone_ratio: 1.0,
            query_load_inverse: 1.0,
            ..Default::default()
        };
        // Nach sehr langer Zeit bei hoher Temperatur → eviction
        assert!(t.should_evict(1.0, 1_000_000, &hot));
    }

    #[test]
    fn test_should_not_evict_fresh_chunk() {
        let t = FreeEnergyThermostat::with_defaults();
        let inputs = ThermostatInputs::default();
        // Frischer Chunk (elapsed_tx=0) → kein Evict
        assert!(!t.should_evict(1.0, 0, &inputs));
    }
}
