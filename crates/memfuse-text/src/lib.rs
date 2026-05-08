// ANCHOR:AUDIT:SAOS-021 — forbid(unsafe_code) fehlte → nachgerüstet
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//! MemFuse Text — BM25 and Inverted Index integration.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;

pub use bm25::Bm25Scorer;
pub use inverted::InvertedIndex;
