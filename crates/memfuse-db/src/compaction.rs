// memfuse-db/src/compaction.rs
// Context Compaction Engine (Grok Pattern)

//! Context Compaction Engine (Grok Pattern)
//!
//! Replaces stale tool outputs and long conversation histories with compact status tokens
//! to preserve the LLM context window.

use memfuse_core::{ContextChunk, DocId, TokenBudget};

/// Strategie für Context Compaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// Einfachste Strategie: Chunks über Token-Limit werden weggelassen.
    Truncate,
    /// Summarisierung via LLM (erfordert externen Summarizer-Trait).
    Summarize,
    /// Ersetze Tool-Outputs durch kompakte Status-Token.
    StatusToken,
}

/// Kompaktierter Kontext für LLM-Übergabe.
#[derive(Debug, Clone)]
pub struct CompactedContext {
    /// Beibehaltene Chunks (innerhalb Budget).
    pub retained_chunks: Vec<ContextChunk>,
    /// Status-Token für kompaktierte Chunks.
    pub status_tokens: Vec<StatusToken>,
    /// Verbrauchte Tokens.
    pub tokens_used: usize,
}

/// Kompakter Stellvertreter für einen oder mehrere kompaktierte Chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusToken {
    /// Kompakter Beschreibungstext (z. B. "Tool-Output: DB-Abfrage lieferte 42 Ergebnisse").
    pub summary: String,
    /// Anzahl der ersetzten originalen Tokens.
    pub replaced_tokens: usize,
    /// Referenz auf die ersetzten Chunk-IDs.
    pub replaced_doc_ids: Vec<DocId>,
}

/// Context Compaction Engine.
#[derive(Debug)]
pub struct ContextCompactor {
    budget: TokenBudget,
    strategy: CompactionStrategy,
}

impl ContextCompactor {
    /// Creates a new `ContextCompactor` with given `TokenBudget` and `CompactionStrategy`.
    pub fn new(budget: TokenBudget, strategy: CompactionStrategy) -> Self {
        Self { budget, strategy }
    }

    /// Kompaktiert eine Liste von Chunks auf das Token-Budget.
    ///
    /// Priorisiert nach Relevanz-Score. Tool-Output-Chunks (erkennbar an Metadata-Key "tool_output")
    /// werden zuerst kompaktiert.
    pub fn compact(&self, chunks: Vec<ContextChunk>) -> CompactedContext {
        let max_tokens = self.budget.available();
        let mut tokens_used = 0;
        let mut retained = Vec::new();
        let mut status_tokens = Vec::new();

        // Sortierung: Tool-Outputs ans Ende (werden zuerst kompaktiert)
        let mut sorted = chunks;
        sorted.sort_by(|a, b| {
            let a_tool = Self::is_tool_output(a);
            let b_tool = Self::is_tool_output(b);
            match (a_tool, b_tool) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => b
                    .relevance
                    .partial_cmp(&a.relevance)
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        for chunk in sorted {
            let chunk_tokens = chunk.combined_token_count();

            if tokens_used + chunk_tokens <= max_tokens {
                tokens_used += chunk_tokens;
                retained.push(chunk);
            } else {
                // Kompaktierung
                match self.strategy {
                    CompactionStrategy::StatusToken => {
                        let summary = Self::generate_status_token(&chunk);
                        status_tokens.push(StatusToken {
                            summary,
                            replaced_tokens: chunk_tokens,
                            replaced_doc_ids: vec![chunk.doc_id],
                        });
                    }
                    CompactionStrategy::Truncate => {
                        // Chunk wird verworfen
                    }
                    CompactionStrategy::Summarize => {
                        // TODO Sprint 6: Async LLM-Summarization
                        // Fallback: Status-Token
                        let summary = Self::generate_status_token(&chunk);
                        status_tokens.push(StatusToken {
                            summary,
                            replaced_tokens: chunk_tokens,
                            replaced_doc_ids: vec![chunk.doc_id],
                        });
                    }
                }
            }
        }

        CompactedContext {
            retained_chunks: retained,
            status_tokens,
            tokens_used,
        }
    }

    fn is_tool_output(chunk: &ContextChunk) -> bool {
        chunk
            .metadata
            .as_ref()
            .and_then(|m| m.get("tool_output"))
            .is_some()
    }

    fn generate_status_token(chunk: &ContextChunk) -> String {
        let preview: String = chunk.content.chars().take(80).collect();
        format!(
            "[Kompaktiert: {} Tokens — {}...]",
            chunk.token_count, preview
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(id: u64, content: &str, relevance: f32, is_tool: bool) -> ContextChunk {
        let metadata = if is_tool {
            Some(serde_json::json!({"tool_output": true}))
        } else {
            None
        };
        ContextChunk {
            doc_id: DocId::new(id),
            content: content.to_string(),
            relevance,
            token_count: content.len(),
            metadata,
            contextual_prefix: None,
        }
    }

    #[test]
    fn test_compactor_within_budget() {
        let budget = TokenBudget::new(100, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::StatusToken);

        let chunks = vec![
            make_chunk(1, "chunk 1", 0.9, false),
            make_chunk(2, "chunk 2", 0.8, false),
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 2);
        assert!(result.status_tokens.is_empty());
        assert_eq!(result.tokens_used, "chunk 1".len() + "chunk 2".len());
    }

    #[test]
    fn test_compactor_tool_output_compacted_first() {
        let budget = TokenBudget::new(20, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::StatusToken);

        let chunks = vec![
            make_chunk(1, "tool output data 12345", 0.95, true), // 22 bytes, tool output
            make_chunk(2, "important context", 0.8, false),      // 17 bytes, normal
        ];

        let result = compactor.compact(chunks);
        // Important context should be retained (17 bytes <= 20 max_tokens),
        // tool output should be sorted last and converted to status token.
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.retained_chunks[0].doc_id, DocId::new(2));
        assert_eq!(result.status_tokens.len(), 1);
        assert_eq!(
            result.status_tokens[0].replaced_doc_ids,
            vec![DocId::new(1)]
        );
    }

    #[test]
    fn test_compactor_truncate_strategy() {
        let budget = TokenBudget::new(10, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Truncate);

        let chunks = vec![
            make_chunk(1, "small", 0.9, false),            // 5 bytes
            make_chunk(2, "exceeding budget", 0.8, false), // 16 bytes
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.retained_chunks[0].doc_id, DocId::new(1));
        assert!(result.status_tokens.is_empty());
    }

    #[test]
    fn test_compactor_summarize_strategy_fallback() {
        let budget = TokenBudget::new(10, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Summarize);

        let chunks = vec![
            make_chunk(1, "small", 0.9, false),
            make_chunk(2, "large content for summarization", 0.8, false),
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.status_tokens.len(), 1);
        assert_eq!(
            result.status_tokens[0].replaced_doc_ids,
            vec![DocId::new(2)]
        );
    }

    #[test]
    fn test_compact_with_contextual_prefix_respects_budget() {
        use memfuse_core::{ContextChunk, DocId, TokenBudget};

        let budget = TokenBudget::new(20, 0); // 20 tokens available
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Truncate);

        // Chunk: token_count=10, prefix adds ~5 tokens → combined=15
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "content".to_string(),
            relevance: 1.0,
            token_count: 10,
            metadata: None,
            contextual_prefix: Some("1234567890123456789012".to_string()), // 22 chars → +5 tokens
        };

        // Without prefix: chunk fits (10 <= 20)
        // With prefix: chunk fits (15 <= 20)
        let result = compactor.compact(vec![chunk]);
        // combined_token_count=15 <= budget=20 → retained
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.tokens_used, 15); // combined_token_count, not raw token_count
    }
}
