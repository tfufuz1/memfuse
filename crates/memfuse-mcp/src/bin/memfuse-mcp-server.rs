use memfuse_db::MemFuse;
use memfuse_mcp::{EmbeddingConfig, McpServer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // MCP-Protokoll: stdout ist für JSON-RPC reserviert — Logs ausschließlich nach stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .iter()
        .position(|a| a == "--db-path")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "./memfuse_data".to_string());

    let mut config = EmbeddingConfig::from_env();

    if let Some(i) = args.iter().position(|a| a == "--provider") {
        if let Some(val) = args.get(i + 1) {
            config.provider = val.clone();
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--ollama-url") {
        if let Some(val) = args.get(i + 1) {
            config.ollama_url = val.clone();
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--embed-model") {
        if let Some(val) = args.get(i + 1) {
            config.embed_model = val.clone();
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--onnx-model-path") {
        if let Some(val) = args.get(i + 1) {
            config.onnx_model_path = Some(std::path::PathBuf::from(val));
        }
    }

    let allow_write = if args.iter().any(|a| a == "--read-only") {
        false
    } else if args.iter().any(|a| a == "--allow-write") {
        true
    } else {
        memfuse_mcp::is_write_allowed_by_env()
    };

    let db = Arc::new(MemFuse::open(&db_path).await?);
    let embedder = config.build_provider()?;
    let server = Arc::new(McpServer::with_write_permission(db, embedder, allow_write)?);

    tracing::info!(
        db_path,
        allow_write,
        provider = %config.provider,
        "MemFuse MCP-Server gestartet (stdio transport)"
    );
    server.run_stdio().await?;
    Ok(())
}
