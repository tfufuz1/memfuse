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

## Breaking Changes & Release Notes

- **Default Dimension Change**: The default `dimension` parameter in `memfuse.open()` was changed from `1536` to `768` to align with `MemFuseConfig::default().dimension` in `memfuse-db` (matching `nomic-embed-text`, the default ONNX embedding model). Callers relying implicitly on `1536` dimensions must explicitly pass `dimension=1536`.

## Development & Publishing

Refer to [PUBLISHING.md](PUBLISHING.md) for instructions on local building, testing, and release management.

> **Note on Workspace Architecture**:
> Dieses Crate wird ABSICHTLICH NICHT im Root-Workspace geführt, da es ein abweichendes Panic-Profil (`unwind` statt `abort`) für sichere FFI-Panic-Behandlung benötigt (siehe `run_blocking_ffi`, AGT-PY-d5d2be30). Build separat via `cd crates/memfuse-py && cargo build --release`.
