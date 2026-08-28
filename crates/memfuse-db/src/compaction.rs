// memfuse-db/src/compaction.rs
// Context Compaction Engine (Grok Pattern)

//! Context Compaction Engine (Grok Pattern)
//!
//! Replaces stale tool outputs and long conversation histories with compact status tokens
//! to preserve the LLM context window.

use memfuse_core::{ContextChunk, DocId, Result, TokenBudget};
use memfuse_ollama::OllamaClient;

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
    /// Ursprüngliche Quell-Dokument-IDs.
    pub source_doc_ids: Vec<DocId>,
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
        let source_doc_ids: Vec<DocId> = chunks.iter().map(|c| c.doc_id).collect();
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
                        // AI-TAG[SMELL][MINOR] Async LLM-Summarization for context compaction (ID: AGT-DB-004) (TS:2026-08-25T00:00:00Z)
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

        let source_doc_ids = retained.iter().map(|c| c.doc_id).collect();
        CompactedContext {
            retained_chunks: retained,
            status_tokens,
            tokens_used,
            source_doc_ids,
        }
    }

    // AI-TAG[SMELL][MINOR][RESOLVED] Async LLM-Summarization for context compaction (ID: AGT-DB-004) (TS:2026-08-28T00:00:00Z)
    /// Consolidates multiple context chunks into a single summarized chunk using an external LLM via Ollama.
    ///
    /// Preserves strict provenance tracking in `source_doc_ids`. If the LLM call fails, the error is
    /// returned directly to the caller (no silent fallback to `StatusToken`).
    pub async fn consolidate_via_llm(
        &self,
        chunks: &[ContextChunk],
        ollama: &OllamaClient,
    ) -> Result<CompactedContext> {
        if chunks.is_empty() {
            return Ok(CompactedContext {
                retained_chunks: Vec::new(),
                status_tokens: Vec::new(),
                tokens_used: 0,
                source_doc_ids: Vec::new(),
            });
        }

        let mut source_doc_ids = Vec::with_capacity(chunks.len());
        let mut prompt_content = String::new();

        for chunk in chunks {
            source_doc_ids.push(chunk.doc_id);
            prompt_content.push_str(&format!(
                "- Chunk [DocId: {}]: {}\n",
                chunk.doc_id.0, chunk.content
            ));
        }

        let prompt = format!(
            "Fasse die folgenden Kontext-Informationen faktentreu zu einem prägnanten Überblick zusammen.\n\
             Erhalte wichtige Details und wahre den Bezug zu den ursprünglichen Dokumenten.\n\n\
             Kontext-Chunks:\n{}\n\nZusammenfassung:",
            prompt_content
        );

        let model = &ollama.config().model;
        let summary_text = ollama.generate_text(model, &prompt).await?;

        let estimated_tokens = summary_text.len(); // Simple token estimation based on length

        // Combine metadata if present
        let mut combined_metadata = serde_json::Map::new();
        combined_metadata.insert("llm_summarized".to_string(), serde_json::Value::Bool(true));
        combined_metadata.insert(
            "source_doc_count".to_string(),
            serde_json::Value::Number(chunks.len().into()),
        );

        // Generate a new DocId deterministically or using base doc_id of first chunk
        let synthesized_doc_id = chunks[0].doc_id;

        let max_relevance = chunks.iter().fold(0.0f32, |max, c| max.max(c.relevance));

        let consolidated_chunk = ContextChunk {
            doc_id: synthesized_doc_id,
            content: summary_text,
            relevance: max_relevance,
            token_count: estimated_tokens,
            metadata: Some(serde_json::Value::Object(combined_metadata)),
            contextual_prefix: None,
        };

        let tokens_used = consolidated_chunk.combined_token_count();

        Ok(CompactedContext {
            retained_chunks: vec![consolidated_chunk],
            status_tokens: Vec::new(),
            tokens_used,
            source_doc_ids,
        })
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
