//! # MemFuse Text Processing
//!
//! Provides tokenization, inverted indexing on LSM, and BM25 scoring.

// ANCHOR:INTEGRATION STATUS:DONE AGENT:05 DATE:2026-05-15
// ANCHOR:PERF:LATENCY-003
// AGENT:09 DATE:2026-05-22 STATUS:DONE
// VORHER: ~24.6 µs (upsert) → NACHHER: ~18.6 µs (~24% gain)
// BOTTLENECK: String-Allokationen (format!) in Key-Generierung
// OPTIMIERUNG: itoa + Byte-Buffer für Key-Konstruktion

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;
