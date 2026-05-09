// ANCHOR:AUDIT:SAOS-021 — forbid(unsafe_code) fehlte → nachgerüstet
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//
// ANCHOR:ARCH:TEXT-001 — BM25 + Full-Text Search (Getriebe — Layer 2).
// ZIEL: Hybrid Search (WP-2.1) — kombiniert mit Vector Search + RRF (Reciprocal Rank Fusion).
//! MemFuse Text — BM25 and Inverted Index integration.

#![forbid(unsafe_code)]

pub mod bm25;
pub mod inverted;
pub mod tokenizer;

pub use bm25::Bm25Scorer;
pub use inverted::InvertedIndex;
pub use tokenizer::Tokenizer;
