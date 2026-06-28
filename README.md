# MemFuse — Embedded Hybrid-Search for AI Agents

MemFuse is a high-performance, embedded hybrid-search database written in Rust. It combines vector similarity search, keyword-based BM25, and relationship traversal into a single, unified "4-Signal Fusion" engine.

Designed for AI agents and local-first applications, MemFuse provides a production-ready, ACID-compliant storage layer with a minimal footprint.

---

## 🚀 Key Features

- **4-Signal Fusion**: Combines Vector, Text (BM25), Graph, and Metadata signals for unparalleled recall.
- **ACID Compliant**: Transactional safety with MVCC and a robust Write-Ahead-Log (WAL).
- **Embedded & Sovereign**: Zero external C-dependencies. Runs locally on Linux/macOS.
- **SIMD Accelerated**: Hardware-accelerated vector distances (AVX-512, AVX2, NEON).
- **Quantization (SQ8)**: Reduces memory footprint by up to 4x with minimal recall loss.
- **Python Bindings**: Seamless integration with NumPy and the Python AI ecosystem.

---

## 📦 Installation

### Rust
Add MemFuse to your `Cargo.toml`:
```toml
[dependencies]
memfuse-db = "0.2.0"
```

### Python
```bash
pip install memfuse
```

---

## 🏎️ Quickstart (Python)

```python
import memfuse
import numpy as np

# Open or create a database
with memfuse.open("./data", dimension=1536) as db:
    # Insert a document with embedding and metadata
    vector = np.random.rand(1536).astype("float32")
    db.insert("doc1", vector, {"text": "Hello MemFuse!", "category": "AI"})

    # Perform hybrid search
    results = db.hybrid_search("Hello", vector, k=5)
    
    for r in results:
        print(f"Found {r.id} with score {r.score}")
```

---

## 🛠️ Development

MemFuse uses a strict MECE **Unified Documentation System**. For all details, see:
- [README.md](./README.md) — This file, features and high-level introduction.
- [CONSTITUTION.md](./CONSTITUTION.md) & [DEVELOPERS.md](./DEVELOPERS.md) — Mandatory developer policies.
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — System design DAG and invariants.
- [docs/SOURCE_OF_TRUTH.md](./docs/SOURCE_OF_TRUTH.md) — The living document matching the exact current implementation state.

---

## ⚖️ License

MemFuse is licensed under the Apache 2.0 / MIT License.
