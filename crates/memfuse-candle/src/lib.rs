//! `memfuse-candle`: Native Candle GGUF ML Inferenz-Backend für MemFuse.
//!
//! # Architektur-Strategie
//!
//! **Strategie B** (mistral.rs-artiger Ansatz):
//! Dieses Crate bietet eine schnell deploybare, native Inferenz-Engine hinter den bestehenden
//! Traits (`LlmTextGenerator`, `EmbeddingProvider`, `TextEmbeddingEngine`).
//! Es kapselt Modell-Laden und Tensor-Inferenz ohne direkten Attention-Level- oder
//! RoPE-Shift-Zugriff auf KV-Cache-Ebene.
//!
//! *Hinweis*: **Strategie A** (expliziter RoPE-Shift- und KV-Cache-Bridge-Zugriff für mandantenisolierte
//! Cache-Projektionen) ist ein separates, zukünftiges Vorhaben und NICHT Gegenstand dieser Erstfassung.

pub mod embedding;
pub mod gguf_loader;
pub mod inference;
pub mod model_registry;

pub use embedding::CandleEmbedClient;
pub use inference::CandleLlmClient;
pub use model_registry::{compute_fingerprint, CandleQuantization, ModelFingerprint};
