# MemFuse Python Bindings (`memfuse`)

Official Python bindings for MemFuse — an embedded 4-signal hybrid-search vector database built with Rust and PyO3.

## Installation

```bash
pip install memfuse
```

## Quick Start

```python
import memfuse

# Initialize database
db = memfuse.PyMemFuse("./data")
collection = db.collection("documents")

# Insert document
collection.insert("doc_1", "MemFuse provides high-performance embedded vector search.")

# Perform hybrid search
results = collection.hybrid_search("vector search")
for res in results:
    print(res.id, res.score, res.text)
```

## Development & Publishing

Refer to [PUBLISHING.md](PUBLISHING.md) for instructions on local building, testing, and release management.
