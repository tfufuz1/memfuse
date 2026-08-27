mod client;
pub mod context_prefixer;
mod embedding;
pub mod importance;
pub mod model_info;

pub use client::{OllamaClient, OllamaConfig, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL};
pub use context_prefixer::{ContextPrefixConfig, ContextPrefixEngine, ContextPrefixer};
pub use embedding::OllamaEmbedder;
pub use importance::score_importance;
pub use model_info::ModelInfo;
