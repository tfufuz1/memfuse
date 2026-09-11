// REVIEW-PASS[1/2] STATUS:PASS (TS: 2026-09-11T14:45:00Z) (SESSION: 089dd3c0) PRÜFER-KONTEXT: FRESH - Verified prompt injection safety (xml_escape, build_rag_prompt), HTTP retry policy, zero unsafe, and test suite green.

mod client;
pub mod context_prefixer;
mod embedding;
pub mod importance;
pub mod model_info;

pub use client::{
    build_rag_prompt, xml_escape, OllamaClient, OllamaConfig, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL,
};
pub use context_prefixer::{ContextPrefixConfig, ContextPrefixEngine, ContextPrefixer};
pub use embedding::OllamaEmbedder;
pub use importance::{
    parse_importance_score_response, score_importance, score_importance_batch,
    score_importance_with_calibrator, Confidence, ImportanceAssessment,
};
pub use model_info::ModelInfo;
