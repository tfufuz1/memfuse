use memfuse_db::MemFuse;
use memfuse_mcp::McpServer;
use memfuse_ollama::OllamaEmbedder;
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

    let ollama_url = std::env::var("MEMFUSE_OLLAMA_URL")
        .unwrap_or_else(|_| memfuse_ollama::DEFAULT_BASE_URL.to_string());
    let embed_model = std::env::var("MEMFUSE_EMBED_MODEL")
        .unwrap_or_else(|_| memfuse_ollama::DEFAULT_EMBED_MODEL.to_string());

    let allow_write = if args.iter().any(|a| a == "--read-only") {
        false
    } else if args.iter().any(|a| a == "--allow-write") {
        true
    } else {
        memfuse_mcp::is_write_allowed_by_env()
    };

    let db = Arc::new(MemFuse::open(&db_path).await?);
    let embedder = Arc::new(OllamaEmbedder::new(&ollama_url, &embed_model));
    let server = Arc::new(McpServer::with_write_permission(db, embedder, allow_write)?);

    tracing::info!(
        db_path,
        allow_write,
        "MemFuse MCP-Server gestartet (stdio transport)"
    );
    server.run_stdio().await?;
    Ok(())
}
