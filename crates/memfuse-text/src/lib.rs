//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

// ANCHOR:INTEGRATION STATUS:DONE AGENT:05 DATE:2026-05-15

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
