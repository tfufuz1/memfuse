// FILE-CONTEXT Header (Format v3)
// ZWECK: Step result structures and AgentTool trait definitions for orchestration.
// INVARIANTEN: StepResult contains node_id, output payload, and consumed tokens; AgentTool enforces async execution and cost estimation.
// NICHT-OFFENSICHTLICH: estimated_cost defaults to 0 for zero-cost tools and enables pre-execution budget validation.
// HOTSPOTS: AgentTool::execute (ll. 25-35).
// STAND: TS:2026-09-02T23:19:10Z (SESSION: 088b4a44)

//! Step result and tool trait definitions for agent workflows.
//!
//! Each agent step produces a [`StepResult`] and tools implement the
//! [`AgentTool`] trait to participate in the orchestration loop.

use crate::context::AgentContext;
use memfuse_core::Result;
use serde::{Deserialize, Serialize};

/// Ein fehlgeschlagener Agent-Schritt der für spätere Analyse persistiert wird.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDeadLetter {
    /// Session-ID des Agenten
    pub session_id: String,
    /// Node-ID des fehlgeschlagenen Schritts
    pub node_id: String,
    /// Ursache des Fehlers
    pub failure_reason: DeadLetterReason,
    /// Input der zum Fehler geführt hat
    pub input: serde_json::Value,
    /// Anzahl bisheriger Versuche (für Retry-Steuerung)
    pub attempt: u32,
    /// Zeitstempel des Fehlers (Unix-Sekunden)
    pub failed_at_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeadLetterReason {
    /// Tool hat nicht innerhalb des Timeouts geantwortet
    Timeout { timeout_ms: u64 },
    /// Budget erschöpft bevor der Schritt starten konnte
    BudgetExhausted { available: usize, required: usize },
    /// Tool hat einen nicht-retriable Fehler zurückgegeben
    ToolError { message: String },
    /// Maximale Retry-Anzahl für diesen Schritt erreicht
    MaxRetriesExceeded { attempts: u32 },
}

/// The explicit result of an agent step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub node_id: String,
    pub output: serde_json::Value,
    pub tokens_consumed: usize,
    /// Identifier condition of the next edge transition if dictated dynamically.
    pub next_edge: Option<String>,
}

#[async_trait::async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;

    /// Returns the estimated token cost of executing this tool with the given input.
    ///
    /// Used for strict pre-execution budget validation to prevent side-effects when budget is exhausted.
    fn estimated_cost(&self, _input: &serde_json::Value) -> usize {
        0
    }

    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;

    /// Maximale Ausführungszeit dieses Tools in Millisekunden.
    /// Standard: 30 Sekunden. Überschreibe für langläufige Tools.
    fn timeout_ms(&self) -> u64 {
        30_000
    }

    /// Ob dieser Schritt bei Timeout/transientem Fehler wiederholbar ist.
    fn is_retriable(&self) -> bool {
        true
    }

    /// Maximale Anzahl Retries. Standard: 2 (d.h. insgesamt 3 Versuche).
    fn max_retries(&self) -> u32 {
        2
    }
}
