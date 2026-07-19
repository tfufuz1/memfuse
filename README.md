# MemFuse — Eingebettete 4-Signal-Memory-Engine für lokale AI-Agenten

MemFuse ist eine hochperformante, eingebettete Hybrid-Search-Engine in reinem Rust. Sie kombiniert Vektor-Ähnlichkeitssuche, BM25-Volltextsuche, Beziehungsgraph-Traversal und Metadaten-Filter zu einer einzigen **4-Signal-Fusion-Engine**.

Designed für AI-Agenten und local-first-Anwendungen — kein Server, kein Docker, kein Cloud-Account.

> ⚠️ **Status: Pre-Alpha (v0.1.0 ausstehend)** — Kern-Crates sind funktionsfähig, Python-Bindings und öffentliche Releases sind in Vorbereitung (siehe [Roadmap](#roadmap)).

---

## 🎯 USP gegenüber ChromaDB / LanceDB / Qdrant

| Eigenschaft | MemFuse | ChromaDB | LanceDB | Qdrant |
|---|---|---|---|---|
| **In-Process (kein Server)** | ✅ | ✅ | ✅ | ❌ (Server) |
| **4-Signal Fusion** | ✅ (Vektor+BM25+Graph+Meta) | ❌ (nur Vektor+Meta) | ❌ | ❌ |
| **Zero-C-Deps (Default)** | ✅ Sovereign Core | ❌ | ❌ | ❌ |
| **ACID + WAL** | ✅ | ❌ | ⚠️ | ✅ |
| **Python Bindings** | 🟡 In Entwicklung | ✅ | ✅ | ✅ |

---

## 🚀 Features

- **4-Signal Fusion**: Kombiniert Vektor (HNSW), Text (BM25), Graph (CSR) und Metadaten über Reciprocal Rank Fusion.
- **ACID-Compliant**: Transaktionssicherheit mit MVCC und Write-Ahead-Log (WAL).
- **Embedded & Sovereign**: Zero externe C-Abhängigkeiten. Läuft lokal auf Linux/macOS.
- **SIMD-beschleunigt**: Hardware-beschleunigte Vektordistanzen (AVX-512, AVX2, NEON).
- **Quantisierung (SQ8)**: Reduziert den Speicherbedarf um bis zu 4× bei minimalem Recall-Verlust.

---

## 📦 Installation (Rust — aktuell verfügbar)

```toml
# Cargo.toml
[dependencies]
memfuse-db = { path = "crates/memfuse-db" }  # Noch kein crates.io-Release
```

## 🐍 Python (In Vorbereitung)

> PyPI-Release ist in Phase 2 der Roadmap. Aktuell über `maturin develop` im Repo verwendbar.

```bash
# Lokale Entwicklung (erfordert Rust + maturin)
cd crates/memfuse-py
maturin develop
```

---

## 🏎️ Quickstart (Rust)

```rust
use memfuse_db::MemFuse;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = MemFuse::open("./data").await?;
    let col = db.create_collection("agents", 1536).await?;

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
| **P0** | CVE-Fixes (`memmap2`, `lru`), Scope-Bereinigung | 🟡 Aktiv |
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
