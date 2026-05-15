# MemFuse

**Sovereign Agentic Operating System (SAOS)**

> "The ultimate runtime for AI Agents: Database, Saftey Layer, and Orchestrator in one crate."

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

## Architecture: The 3 SAOS Layers

MemFuse is no longer just a vector database, it is the runtime environment in which the agent exists.

1. **Das Triebwerk (Data & Memory Foundation)**
```
memfuse-core   ← Core Types, WAL, Transactions, State Checkpoints
memfuse-store  ← LSM-Tree Persistence
memfuse-index  ← HNSW + SIMD Vector Search + CSR Graph (Multi-Hop)
memfuse-text   ← Inverted Index + BM25 Scoring
```
*Provides 4-Signal-Fusion (Dense, Sparse, Graph, Meta) at edge latency.*

2. **Das Getriebe (Execution & Safety Layer)**
```
memfuse-runtime ← WASM Sandboxing & Native Tool Execution
```
*Provides guaranteed host-safety and native state checkpointing (Time-Travel).*

3. **Das Cockpit (Agentic Workflow Orchestration)**
```
memfuse-orchestrator ← Rust-native Declarative StateGraphs
memfuse-py           ← PyO3 bindings (`pip install memfuse`)
```
*Autonomously injects context, controls execution flow, and enforces isolation.*

## 4. The Jules Squad: 13 Autonomous Agents

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
- **Relationship Tracking** — `relate()` API for graph-aware retrieval
- **Hybrid Search** — Optimized BM25 + Vector Fusion (RRF)
- **Scalar Quantization** — SQ8 compression for 4x reduced RAM footprint
- **Deterministic Checkpointing** — Native state pinning for "Time-Travel" debugging

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
