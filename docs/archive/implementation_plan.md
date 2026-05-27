# Strategic Architecture Audit — MemFuse

> **Date:** 2026-05-24  
> **Scope:** Full codebase (12,365 LoC Rust, 11 crates, 38 specs)  
> **Methodology:** Systematic `grep`, `tokei`, AST analysis, spec cross-reference

---

## Executive Summary

MemFuse has solid engineering foundations (LSM-Tree, HNSW with SQ8, BM25 with German morphology, atomic commits, mmap SSTables). However, the **central bottleneck identified in the strategic analysis — RAM exhaustion from in-memory vector storage — is unresolved**. The HNSW index stores all vectors as heap-allocated `Vec<f32>` / `Vec<u8>` with no disk-offloading path for the graph or vector data. On an 8GB Ryzen 3500U target, this limits practical capacity to ~200K 1536-dim embeddings before OOM.

---

## Weakness Catalog

### 🔴 Critical — Memory Architecture (Existential)

| ID | Weakness | Location | Impact |
|----|----------|----------|--------|
| **W-01** | **HNSW vectors fully in-memory** | [hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs) L171 `nodes: RwLock<Vec<HnswNode>>` | 1M × 1536-dim f32 = **5.7 GB RAM** for vectors alone. SQ8 reduces to 1.4 GB but graph metadata + connections add ~800MB. **System OOMs at ~300K docs on 8GB.** |
| **W-02** | **DiskANN scaffold: batch-only, no incremental ops** | [diskann.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs) L360-374 | [insert()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#132-159) and [delete()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#973-989) return `Err("read-only")`. Unusable as production index. Random graph construction instead of Vamana algorithm (Recall@10 far below 90%). |
| **W-03** | **No mmap for HNSW persistence** | `memfuse-index` has no `HnswIndex::save()/load()` | Index rebuilt from LSM scan on startup via `collection.load_index()`. Cold start at 100K docs = **minutes of CPU-bound reinsertion**. |
| **W-04** | **[StoredDocument](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#21-26) embeds full `Vec<f32>`** | [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs) L22-23 | Every document JSON blob includes the full embedding. On a 1536-dim model: 6KB per doc × 100K = 600MB of duplicated vector data in LSM + HNSW. |

### 🟠 High — Production Code Quality

| ID | Weakness | Location | Impact |
|----|----------|----------|--------|
| **W-05** | **`unwrap()` in production crypto** | [wal_crypto.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs) L117 `WalHmac::new(key).unwrap()` | Zero-Panic doctrine violation. HMAC init can fail on invalid key lengths → process crash. |
| **W-06** | **CRIT-001: DocId::from_key() uses .expect()** | `memfuse-core` | ✅ RESOLVED 2026-05-27 |
| **W-07** | **HIGH-001: WAL replay has no CRC verification** | `memfuse-store` (documented in AGENTS.md) | Corrupted WAL entries silently accepted on recovery → data integrity risk. |

### 🟡 Medium — Missing Capabilities (MVP Gap)

| ID | Weakness | Location | Impact |
|----|----------|----------|--------|
| **W-08** | **No Markdown chunking pipeline** | Entire codebase | RAG value prop requires semantic chunking. Users must pre-chunk externally, defeating "SQLite for AI agents" positioning. |
| **W-09** | **No MCP (Model Context Protocol) manifest** | No files found | External agent swarms cannot discover MemFuse as a context provider. Blocks "invisible engine" strategy. |
| **W-10** | **No ONNX embedding integration** | Only reference in [airgap.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-runtime/src/airgap.rs) scaffold | Users must bring their own embeddings. Air-Gap deployment (GS-06) requires local inference. Without it, MemFuse is a "dumb" vector store. |
| **W-11** | **[ContextManager](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs#20-26) disconnected from [Collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#54-63)** | [context.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs) | Takes `Vec<ContextChunk>` but no path from `Collection.search()` → [ContextChunk](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs#101-107) conversion. Dead code. |

### 🔵 Low — Structural

| ID | Weakness | Location | Impact |
|----|----------|----------|--------|
| **W-12** | **DiskANN cache eviction is naive (full clear)** | [diskann.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs) L341-343 | `cache.clear()` on budget exceed instead of LRU. Thrashing under high query load. |
| **W-13** | **`HnswConfig::default()` sets `quantize: false`** | [hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs) L83 | SQ8 reduces memory 4× but is opt-in. On 8GB target, this should default to `true`. |
| **W-14** | **[scan_prefix()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#468-504) uses `unwrap_or()` on prefix stripping** | [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs) L472 | Not a panic but sloppy handling of the `__rel:` prefix strip. |

---

## Prioritized Remediation — 4 Swim Lanes

### Swim Lane 1: Critical Memory Path (W-01, W-02, W-03, W-04, W-13)

> **Goal:** RAM footprint < 500MB for 100K docs on embedded hardware

| Step | Action | Crate | Effort |
|------|--------|-------|--------|
| **1a** | Default `quantize: true` in [HnswConfig](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#54-72) for 8GB targets | `memfuse-index` | 1 LoC |
| **1b** | Remove `embedding: Vec<f32>` from [StoredDocument](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#21-26) — store vectors separately in a compact binary format alongside the LSM, reference by DocId | `memfuse-db` | ~100 LoC |
| **1c** | Implement HNSW persistence via mmap: `HnswIndex::save(path)` / `HnswIndex::load(path)` serializing graph + quantized vectors to a flat file | `memfuse-index` | ~400 LoC |
| **1d** | Replace DiskANN random graph with simplified Vamana construction. Support incremental inserts via in-memory staging buffer + periodic merge | `memfuse-index` | ~500 LoC |
| **1e** | Add `MemoryProfile` enum (`{ Embedded, Server }`) to `MemFuseConfig` that auto-tunes quantize/cache/mmap defaults | `memfuse-db` | ~50 LoC |

### Swim Lane 2: Production Hygiene (W-05, W-06, W-07)

| Step | Action | Crate | Effort |
|------|--------|-------|--------|
| **2a** | Replace `WalHmac::new(key).unwrap()` → `WalHmac::new(key).map_err(...)` with proper error propagation | `memfuse-crypto` | 3 LoC |
| **2b** | Fix CRIT-001: `DocId::from_key()` `.expect()` → `?` propagation | `memfuse-core` | ✅ DONE |
| **2c** | Add CRC32 verification to WAL replay loop | `memfuse-store` | ~30 LoC |

### Swim Lane 3: RAG Pipeline (W-08, W-10, W-11)

| Step | Action | Crate | Effort |
|------|--------|-------|--------|
| **3a** | New module `memfuse-db/src/chunker.rs`: Markdown heading-aware chunking (`# H1`, `## H2`, paragraphs). Output: `Vec<ContextChunk>` with heading breadcrumbs as metadata | `memfuse-db` | ~200 LoC |
| **3b** | Wire [ContextManager](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs#20-26) to [Collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#54-63): `Collection::prepare_context(query, budget)` that chains search → hydrate → ContextManager | `memfuse-db` | ~50 LoC |
| **3c** | Add `memfuse-embed` crate stub with `EmbeddingProvider` trait + ONNX runtime integration (optional feature flag) | `memfuse-embed` [NEW] | ~300 LoC |

### Swim Lane 4: Market Positioning (W-09)

| Step | Action | Location | Effort |
|------|--------|----------|--------|
| **4a** | Create `mcp.json` manifest declaring MemFuse as MCP context provider with [search](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs#197-269), [get](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#299-303), [insert](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#132-159) tools | Project root | ~50 lines JSON |
| **4b** | Document Atlas integration strategy as ADR | `docs/specs/decisions/ADR-008-atlas-integration.md` | ~50 lines |

---

## Spec Updates Required

### Immediate Updates

1. **SPEC-20260505-WP-4.x-Scale.md** — WP-4.1 (mmap SSTable) should be marked ✅ DONE. WP-4.3 (DiskANN) needs rewrite: current scaffold is far from the spec's 90% Recall@10 target. Add incremental-insert requirement.

2. **SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md** — GS-06 (Air-Gap) needs concrete embedding model list (E5-large, BGE-small, multilingual-e5). Add `memfuse-embed` as new crate.

3. **SYSTEM.spec.md** — Add Level 2.5 for `memfuse-embed` crate. Update roadmap Phase 4 to explicitly include HNSW persistence.

### New Specs Needed

4. **SPEC-WP-7.1-MarkdownChunker.md** — Semantic chunking based on Markdown heading hierarchy.
5. **SPEC-WP-7.2-HnswPersistence.md** — mmap-based graph + vector persistence for cold-start elimination.
6. **SPEC-WP-7.3-MCPProvider.md** — MCP manifest and tool implementations.
7. **ADR-008-atlas-integration.md** — B2C pivot strategy: MemFuse as invisible engine inside Atlas/Tauri.

---

## Verification Plan

### Automated Tests

**Swim Lane 1 (Memory Path)**:
```bash
# After implementing W-13 fix (quantize default):
cargo test -p memfuse-index -- hnsw --nocapture

# After HNSW persistence:
cargo test -p memfuse-index -- persistence --nocapture

# Memory footprint validation (new benchmark):
cargo bench -p memfuse-index -- memory_footprint
```

**Swim Lane 2 (Hygiene)**:
```bash
# Zero-panic audit:
just debt-audit

# Full workspace:
cargo test --workspace
```

**Swim Lane 3 (RAG Pipeline)**:
```bash
# Chunker tests:
cargo test -p memfuse-db -- chunker --nocapture

# ContextManager integration:
cargo test -p memfuse-db -- context --nocapture
```

### Manual Verification

1. **RAM validation**: Run `cargo bench -p memfuse-index -- memory_footprint` and verify peak RSS < 500MB for 100K × 384-dim SQ8 vectors
2. **Cold-start**: Measure `collection.load_index()` time before/after HNSW persistence
3. **Markdown chunking**: Feed a real 50-page Markdown doc and verify chunk boundaries align with heading hierarchy

---

## Risk Assessment

| Risk | Probability | Mitigation |
|------|------------|------------|
| Microsoft/Google free RAG (Copilot Monopolization) | **75%** (70-85% CI) | Air-Gap niche: fully offline, no cloud dependency, compliance certifiable |
| HNSW persistence breaks concurrent access | Medium | mmap + `Arc<Mmap>` with reader-writer isolation (SSTable pattern proven) |
| ONNX runtime adds 50MB+ binary size | Medium | Feature-gated behind `embedding` cargo feature |
| DiskANN Vamana too complex for MVP | Low | Start with simplified Vamana (random + beam search refinement) |

---

> [!IMPORTANT]
> **Empfohlene Reihenfolge:** Swim Lane 2 (Hygiene, 1 Tag) → Swim Lane 1 Steps 1a-1c (Memory, 1 Woche) → Swim Lane 3 Step 3a (Chunker, 3 Tage) → Swim Lane 4 (MCP, 1 Tag). DiskANN Vamana (1d) und ONNX (3c) sind Sprint+1.
