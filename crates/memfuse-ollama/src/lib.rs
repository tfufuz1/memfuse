pub mod api;
mod client;
pub mod context_prefixer;
mod embedding;
pub mod importance;
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;
pub mod model_info;
pub mod prompt;

pub use api::OllamaApi;
pub use client::{
    OllamaClient, OllamaConfig, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL,
};
pub use context_prefixer::{ContextPrefixConfig, ContextPrefixEngine, ContextPrefixer};
pub use embedding::OllamaEmbedder;
pub use importance::score_importance;
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockOllamaClient;
pub use model_info::ModelInfo;
pub use prompt::{build_rag_prompt, xml_escape};
