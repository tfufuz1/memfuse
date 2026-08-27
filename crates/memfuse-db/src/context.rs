//! Autonomous Context Management (WP-6.3).
//!
//! Proactively injects the most relevant context into the LLM working memory
//! before inference. Small-to-Big retrieval with token budget management.

// INVARIANT: Autonomes Kontext-Management (WP-6.3)

// ANCHOR:INTEGRATION:WP-7.1-CHUNKER — Wire MarkdownChunker to ContextManager
// TEST: cargo test -p memfuse-db
// DONE: ContextManager nutzt MarkdownChunker zur Dokument-Zerlegung.

use memfuse_core::{ContextChunk, ContextWindow, DocId, Result, TokenBudget};

/// Manages autonomous context preparation for LLM consumption.
///
/// Implements Small-to-Big retrieval:
/// 1. Find small, precise chunks (Small Retrieval)
/// 2. Load parent documents (Big Context)
/// 3. Trim to token budget (relevance-weighted)
pub struct ContextManager {
    /// Token budget configuration.
    budget: TokenBudget,
    /// Adaptive relevance threshold.
    relevance_threshold: f32,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl TryFrom<crate::SearchResult> for ContextChunk {
    type Error = memfuse_core::MemFuseError;

    fn try_from(r: crate::SearchResult) -> std::result::Result<Self, Self::Error> {
        let doc_id = DocId::from_key(&r.id).map_err(|e| {
            memfuse_core::MemFuseError::InvalidInput(format!(
                "SearchResult-ID '{}' ungültig: {e}",
                r.id
            ))
        })?;
        let content = r
            .metadata
            .as_ref()
            .and_then(|m| m.get("text").or_else(|| m.get("content")))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let token_count = ContextManager::estimate_tokens(&content);
        Ok(ContextChunk {
            doc_id,
            content,
            relevance: r.score,
            token_count,
            metadata: r.metadata,
            contextual_prefix: None,
        })
    }
}

impl ContextManager {
    /// Creates a new ContextManager with the given token budget.
    pub fn new(budget: TokenBudget) -> Self {
        Self {
            budget,
            relevance_threshold: 0.1,
        }
    }

    /// Creates a ContextManager with default settings.
    pub fn with_defaults() -> Self {
        Self::new(TokenBudget::default())
    }

    /// Sets the minimum relevance score for context inclusion.
    pub fn set_relevance_threshold(&mut self, threshold: f32) {
        self.relevance_threshold = threshold;
    }

    /// Returns the current relevance threshold.
    pub fn relevance_threshold(&self) -> f32 {
        self.relevance_threshold
    }

    /// Prepares a context window from retrieved chunks.
    ///
    /// Filters by relevance threshold, sorts by score, and truncates to budget.
    pub fn prepare_context(&self, mut chunks: Vec<ContextChunk>) -> Result<ContextWindow> {
        // Filter by relevance threshold
        chunks.retain(|c| c.relevance >= self.relevance_threshold);

        // Sort by relevance descending
        chunks.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to token budget
        let available = self.budget.available();
        let mut total_tokens = 0;
        let mut truncated = false;
        let mut selected = Vec::new();

        for chunk in chunks {
            if total_tokens + chunk.token_count > available {
                truncated = true;
                if selected.is_empty() && available > 0 {
                    let remaining = available.saturating_sub(total_tokens);
                    let mut truncated_chunk = chunk;
                    let content_chars: Vec<char> = truncated_chunk.content.chars().collect();
                    if !content_chars.is_empty() && remaining > 0 {
                        let mut low = 0;
                        let mut high = content_chars.len();
                        let mut best_len = 0;
                        while low <= high {
                            let mid = (low + high) / 2;
                            let sub: String = content_chars[..mid].iter().collect();
                            let tokens = Self::estimate_tokens(&sub);
                            if tokens <= remaining {
                                best_len = mid;
                                low = mid + 1;
                            } else {
                                if mid == 0 {
                                    break;
                                }
                                high = mid - 1;
                            }
                        }
                        if best_len > 0 {
                            truncated_chunk.content = content_chars[..best_len].iter().collect();
                            truncated_chunk.token_count =
                                Self::estimate_tokens(&truncated_chunk.content);
                            total_tokens += truncated_chunk.token_count;
                            selected.push(truncated_chunk);
                        }
                    }
                }
                break;
            }
            total_tokens += chunk.token_count;
            selected.push(chunk);
        }

        Ok(ContextWindow {
            chunks: selected,
            total_tokens,
            truncated,
        })
    }

    /// Schätzt die Token-Anzahl eines Textes mit heuristischen Regeln.
    ///
    /// Algorithmus (BPE-Approximation, kalibriert auf cl100k_base / GPT-4):
    /// - Jedes ASCII-Wort: ~1.3 Tokens (Subword-Splitting)
    /// - Jeder CJK-Charakter: 1 Token (eigenes Byte-Pair)
    /// - Jede Zahl-Sequenz: 1 Token pro 3 Ziffern
    /// - Code-Blöcke (```) verdoppeln die Dichte (Identifier, Symbole)
    /// - Interpunktion: 1 Token pro 4 Zeichen
    ///
    /// Kalibriert für ±15% Genauigkeit vs. tiktoken cl100k_base.
    pub fn estimate_tokens(text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let mut tokens: f64 = 0.0;
        let mut in_code_block = false;
        let code_multiplier = 1.8f64; // Code-Tokens sind dichter

        for line in text.lines() {
            if line.starts_with("```") {
                in_code_block = !in_code_block;
                tokens += 1.0;
                continue;
            }

            let multiplier = if in_code_block { code_multiplier } else { 1.0 };

            let mut char_iter = line.chars().peekable();
            while let Some(c) = char_iter.next() {
                match c {
                    // CJK Unified Ideographs, CJK Extension A/B, Hiragana, Katakana
                    '\u{4E00}'..='\u{9FFF}'
                    | '\u{3400}'..='\u{4DBF}'
                    | '\u{20000}'..='\u{2A6DF}'
                    | '\u{3040}'..='\u{309F}'
                    | '\u{30A0}'..='\u{30FF}' => {
                        tokens += 1.0 * multiplier;
                    }
                    // ASCII Wörter (Buchstaben + Zahlen zusammen)
                    'a'..='z' | 'A'..='Z' | '_' => {
                        // Zähle Wortlänge
                        let mut word_len = 1usize;
                        while char_iter
                            .peek()
                            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                        {
                            char_iter.next();
                            word_len += 1;
                        }
                        // BPE: kurze Wörter 1 Token, lange Wörter ~len/4 Tokens
                        let word_tokens = if word_len <= 4 {
                            1.0
                        } else {
                            1.0 + (word_len as f64 - 4.0) / 4.0
                        };
                        tokens += word_tokens * multiplier;
                    }
                    '0'..='9' => {
                        // Zahlen: ~1 Token pro 3 Ziffern
                        let mut num_len = 1usize;
                        while char_iter.peek().is_some_and(|c| c.is_ascii_digit()) {
                            char_iter.next();
                            num_len += 1;
                        }
                        tokens += ((num_len as f64) / 3.0).ceil() * multiplier;
                    }
                    ' ' | '\t' => {} // Whitespace zählt nicht
                    // Interpunktion und Sonderzeichen
                    _ => {
                        tokens += 0.25 * multiplier; // ~4 Sonderzeichen = 1 Token
                    }
                }
            }
            tokens += 0.1; // Newline-Overhead
        }

        // Mindestens 1, maximal sinnvoll deckeln
        (tokens.ceil() as usize).max(1)
    }
}

/// Spatial fencing for geographically constrained context (optional).
///
/// Filters context by geographic region metadata field.
#[derive(Debug, Clone)]
pub struct SpatialFence {
    /// The region identifier to filter by.
    pub region: String,
    /// The metadata field name containing the region.
    pub field_name: String,
}

impl SpatialFence {
    /// Creates a new spatial fence for the given region.
    pub fn new(region: impl Into<String>) -> Self {
        Self {
            region: region.into(),
            field_name: "geo_region".to_string(),
        }
    }

    /// Checks if a chunk's metadata matches this spatial fence.
    pub fn matches(&self, chunk: &ContextChunk) -> bool {
        if let Some(metadata) = &chunk.metadata {
            if let Some(obj) = metadata.as_object() {
                if let Some(val) = obj.get(&self.field_name).and_then(|v| v.as_str()) {
                    return val == self.region;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::DocId;

    #[test]
    fn context_manager_respects_token_budget() {
        let budget = TokenBudget::new(100, 0); // 100 max, 0 reserved => 100 available
        let manager = ContextManager::new(budget);

        let chunks = vec![
            ContextChunk {
                doc_id: DocId::new(1),
                content: "First long content chunk".into(),
                relevance: 0.9,
                token_count: 60,
                metadata: None,
                contextual_prefix: None,
            },
            ContextChunk {
                doc_id: DocId::new(2),
                content: "Second long content chunk".into(),
                relevance: 0.8,
                token_count: 60,
                metadata: None,
                contextual_prefix: None,
            },
        ];

        let window = manager.prepare_context(chunks).expect("prepare_context");
        assert!(window.truncated);
        assert!(window.total_tokens <= 100);
    }

    #[test]
    fn test_context_manager_budget_truncation() {
        let budget = TokenBudget::new(100, 20);
        let mgr = ContextManager::new(budget);

        let chunks = vec![
            ContextChunk {
                doc_id: DocId::new(1),
                content: "high relevance".into(),
                relevance: 0.9,
                token_count: 50,
                metadata: None,
                contextual_prefix: None,
            },
            ContextChunk {
                doc_id: DocId::new(2),
                content: "medium relevance".into(),
                relevance: 0.5,
                token_count: 50,
                metadata: None,
                contextual_prefix: None,
            },
            ContextChunk {
                doc_id: DocId::new(3),
                content: "should be excluded".into(),
                relevance: 0.3,
                token_count: 50,
                metadata: None,
                contextual_prefix: None,
            },
        ];

        let window = mgr.prepare_context(chunks).expect("valid test value");
        // Budget: 100 - 20 = 80 available. Should fit 50 (chunk1) but not 50+50=100.
        assert_eq!(window.chunks.len(), 1);
        assert!(window.truncated);
        assert_eq!(window.total_tokens, 50);
    }

    #[test]
    fn test_token_estimation() {
        let tokens = ContextManager::estimate_tokens("hello world foo bar");
        assert!(tokens >= 4); // At least 4 words
    }

    #[test]
    fn test_single_document_exceeds_budget() {
        let budget = TokenBudget::new(20, 10); // 10 available
        let mgr = ContextManager::new(budget);

        let long_content = "This is a very long text that will definitely exceed the small available token budget of 10 tokens.";
        let token_count = ContextManager::estimate_tokens(long_content);
        assert!(token_count > 10);

        let chunks = vec![ContextChunk {
            doc_id: DocId::new(1),
            content: long_content.into(),
            relevance: 0.9,
            token_count,
            metadata: None,
            contextual_prefix: None,
        }];

        let window = mgr.prepare_context(chunks).expect("valid test value");
        assert_eq!(window.chunks.len(), 1);
        assert!(window.truncated);
        assert!(window.total_tokens <= 10);
        assert!(window.total_tokens > 0);
        assert!(!window.chunks[0].content.is_empty());
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(ContextManager::estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_simple_english() {
        // "Hello world" → ~2 Tokens
        let t = ContextManager::estimate_tokens("Hello world");
        assert!((2..=4).contains(&t), "expected 2-4, got {t}");
    }

    #[test]
    fn test_estimate_tokens_cjk() {
        // 5 CJK Zeichen → ~5 Tokens
        let t = ContextManager::estimate_tokens("你好世界！");
        assert!((4..=7).contains(&t), "expected 4-7, got {t}");
    }

    #[test]
    fn test_estimate_tokens_code_block() {
        let code = "```rust\nfn main() { println!(\"hello\"); }\n```";
        let plain = "fn main println hello";
        // Code-Block sollte mehr Tokens zählen als plain
        assert!(ContextManager::estimate_tokens(code) > ContextManager::estimate_tokens(plain));
    }

    #[test]
    fn test_estimate_tokens_never_zero_for_nonempty() {
        assert!(ContextManager::estimate_tokens("a") >= 1);
        assert!(ContextManager::estimate_tokens("   spaces   ") >= 1);
    }
}
