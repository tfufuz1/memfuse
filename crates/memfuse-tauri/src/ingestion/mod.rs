pub mod docx;
pub mod email;
pub mod pdf;
pub mod pipeline;

pub use pipeline::{EmbeddingProvider, IngestReport, IngestionPipeline};
