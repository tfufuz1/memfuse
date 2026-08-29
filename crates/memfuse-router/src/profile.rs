//! Profile definition for Small Language Models (SLMs) in MemFuse Router.

use memfuse_core::TokenBudget;
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
}
