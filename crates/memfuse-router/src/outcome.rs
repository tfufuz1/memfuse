//! Outcome types and decision tracking identifiers for router calibration.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Eindeutige ID einer Routing-Entscheidung.
/// Wird von route() zurückgegeben und von record_outcome() konsumiert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DecisionId(u64);

static DECISION_COUNTER: AtomicU64 = AtomicU64::new(0);

impl DecisionId {
    /// Erstellt eine neue eindeutige `DecisionId`.
    pub fn new() -> Self {
        DecisionId(DECISION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Liefert den zugrundeliegenden `u64`-Wert der `DecisionId`.
    pub fn inner(self) -> u64 {
        self.0
    }
}

impl Default for DecisionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Tatsächliches Ergebnis einer getroffenen Routing-Entscheidung.
/// Wird vom Agent-Orchestrator NACH dem SLM-Aufruf geliefert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingOutcome {
    /// SLM hat die Anfrage vollständig und korrekt beantwortet.
    Success,
    /// SLM-Antwort war unzureichend — Eskalation zu größerem Modell war nötig.
    Escalated { escalated_to: String },
    /// Nachgelagerter Evaluator/Judge hat die SLM-Antwort als falsch markiert.
    Rejected { reason: Option<String> },
}

impl RoutingOutcome {
    /// Non-Conformity-Score: 0.0 = perfekt, 1.0 = komplett falsch.
    /// Diese Werte sind Expertenschätzungen und sollten durch A/B-Tests kalibriert werden.
    pub fn non_conformity_score(&self) -> f32 {
        match self {
            RoutingOutcome::Success => 0.0,
            RoutingOutcome::Escalated { .. } => 0.7,
            RoutingOutcome::Rejected { .. } => 1.0,
        }
    }
}
