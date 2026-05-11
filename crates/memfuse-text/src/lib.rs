//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:05 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
