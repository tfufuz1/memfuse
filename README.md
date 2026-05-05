# MemFuse

**Embedded Hybrid-Search for AI Agents**

> "SQLite for Vector + Metadata + Graph search — in one crate."

## Quick Start

```rust
use memfuse_db::MemFuse;

#[tokio::main]
async fn main() -> memfuse_core::Result<()> {
    let db = MemFuse::open("./my_agent_memory").await?;

    // Insert a document with embedding + metadata
    let embedding = vec![0.1, 0.2, 0.3, 0.4];
    db.insert("doc-1", &embedding, Some(serde_json::json!({
        "topic": "rust",
        "source": "docs"
    }))).await?;

    // Semantic search
    let results = db.search(&[0.1, 0.2, 0.3, 0.4], 5).await?;
    for r in &results {
        println!("{}: score={:.3}", r.id, r.score);
    }

    // Create relationships (Phase 2: backed by CSR graph)
    db.relate("doc-1", "doc-2", "references").await?;

    // Delete
    db.delete("doc-1").await?;

    Ok(())
}
```

## Architecture

```
memfuse-db          ← User-facing API (insert, search, delete, relate)
  ├── memfuse-store ← LSM-Tree + WAL (persistent key-value storage)
  ├── memfuse-index ← HNSW + SIMD distance (vector search)
  └── memfuse-core  ← Types, traits, errors, tx-buffer
```

## Features

- **Zero Boilerplate** — String IDs, auto-commit, no configuration needed
- **HNSW Vector Search** — Approximate nearest neighbor with diversity heuristic
- **SIMD Acceleration** — portable-simd for distance computation
- **LSM-Tree Persistence** — WAL + MemTable with crash recovery
- **Transactional** — Sharded TxBuffer with orphan-reaping
- **Relationship Tracking** — `relate()` API for graph-aware retrieval

## Building

```bash
# Nightly Rust required (for portable-simd)
rustup override set nightly

cargo check --all-targets
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT OR Apache-2.0
