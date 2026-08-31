use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_core::{Result, TextEmbeddingEngine};
use memfuse_db::MemFuse;
use memfuse_mcp::{read_line_bounded, McpServer, MAX_RPC_BYTES};
use serde_json::json;
use std::io::Cursor;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::BufReader;
use tokio::runtime::Runtime;

#[derive(Debug)]
struct BenchmarkEmbedder {
    dimension: usize,
}

#[async_trait::async_trait]
impl TextEmbeddingEngine for BenchmarkEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1f32; self.dimension])
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1f32; self.dimension]; texts.len()])
    }
}

async fn create_bench_server() -> (Arc<McpServer>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db = MemFuse::open(tmp.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();
    let dim = col.dimension();
    let embedder = Arc::new(BenchmarkEmbedder { dimension: dim });

    // Seed data
    col.insert(
        "doc-bench-1",
        &vec![0.1f32; dim],
        Some(json!({"text": "Benchmark document search query content"})),
    )
    .await
    .unwrap();

    let server = Arc::new(McpServer::with_write_permission(Arc::new(db), embedder, true).unwrap());
    (server, tmp)
}

fn bench_mcp_ops(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (server, _tmp) = rt.block_on(create_bench_server());

    // 1. Benchmark read_line_bounded latency & throughput for different payload sizes
    let mut group = c.benchmark_group("mcp_read_line_bounded");

    let sizes = [
        ("minimal_100b", 100),
        ("medium_64kb", 64 * 1024),
        ("maximal_16mb", 16 * 1024 * 1024),
    ];

    for (label, size) in sizes {
        let mut data = vec![b'a'; size - 1];
        data.push(b'\n');

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("read_line", label), &data, |b, data| {
            b.to_async(&rt).iter(|| async {
                let mut reader = BufReader::new(Cursor::new(data));
                let mut buf = String::new();
                let _ = read_line_bounded(&mut reader, &mut buf, MAX_RPC_BYTES)
                    .await
                    .unwrap();
            });
        });
    }
    group.finish();

    // 2. Benchmark handle_request processing throughput (ping & search)
    let mut group_handle = c.benchmark_group("mcp_handle_request");

    let ping_req = memfuse_mcp::protocol::JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "ping".into(),
        params: json!({}),
    };

    group_handle.bench_function("ping_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _resp = server.handle(ping_req.clone()).await;
        });
    });

    let search_req = memfuse_mcp::protocol::JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(2)),
        method: "memfuse_search".into(),
        params: json!({
            "query": "Benchmark document search",
            "collection": "default",
            "k": 10
        }),
    };

    group_handle.bench_function("search_latency_e2e", |b| {
        b.to_async(&rt).iter(|| async {
            let _resp = server.handle(search_req.clone()).await;
        });
    });

    group_handle.finish();
}

criterion_group!(benches, bench_mcp_ops);
criterion_main!(benches);
