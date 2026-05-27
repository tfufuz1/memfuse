# MemFuse

**The Embedded Edge-AI Vector Database (Sovereign Core)**

> "The ultimate 100% safe-Rust, zero-panic, air-gapped embedded database for AI Agents. No external C/C++ dependencies."

## Quick Start

```python
# MemFuse provides a zero-setup Python experience
import memfuse
import numpy as np

# Open the database
db = memfuse.open("./my_agent_memory", dimension=1536)

# Create or get a collection
col = db.collection("memories")

# Insert a document with embedding and metadata
v = np.random.rand(1536).astype(np.float32)
col.insert("doc1", v, metadata={"topic": "AI", "tags": ["rust", "search"]})

# Semantic search
results = col.search(v, k=5)
for res in results:
    print(f"ID: {res.id}, Score: {res.score}")

# Hybrid Search (BM25 + Vector)
hybrid_results = col.hybrid_search("AI search", v, k=5)
```

## Architecture: The "Sovereign Core"

MemFuse provides absolute computational verifyability and data sovereignty for edge-deployments. It is designed around the **Sovereign Core Doctrine** (Zero-panic, 100% Async Safe Rust, zero external database dependencies like Arrow/C).

1. **Kernel & Sub-Engines (Level 0 & 1)**
```
memfuse-core   ← Core Types, Traits, Errors, TxBuffer
memfuse-store  ← Mathematically verified LSM-Tree Persistence (WAL)
memfuse-index  ← HNSW + SIMD Vector Search (OOM resilient)
memfuse-text   ← Inverted Index + BM25 Scoring
memfuse-crypto ← AES-GCM Encryption-at-Rest
memfuse-graph  ← CSR-Graph Entity Relations
```
*Provides dense, sparse, and semantic retrieval at extreme edge latency without network overhead.*

2. **Orchestration & Facade (Level 2 & 3)**
```
memfuse-db     ← Orchestrator for Namespaces & Hybrid Fusion
memfuse-py     ← PyO3 Zero-Cost bindings (`pip install memfuse`)
```

> **Note:** Development on AgentOS middlewares (e.g., WASM sandboxes, Workflow Engines) has been STRATEGICALLY FROZEN. MemFuse focuses exclusively on being the absolute best embedded edge vector engine.

## The Jules Squad: 13 Autonomous Agents

MemFuse is developed using a revolutionary **Multi-Agent Orchestration** system. 13 autonomous Jules agents work in a staggered 24-hour cycle to provide 24/7 development and maintenance without human intervention.

- **Infinite Free-Tier Mastery**: Orchestration of 13 Google Jules accounts to bypass rate limits.
- **Triple-Test-Gate**: Every PR is validated 3 times and checked for `Zero-Panic` invariants before auto-merging.
- **Proactive Scaling**: Dynamic dispatcher triggers the next agent in the queue immediately after a successful merge.
- **AI-Architect Supervision**: Gemini-CLI automated architectural reviews on every contribution.

```bash
# Monitor the squad in real-time
bash .agent/scripts/jules-dashboard.sh
```

## Features

- **Zero Boilerplate** — String IDs, auto-commit, no configuration needed
- **HNSW Vector Search** — Approximate nearest neighbor with diversity heuristic
- **Scalar Quantization (SQ8)** — 4x RAM reduction for large-scale vector indices
- **Hybrid Search** — Combined BM25 (text) and Vector search via RRF (Reciprocal Rank Fusion)
- **Multi-Tenancy** — Logically isolated collections (namespaces) for different agents/tasks
- **SIMD Acceleration** — portable-simd for distance computation
- **LSM-Tree Persistence** — WAL + MemTable with crash recovery
- **Transactional** — Sharded TxBuffer with orphan-reaping

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
