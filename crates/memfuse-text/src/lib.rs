//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

// ANCHOR:INTEGRATION PRIO:2 STATUS:FIXME AGENT:05 DATE:2026-05-22
// FIXME: Missing dedicated tests/ directory for integration tests.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
