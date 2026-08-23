use memfuse_db::MemFuse;
use memfuse_mcp::{create_router, McpServerState};
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

    let db = MemFuse::open(&db_path).await?;
    let state = Arc::new(McpServerState { db: Arc::new(db) });
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3939").await?;
    tracing::info!("MemFuse MCP-Server läuft auf http://127.0.0.1:3939");
    axum::serve(listener, app).await?;

    Ok(())
}
