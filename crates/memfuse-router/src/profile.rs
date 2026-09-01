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
        if !self.min_relevance_score.is_finite() {
            return Err(MemFuseError::InvalidInput(
                "min_relevance_score cannot be NaN or Infinite".to_string(),
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
