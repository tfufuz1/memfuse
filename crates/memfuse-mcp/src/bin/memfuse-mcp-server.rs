use memfuse_db::MemFuse;
use memfuse_mcp::{create_router, McpServerState};
use memfuse_ollama::OllamaEmbedder;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

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

    let db = MemFuse::open(&db_path).await?;
    let embedder = Arc::new(OllamaEmbedder::new(&ollama_url, &embed_model));
    let state = Arc::new(McpServerState::with_embedder(Arc::new(db), embedder));
    let app = create_router(state);

    let bind_addr = std::env::var("MEMFUSE_MCP_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3939".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("MemFuse MCP-Server läuft auf http://{bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
