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
    parse_importance_score_response, record_importance_outcome, score_importance,
    score_importance_with_calibrator, Confidence, ImportanceAssessment,
};
pub use model_info::ModelInfo;
