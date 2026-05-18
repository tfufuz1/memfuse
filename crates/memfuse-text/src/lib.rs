//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
