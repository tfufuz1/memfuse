// FILE-CONTEXT
// ZWECK: Markdown-basierte semantische Textzerlegung (WP-7.1) und UTF-8-sicheres Chunken.
// INVARIANTEN: Aufteilung respektiert Überschriften-Hierarchien; UTF-8-Grenzen bleiben gewahrt.
// NICHT-OFFENSICHTLICH: Brotbrösel (Breadcrumbs) werden als Metadaten an Chunks angehängt.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

//! Markdown Semantic Chunker (WP-7.1)
//!
//! Deterministically splits Markdown documents into ContextChunks based on
//! heading hierarchy. Merges small sections and attaches heading paths as
//! metadata breadcrumbs.

use crate::context::ContextManager;
use memfuse_core::{
    ContextChunk, DocId};
use serde_json::json;

/// Approximate character count per token (BPE ratio consistent with `ContextManager::estimate_tokens`).
const CHARS_PER_TOKEN: usize = 4;

/// Configuration for the Markdown chunker.
pub struct ChunkerConfig {
    /// Maximum tokens per chunk (soft limit, hard limit is this * 1.2).
    pub max_tokens: usize,
    /// Minimum tokens per chunk (merge threshold).
    pub min_tokens: usize,
    /// Include heading breadcrumb as metadata in resulting chunks.
    pub include_breadcrumbs: bool,
    /// Heading levels to split on (1 = H1, 2 = H2, ...).
    pub split_levels: Vec<u8>,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            min_tokens: 50,
            include_breadcrumbs: true,
            split_levels: vec![1, 2, 3],
        }
    }
}

/// A deterministic chunker for markdown files.
pub struct MarkdownChunker {
    config: ChunkerConfig,
}

#[derive(Debug, Clone)]
struct RawSection {
    lines: Vec<String>,
    breadcrumb: String,
    heading_level: u8,
    source_line: usize,
    tokens: usize,
}

impl MarkdownChunker {
    /// Creates a new MarkdownChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Creates a new MarkdownChunker with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ChunkerConfig::default())
    }

    /// Chunks a Markdown document into semantically coherent pieces.
    pub fn chunk(&self, doc_id: DocId, markdown: &str) -> Vec<ContextChunk> {
        if markdown.is_empty() {
            return Vec::new();
        }
        let hard_limit = ((self.config.max_tokens as f64 * 1.2) as usize).max(1);
        let mut raw_sections = Vec::new();
        let mut current_lines = Vec::new();
        let mut current_breadcrumb = String::new();
        let mut current_heading_level = 0;
        let mut current_source_line = 1;

        let mut heading_stack: Vec<(u8, String)> = Vec::new();

        for (i, line) in markdown.lines().enumerate() {
            let line_num = i + 1;

            let mut is_heading = false;
            let mut h_level = 0;
            if line.starts_with('#') {
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 && parts[0].chars().all(|c| c == '#') {
                    h_level = parts[0].len() as u8;
                    if self.config.split_levels.contains(&h_level) {
                        is_heading = true;
                    }
                }
            }

            if is_heading {
                if !current_lines.is_empty() {
                    let content = current_lines.join("\n");
                    let tokens = ContextManager::estimate_tokens(&content);
                    raw_sections.push(RawSection {
                        lines: std::mem::take(&mut current_lines),
                        breadcrumb: current_breadcrumb.clone(),
                        heading_level: current_heading_level,
                        source_line: current_source_line,
                        tokens,
                    });
                }

                heading_stack.retain(|(lvl, _)| *lvl < h_level);
                // Simplify heading display for breadcrumb
                let heading_text = line.trim_start_matches('#').trim();
                let breadcrumb_part = format!("{} {}", parts_first(line, h_level), heading_text);
                heading_stack.push((h_level, breadcrumb_part));

                current_breadcrumb = heading_stack
                    .iter()
                    .map(|(_, t)| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" > ");
                current_heading_level = h_level;
                current_source_line = line_num;
                current_lines.push(line.to_string());
            } else {
                current_lines.push(line.to_string());
            }
        }

        if !current_lines.is_empty() {
            let content = current_lines.join("\n");
            let tokens = ContextManager::estimate_tokens(&content);
            raw_sections.push(RawSection {
                lines: current_lines,
                breadcrumb: current_breadcrumb,
                heading_level: current_heading_level,
                source_line: current_source_line,
                tokens,
            });
        }

        // Enforce hard limits by splitting large sections
        let mut limited_sections = Vec::new();
        let window_chars = hard_limit * CHARS_PER_TOKEN;
        let overlap_chars = window_chars / 5; // ~20% overlap

        for sec in raw_sections {
            if sec.tokens > hard_limit {
                // Approximate paragraph splitting
                let content = sec.lines.join("\n");
                let paragraphs: Vec<&str> = content.split("\n\n").collect();
                let mut current_p_lines = Vec::new();
                let mut current_p_tokens = 0;

                let push_paragraph_section = |lines: Vec<String>, target: &mut Vec<RawSection>| {
                    let p_content = lines.join("\n");
                    let actual_tokens = ContextManager::estimate_tokens(&p_content);
                    if actual_tokens > hard_limit {
                        // Fallback for oversized paragraphs/sections: split into sliding token windows with overlap
                        let windows =
                            chunk_text_with_overlap(&p_content, window_chars, overlap_chars);
                        for w in windows {
                            let w_tokens = ContextManager::estimate_tokens(w);
                            target.push(RawSection {
                                lines: vec![w.to_string()],
                                breadcrumb: sec.breadcrumb.clone(),
                                heading_level: sec.heading_level,
                                source_line: sec.source_line,
                                tokens: w_tokens,
                            });
                        }
                    } else {
                        target.push(RawSection {
                            lines,
                            breadcrumb: sec.breadcrumb.clone(),
                            heading_level: sec.heading_level,
                            source_line: sec.source_line,
                            tokens: actual_tokens,
                        });
                    }
                };

                for p in paragraphs {
                    let p_tokens = ContextManager::estimate_tokens(p);
                    // Add back the double newline logic
                    let p_text = if current_p_lines.is_empty() {
                        p.to_string()
                    } else {
                        format!("\n\n{}", p)
                    };

                    if current_p_tokens + p_tokens > hard_limit && !current_p_lines.is_empty() {
                        let lines = std::mem::take(&mut current_p_lines);
                        push_paragraph_section(lines, &mut limited_sections);
                        current_p_tokens = 0;
                        current_p_lines.push(p.to_string());
                        current_p_tokens += p_tokens;
                    } else {
                        current_p_lines.push(p_text);
                        current_p_tokens += p_tokens;
                    }
                }
                if !current_p_lines.is_empty() {
                    let lines = std::mem::take(&mut current_p_lines);
                    push_paragraph_section(lines, &mut limited_sections);
                }
            } else {
                limited_sections.push(sec);
            }
        }

        // Merge small sections
        let mut final_sections: Vec<RawSection> = Vec::new();
        for sec in limited_sections {
            if let Some(last) = final_sections.last_mut() {
                if (last.tokens < self.config.min_tokens || sec.tokens < self.config.min_tokens)
                    && (last.tokens + sec.tokens) <= hard_limit
                {
                    // Prepend newline before pushing the extra lines if needed, wait, lines themselves might not have newlines
                    // If we just join lines and split later, it's safer. Let's just extend.
                    last.lines.push("".to_string()); // empty line for separation?
                    last.lines.extend(sec.lines);
                    last.tokens += sec.tokens; // approximation might drift slightly, but safe enough
                    continue;
                }
            }
            final_sections.push(sec);
        }

        final_sections
            .into_iter()
            .map(|sec| {
                let metadata = if self.config.include_breadcrumbs {
                    Some(json!({
                        "breadcrumb": sec.breadcrumb,
                        "heading_level": sec.heading_level,
                        "source_line": sec.source_line
                    }))
                } else {
                    None
                };

                let content = sec.lines.join("\n");
                let token_count = ContextManager::estimate_tokens(&content);

                ContextChunk {
                    doc_id,
                    content,
                    relevance: 1.0,
                    token_count,
                    metadata,
                    contextual_prefix: None,
                    links: Vec::new(),
                }
            })
            .collect()
    }
}

fn parts_first(_line: &str, h_level: u8) -> String {
    let mut s = String::new();
    for _ in 0..h_level {
        s.push('#');
    }
    s
}

/// Splits a text string into overlapping chunks of at most `window_chars` Unicode characters (code points),
/// safely respecting UTF-8 character boundaries using `str::char_indices()`.
pub fn chunk_text_with_overlap(text: &str, window_chars: usize, overlap_chars: usize) -> Vec<&str> {
    if text.is_empty() || window_chars == 0 {
        return Vec::new();
    }

    let indices: Vec<usize> = text
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(text.len()))
        .collect();

    let total_chars = indices.len() - 1;
    let overlap = overlap_chars.min(window_chars.saturating_sub(1));
    let step = window_chars.saturating_sub(overlap).max(1);

    let mut chunks = Vec::new();
    let mut start_char = 0;

    while start_char < total_chars {
        let end_char = (start_char + window_chars).min(total_chars);
        let start_byte = indices[start_char];
        let end_byte = indices[end_char];

        chunks.push(&text[start_byte..end_byte]);

        if end_char == total_chars {
            break;
        }

        start_char += step;
    }

    chunks
}

/// Splits a text string into chunks of at most `chunk_size` Unicode characters (code points),
/// safely respecting UTF-8 character boundaries using `str::char_indices()`.
pub fn chunk_text(text: &str, chunk_size: usize) -> Vec<&str> {
    if text.is_empty() || chunk_size == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut char_count = 0;
    let mut start_byte = 0;

    for (byte_idx, _ch) in text.char_indices() {
        if char_count == chunk_size {
            chunks.push(&text[start_byte..byte_idx]);
            start_byte = byte_idx;
            char_count = 0;
        }
        char_count += 1;
    }

    if start_byte < text.len() {
        chunks.push(&text[start_byte..]);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_unicode_german_umlauts() {
        // German string with umlauts (ä, ö, ü, ß) and unicode characters.
        // Total character count = 100 unicode chars (50 chars * 2).
        let text = "Äpfel, Öle, Übermut und Straße sind wunderschön!! ".repeat(2);
        let char_count = text.chars().count();
        assert!(
            char_count >= 100,
            "Test string must have at least 100 chars, got {}",
            char_count
        );

        let chunks = chunk_text(&text, 30);
        assert!(!chunks.is_empty());

        for chunk in &chunks {
            // Check that chunk is valid UTF-8 (slice indexing would panic otherwise)
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
            assert!(chunk.chars().count() <= 30);
        }

        let reassembled = chunks.join("");
        assert_eq!(reassembled, text);
    }

    #[test]
    fn chunker_empty_input() {
        let chunks = MarkdownChunker::with_defaults().chunk(DocId::new(1), "");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunker_single_chunk_no_split() {
        let text = "Short text";
        let chunks = MarkdownChunker::with_defaults().chunk(DocId::new(1), text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, text);
    }

    #[test]
    fn chunker_respects_max_chunk_size() {
        let text = (0..50)
            .map(|_| "word ".repeat(20))
            .collect::<Vec<_>>()
            .join("\n\n");
        let config = ChunkerConfig {
            max_tokens: 100,
            min_tokens: 10,
            ..Default::default()
        };
        let chunks = MarkdownChunker::new(config).chunk(DocId::new(1), &text);
        let limit = (100.0 * 1.2) as usize;
        for chunk in &chunks {
            assert!(
                chunk.token_count <= limit,
                "Chunk too large: {}",
                chunk.token_count
            );
        }
    }

    #[test]
    fn test_chunk_by_headings() {
        let markdown = "# Title\nSome intro.\n## Section 1\nContent 1\n### Sub 1\nSub content";
        let doc_id = DocId::new(1);

        let config = ChunkerConfig {
            min_tokens: 0,
            ..Default::default()
        };
        let chunker = MarkdownChunker::new(config);
        let chunks = chunker.chunk(doc_id, markdown);

        assert_eq!(chunks.len(), 3);

        let m0 = chunks[0].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(m0["breadcrumb"], "# Title");
        assert_eq!(m0["heading_level"], 1);

        let m1 = chunks[1].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(m1["breadcrumb"], "# Title > ## Section 1");
        assert_eq!(m1["heading_level"], 2);

        let m2 = chunks[2].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(m2["breadcrumb"], "# Title > ## Section 1 > ### Sub 1");
        assert_eq!(m2["heading_level"], 3);
    }

    #[test]
    fn test_merge_small_sections() {
        let config = ChunkerConfig {
            min_tokens: 50,
            ..Default::default()
        };
        let chunker = MarkdownChunker::new(config);
        let markdown = "# S1\na\n# S2\nb\n# S3\nc\n# S4\nd\n# S5\ne";
        let doc_id = DocId::new(2);
        let chunks = chunker.chunk(doc_id, markdown);

        // 5 small sections of 1 token each. They should merge into 1 chunk (since total < 50)
        assert_eq!(chunks.len(), 1);

        // The merged chunk inherits the breadcrumb of the first sub-section.
        let m = chunks[0].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(m["breadcrumb"], "# S1");
        assert!(chunks[0].content.contains("e"));
    }

    #[test]
    fn test_no_content_loss() {
        let chunker = MarkdownChunker::with_defaults();
        let markdown = "# Title\n\nSome text\n\n## Subtitle\nMore text.";
        let doc_id = DocId::new(3);
        let chunks = chunker.chunk(doc_id, markdown);

        let concatenated: String = chunks
            .iter()
            .map(|c| c.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        // Normalizing spaces exactly is tricky depending on how splits happen, but let's just check if all words exist
        let orig_words: Vec<_> = markdown.split_whitespace().collect();
        let concat_words: Vec<_> = concatenated.split_whitespace().collect();
        assert_eq!(orig_words, concat_words);
    }

    #[test]
    fn test_real_document() {
        let chunker = MarkdownChunker::with_defaults();
        let doc_id = DocId::new(4);

        // Use a dummy large doc to simulate AGENTS.md
        let mut markdown = String::new();
        for i in 0..100 {
            markdown.push_str(&format!("## Heading {}\n", i));
            for j in 0..20 {
                markdown.push_str(&format!("This is line {} in paragraph {}.\n", j, i));
            }
        }

        // The hard limit is 512 * 1.2 = 614 tokens
        let limit = (chunker.config.max_tokens as f64 * 1.2) as usize;

        let chunks = chunker.chunk(doc_id, &markdown);
        assert!(!chunks.is_empty());
        for chunk in chunks {
            assert!(
                chunk.token_count <= limit,
                "Chunk token count {} exceeds limit {}",
                chunk.token_count,
                limit
            );
        }
    }

    #[test]
    fn test_chunk_text_zero_chunk_size() {
        assert!(chunk_text("hello world", 0).is_empty());
    }

    #[test]
    fn test_chunk_text_empty_string_returns_empty() {
        assert!(chunk_text("", 10).is_empty());
    }

    #[test]
    fn test_chunk_text_emoji_multibyte_boundary() {
        let text = "🦀🚀🔥⭐🎉";
        let chunks = chunk_text(text, 2);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "🦀🚀");
        assert_eq!(chunks[1], "🔥⭐");
        assert_eq!(chunks[2], "🎉");
    }

    #[test]
    fn test_chunk_text_exact_chunk_size_multiple() {
        let text = "abcdefghij"; // 10 chars
        let chunks = chunk_text(text, 5);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "abcde");
        assert_eq!(chunks[1], "fghij");
    }

    #[test]
    fn test_chunker_zero_max_tokens_handled_safely() {
        let config = ChunkerConfig {
            max_tokens: 0,
            ..Default::default()
        };
        let chunker = MarkdownChunker::new(config);
        let chunks = chunker.chunk(DocId::new(10), "Line 1\nLine 2");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunker_single_line_no_headings() {
        let chunker = MarkdownChunker::with_defaults();
        let chunks = chunker.chunk(DocId::new(5), "Plain single line document.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "Plain single line document.");
        let meta = chunks[0].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(meta["breadcrumb"], "");
        assert_eq!(meta["heading_level"], 0);
    }

    #[test]
    fn test_chunker_unsplit_heading_level_ignored() {
        let config = ChunkerConfig {
            split_levels: vec![1, 2], // H3 (###) ignored
            min_tokens: 0,
            ..Default::default()
        };
        let chunker = MarkdownChunker::new(config);
        let markdown = "# Level 1\nText 1\n### Level 3\nText 3";
        let chunks = chunker.chunk(DocId::new(6), markdown);
        // Level 3 heading should NOT split into a new section
        assert_eq!(chunks.len(), 1);
        let meta = chunks[0].metadata.as_ref().unwrap(); // unwrap
        assert_eq!(meta["breadcrumb"], "# Level 1");
    }

    #[test]
    fn chunker_handles_headingless_single_paragraph_document() {
        // Construct continuous plaintext of ~3000 words without '#' headings or '\n\n' breaks
        let line = "This is a continuous sentence representing extracted PDF text with single line breaks.\n";
        let text = line.repeat(250); // ~2500 words, >> hard_limit tokens
        assert!(!text.contains("#"));
        assert!(!text.contains("\n\n"));

        let chunker = MarkdownChunker::with_defaults();
        let hard_limit = (chunker.config.max_tokens as f64 * 1.2) as usize;
        let chunks = chunker.chunk(DocId::new(100), &text);

        assert!(
            chunks.len() > 1,
            "Document should be split into multiple chunks, got {}",
            chunks.len()
        );

        for chunk in &chunks {
            assert!(
                chunk.token_count <= hard_limit,
                "Chunk token count {} exceeds hard limit {}",
                chunk.token_count,
                hard_limit
            );
        }
    }

    #[test]
    fn chunk_text_with_overlap_produces_overlapping_windows() {
        let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"; // 36 chars
        let window_chars = 10;
        let overlap_chars = 3;

        let windows = chunk_text_with_overlap(text, window_chars, overlap_chars);
        assert!(!windows.is_empty());

        // Check overlap between consecutive windows
        for i in 0..windows.len() - 1 {
            let w1 = windows[i];
            let w2 = windows[i + 1];
            let suffix_w1 = &w1[w1.len() - overlap_chars..];
            let prefix_w2 = &w2[..overlap_chars];
            assert_eq!(
                suffix_w1,
                prefix_w2,
                "Windows {} and {} do not overlap correctly",
                i,
                i + 1
            );
        }

        // Verify first window starts at index 0 and last window finishes at text end
        assert_eq!(
            windows.first().unwrap().chars().take(3).collect::<String>(),
            "ABC"
        );
        assert_eq!(
            windows
                .last()
                .unwrap()
                .chars()
                .rev()
                .take(3)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>(),
            "789"
        );
    }

    #[test]
    fn chunk_text_with_overlap_respects_utf8_boundaries() {
        let text = "Äpfel, Öle, Übermut und Straße sind wunderschön!! ".repeat(5);
        let windows = chunk_text_with_overlap(&text, 25, 5);
        assert!(!windows.is_empty());

        for window in &windows {
            assert!(std::str::from_utf8(window.as_bytes()).is_ok());
            assert!(window.chars().count() <= 25);
        }
    }
}
