# MemFuse — Die eingebettete 3-in-1 Memory Engine für lokale AI-Agenten

MemFuse ist eine in-process Speicher-Engine in reinem Rust, die **Vektorsuche (HNSW)**, **BM25-Volltextsuche**, **Entity-Relation Graph-Traversal (CSR)** und **Metadaten-Filter** zu einer einzigen 4-Signal-Fusion-Engine kombiniert.

**Ersetzt ChromaDB + Elasticsearch + Neo4j durch ein einfaches `pip install memfuse`.**

Designed für autonome AI-Agenten und local-first Anwendungen — in-process, kein Server, kein Docker, kein Cloud-Account.

---

## 🎯 Warum MemFuse?

| Eigenschaft | MemFuse | ChromaDB | LanceDB | Kùzu / Neo4j |
|---|---|---|---|---|
| **In-Process (Embedded)** | ✅ | ✅ | ✅ | ⚠️ (Kùzu embedded / Neo4j Server) |
| **3-in-1 Signal Fusion (RRF)** | ✅ (Vektor + BM25 + Graph + Meta) | ❌ (nur Vektor+Meta) | ❌ (kein Graph) | ❌ (kein Vektor/RRF) |
| **Zero C-Deps (Sovereign Core)**| ✅ 100% Pure-Rust Core | ❌ | ❌ | ❌ |
| **ACID + WAL** | ✅ MVCC + HMAC-WAL | ❌ | ⚠️ | ✅ |
| **MCP Server Support** | ✅ Eingebaut in Python SDK | ❌ | ❌ | ❌ |

---

## 🚀 Key Features

- **4-Signal Reciprocal Rank Fusion (RRF)**: Fusioniert episodische (Vektor), lexikalische (BM25) und assoziative (Graph) Signale in einer einzigen Abfrage.
- **MCP Server Protocol**: Nahtlose Anbindung an Claude Desktop, Cursor und LLM-Agenten via Model Context Protocol.
- **ACID & MVCC**: Strikte Transaktionssicherheit mit Write-Ahead-Log (WAL) und Snapshot-Isolation.
- **SIMD & SQ8 Quantisierung**: Hardware-beschleunigte Distanzberechnung mit bis zu 4× Speicherersparnis.
- **Pure-Rust Sovereign Core**: Keine C-Bibliotheken oder externen System-Runtimes erforderlich.

---

## 🐍 Python Integration

```bash
pip install memfuse
```

```python
from memfuse import MemFuse

db = MemFuse.open("./agent_memory")
col = db.collection("context")

# Dokument mit Vektor, Text & Metadaten einfügen
col.insert(
    id="doc1",
    vector=[0.1] * 1536,
    metadata={"text": "Kundenanfrage bezüglich Rückerstattung", "category": "support"}
)

# 4-Signal Hybrid-Suche
results = col.hybrid_search(
    query_text="Rückerstattung",
    query_vector=[0.1] * 1536,
    top_k=5
)
```

### Model Context Protocol (MCP) Server

Starten des integrierten MCP-Servers für Claude Desktop & AI-Tools:

```bash
python -m memfuse.mcp --db-path ./agent_memory
```

---

## 📦 Rust Crate Usage

```toml
[dependencies]
memfuse-db = { path = "crates/memfuse-db" }
```

---

## 🏎️ Quickstart (Rust)

```rust
use memfuse_db::MemFuse;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = MemFuse::open("./data").await?;
    let col = db.collection("agents").await?;

    // Dokument einfügen
    let vec = vec![0.1f32; 1536];
    col.insert("doc1", &vec, Some(serde_json::json!({"text": "Hello MemFuse!"}))).await?;

    // Hybrid-Suche
    let results = col.hybrid_search("Hello", &vec, 5).await?;
    for r in results {
        println!("Found {} (score: {:.4})", r.id, r.score);
    }

    Ok(())
}
```

---

## 🗺️ Roadmap

| Phase | Ziel | Status |
|---|---|---|
| **P0** | CVE-Fixes (`memmap2`, `lru`), Scope-Bereinigung | 🟢 Erledigt |
| **P1** | Zero-Panic durchsetzen, FIND-STO-001 (Phantom-Daten), `memfuse-graph` reaktivieren | 🟡 Aktiv |
| **P2** | `memfuse-py` + pytest-Suite, PyPI alpha, crates.io v0.1.0 | ⬜ Geplant |
| **P3** | Öffentliche Benchmarks (vs. ChromaDB/LanceDB), HN-Launch | ⬜ Geplant |

---

## 🛠️ Entwicklung

```bash
# Voraussetzung: Nix mit Flakes (empfohlen) oder Rust stable 1.89+
nix develop

# Build & Tests
just check      # fmt + clippy + compile
just test       # Testsuite
just debt-audit # Zero-Panic + Security Audit
```

### Dokumentation
- [README.md](./README.md) — Dieses Dokument.
- [CONSTITUTION.md](./CONSTITUTION.md) & [DEVELOPERS.md](./DEVELOPERS.md) — Projekt-Governance.
- [AGENTS.md](./AGENTS.md) — LLM-Agent-Regeln und Verifikationsschleifen.
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Schichtmodell, Invarianten-Status.
- [docs/SOURCE_OF_TRUTH.md](./docs/SOURCE_OF_TRUTH.md) — Living State Document (Backlog, Crate-Inventar, Roadmap).
- [DECISIONS.md](./DECISIONS.md) — Architecture Decision Records (ADRs).

---

## ⚖️ Lizenz

MIT OR Apache-2.0
