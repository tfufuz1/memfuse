mod client;
mod embedding;
mod model_info;

pub use client::{OllamaClient, DEFAULT_BASE_URL, DEFAULT_EMBED_MODEL};
pub use embedding::OllamaEmbedder;
pub use model_info::ModelInfo;
