pub mod docx;
pub mod email;
pub mod entities;
pub mod pdf;
pub mod pipeline;

pub use entities::SimpleEntityExtractor;
pub use pipeline::{IngestReport, IngestionPipeline};
