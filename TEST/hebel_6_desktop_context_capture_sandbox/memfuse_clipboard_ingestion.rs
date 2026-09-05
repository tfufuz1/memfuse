//! MemFuse Auto-Context Ingestion Service
//!
//! Bridges TextForge's source-app-aware clipboard monitoring into MemFuse collections.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardCapturedChunk {
    pub text: String,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
    pub timestamp: u64,
}

pub struct AutoContextIngestService {
    is_enabled: Arc<Mutex<bool>>,
    target_collection: String,
}

impl AutoContextIngestService {
    pub fn new(target_collection: impl Into<String>) -> Self {
        Self {
            is_enabled: Arc::new(Mutex::new(true)),
            target_collection: target_collection.into(),
        }
    }

    /// Handles a new clipboard event captured from Wayland/X11.
    pub async fn on_clipboard_text_captured(
        &self,
        text: String,
        source_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<(), String> {
        let enabled = *self.is_enabled.lock().await;
        if !enabled || text.trim().is_empty() {
            return Ok(());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // In MemFuse: call collection.insert(id, text, metadata)
        tracing::info!(
            target: "memfuse::auto_context",
            app = ?source_app,
            title = ?window_title,
            length = text.len(),
            collection = %self.target_collection,
            "Auto-ingesting clipboard snippet into MemFuse"
        );

        Ok(())
    }
}
