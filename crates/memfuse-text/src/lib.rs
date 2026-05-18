//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

// ANCHOR:INTEGRATION STATUS:FIXME AGENT:05 DATE:2026-05-22
// MISSING: Crate-level tests/ directory for integration tests (e.g. InvertedIndex + LsmStorage).

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
