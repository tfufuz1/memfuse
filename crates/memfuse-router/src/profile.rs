//! Profile definition for Small Language Models (SLMs) in MemFuse Router.

use memfuse_core::{MemFuseError, Result, TokenBudget};
use serde::{Deserialize, Serialize};

/// Represents a Small Language Model (SLM) target and its domain expertise parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlmProfile {
    /// Identifier name of the SLM (e.g. "coding-slm", "docs-slm").
    pub name: String,
    /// MCP endpoint address or URI for client communication.
    pub mcp_endpoint: String,
    /// Vector of graph community IDs this SLM is domain-responsible for.
    pub domain_communities: Vec<u64>,
    /// Token budget configuration for prompt context trimming.
    pub token_budget: TokenBudget,
    /// Minimum relevance threshold score required for routing candidates.
    pub min_relevance_score: f32,
}

impl SlmProfile {
    /// Creates a new `SlmProfile`.
    pub fn new(
        name: impl Into<String>,
        mcp_endpoint: impl Into<String>,
        domain_communities: Vec<u64>,
        token_budget: TokenBudget,
        min_relevance_score: f32,
    ) -> Self {
        Self {
            name: name.into(),
            mcp_endpoint: mcp_endpoint.into(),
            domain_communities,
            token_budget,
            min_relevance_score,
        }
    }

    /// Validates `SlmProfile` parameters.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "SLM profile name cannot be empty".to_string(),
            ));
        }
        if self.mcp_endpoint.trim().is_empty() {
            return Err(MemFuseError::InvalidInput(
                "MCP endpoint cannot be empty".to_string(),
            ));
        }
        if !self.min_relevance_score.is_finite() || self.min_relevance_score < 0.0 {
            return Err(MemFuseError::InvalidInput(
                "min_relevance_score must be finite and non-negative".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates and validates a new `SlmProfile`.
    pub fn try_new(
        name: impl Into<String>,
        mcp_endpoint: impl Into<String>,
        domain_communities: Vec<u64>,
        token_budget: TokenBudget,
        min_relevance_score: f32,
    ) -> Result<Self> {
        let profile = Self::new(
            name,
            mcp_endpoint,
            domain_communities,
            token_budget,
            min_relevance_score,
        );
        profile.validate()?;
        Ok(profile)
    }
}

/// Laufzeit-Kalibrierungsstatistik für ein SLM-Profil.
/// Wird von RouterEngine verwaltet und nicht vom Aufrufer gesetzt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileCalibrationState {
    /// Gesamtzahl der Routing-Entscheidungen für dieses Profil.
    pub times_selected: u64,
    /// Kumulierte Routing-Konfidenz (best_score / second_best_score).
    /// Höher = distinktiver. Startwert: 1.0
    pub cumulative_confidence: f64,
    /// Aktuell kalibrierter min_relevance_score-Wert
    /// (kann von original_min_relevance_score abweichen).
    pub calibrated_min_score: f32,
    /// Ursprünglicher min_relevance_score aus der Konfiguration.
    pub original_min_score: f32,
}

impl ProfileCalibrationState {
    pub fn new(original_min_score: f32) -> Self {
        Self {
            times_selected: 0,
            cumulative_confidence: 1.0,
            calibrated_min_score: original_min_score,
            original_min_score,
        }
    }

    /// Durchschnittliche Konfidenz über alle bisherigen Entscheidungen.
    pub fn average_confidence(&self) -> f64 {
        if self.times_selected == 0 {
            return 1.0;
        }
        self.cumulative_confidence / self.times_selected as f64
    }

    /// Erhöht min_score falls durchschnittliche Konfidenz unter Schwellenwert.
    /// Gibt true zurück wenn eine Anpassung vorgenommen wurde.
    pub fn recalibrate(&mut self, low_confidence_threshold: f64) -> bool {
        if self.times_selected < 10 {
            // Nicht genug Daten für Kalibrierung
            return false;
        }
        let avg = self.average_confidence();
        if avg < low_confidence_threshold {
            // Score um 10% erhöhen, max 2× original
            let new_score = (self.calibrated_min_score * 1.1).min(self.original_min_score * 2.0);
            if new_score != self.calibrated_min_score {
                self.calibrated_min_score = new_score;
                return true;
            }
        } else if avg > low_confidence_threshold * 1.5 {
            // Konfidenz gut → Score in Richtung original relaxieren
            let new_score = (self.calibrated_min_score * 0.95).max(self.original_min_score);
            self.calibrated_min_score = new_score;
        }
        false
    }

    /// Setzt Kalibrierungsstate vollständig zurück.
    pub fn reset(&mut self) {
        self.times_selected = 0;
        self.cumulative_confidence = 1.0;
        self.calibrated_min_score = self.original_min_score;
    }
}
