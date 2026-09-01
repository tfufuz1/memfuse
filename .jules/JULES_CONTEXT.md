# MemFuse — Jules Agent Context
> Version: 2.0 | Stand: 2026-09-01 | Permanent Ambient Context für Jules Sessions
>
> ⚠️ **FRISCHEGARANTIE**: Diese Datei ist ein Kurzzeit-Snapshot.
> Bei Widerspruch gilt immer: `WORKING_STATE.md` (autogeneriert) > Code > diese Datei.
> Aktualisiere diesen Header-Timestamp wenn du dieses File bearbeitest.

---

## 🎯 Aktueller Projektstatus (Stand: 2026-09-01)

**Phase 1 — RAG-Fundament: ✅ VOLLSTÄNDIG ABGESCHLOSSEN (HEAD 4162ebb)**

| Sprint | Ziel | Status |
|--------|------|--------|
| Sprint RAG-01 | Contextual Retrieval | ✅ Fertig |
| Sprint RAG-02 | Cross-Encoder Reranking | ✅ Fertig |
| Sprint RAG-03 | Multi-Step Query Engine | ✅ Fertig |
| Sprint RAG-04 | Context Compaction | ✅ Fertig |
| Sprint RAG-05 | Session DAG + MCP Sandbox | ✅ Fertig |

**Phase 2 — Cognitive Memory: 🔄 In Umsetzung**
- Kognitive Gedächtnistypen & Importance Scoring (ADR-025) ✅
- Personalized PageRank (PPR) Graph-Retrieval (ADR-026) ✅
- Temporaler bi-temporaler Wissensgraph (ADR-033) ✅
- Zettelkasten Memory Links & Supersedes-Verdrängung (ADR-038) ✅

---

## 📐 DAG — Crate-Schichten (niemals verletzen)

```
Layer 0:  memfuse-core        ← Keine Workspace-Deps
Layer 1:  memfuse-{store,index,text,crypto,graph,checkpoint}  ← nur core
Layer 2:  memfuse-db          ← alle Layer-1 + ollama + embed(optional)
Layer 3:  memfuse-{py,ollama,embed,agent,router}  ← db + core (+ ollama/store)
Layer 4:  memfuse-{mcp,tauri} ← agent + db + ollama + crypto/graph
```

---

## 🚫 Kritische ADRs — VOR Code-Schreiben kennen

| ADR | Entscheidung | Konsequenz |
|-----|-------------|------------|
| ADR-010 | MCP = stdio JSON-RPC 2.0 | KEIN HTTP/axum in memfuse-mcp |
| ADR-016 | TxId via `collection.allocate_tx()` | NIE SystemTime |
| ADR-017 | unsafe NUR in distance.rs, diskann.rs, persistence.rs | Jedes unsafe → SAFETY: Beweis |
| ADR-028 | TS: + SESSION: Pflichtfelder | Tags ohne diese Felder = CI-Fehler |
| ADR-030 | Pre-Commit-Hook für rustfmt | `cargo fmt --all` vor Commit |
| ADR-039 | reqwest als Workspace-Dep | `reqwest` für memfuse-router freigegeben (rustls-tls, no native-tls) |
| ADR-041 | TOMBSTONE_BIT-Disziplin | Bit 63 strikt maskieren (`seq & !TOMBSTONE_BIT`) vor max_seq Vergleichen |
| ADR-043 | last_committed_tx vor SSTable-Sichtbarkeit | `last_committed_tx` vor `sstables.push()` in LsmStorage::flush aktualisieren |
| ADR-044 | MCP Write-Authorization & Sandbox Policy | DB-Schreibzugriffe im MCP Server standardmäßig GESPERRT (Read-Only) |
| ADR-045 | Entkopplung memfuse-router und memfuse-mcp | JSON-RPC Typen in `memfuse-core::ipc`, memfuse-router hängtfrei von memfuse-mcp |

---

## ✅ Was VOLLSTÄNDIG existiert (NICHT neu implementieren)

```rust
// Alle diese Dateien und Typen existieren bereits:
memfuse-db/src/fusion.rs         ✅ reciprocal_rank_fusion() + weighted variant
memfuse-db/src/collection.rs     ✅ hybrid_search() mit 4-Signal-Fusion
memfuse-db/src/chunker.rs        ✅ MarkdownChunker
memfuse-db/src/multistep.rs      ✅ MultiStepEngine
memfuse-db/src/compaction.rs     ✅ ContextCompactor mit StatusToken & LLM Summarize
memfuse-checkpoint/src/lib.rs    ✅ CheckpointGuard<S> RAII + PersistentCheckpointStore
memfuse-crypto/src/crypto.rs     ✅ AES-256-GCM-SIV + Zeroize
memfuse-embed/src/lib.rs         ✅ ONNX SessionPool (feature=onnx)
memfuse-embed/src/reranker.rs    ✅ CrossEncoderReranker
memfuse-mcp/src/lib.rs           ✅ stdio JSON-RPC 2.0 (ADR-010) + McpSandbox
memfuse-ollama/src/client.rs     ✅ embed() + embed_batch() + generate_text()
memfuse-ollama/src/context_prefixer.rs ✅ ContextPrefixEngine
memfuse-graph/src/csr.rs         ✅ CSR-Graph + BFS-Traversal + PPR (ADR-026)
memfuse-agent/src/lib.rs         ✅ PersistentAgentWorkflow
```

**VOR jeder Neuimplementierung**: `find crates/ -name "*.rs" | xargs grep -l "<Typ-Name>"` ausführen.

---

## 🔑 API-Fallstricke (LLM halluziniert diese falsch)

```rust
// TxId — RICHTIG:
let tx_id = collection.allocate_tx().await?;
// TxId — FALSCH (NIE!):
let tx_id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;

// WAL sync — RICHTIG:
dir.sync_all().await?;   // Fehler propagieren!
// WAL sync — FALSCH:
let _ = dir.sync_all();  // Silent failure = Datenverlust!

// HMAC Key — RICHTIG:
let key = load_or_create_integrity_key(&path)?;
// HMAC Key — FALSCH:
let key = b"hardcoded_key_32bytes___________"; // SECURITY BLOCKER

// Chunking — RICHTIG:
let chunks = MarkdownChunker::new(config).chunk(&text)?;
// Chunking — FALSCH:
collection.insert("key", &text, embedding).await?; // Gesamter Text als 1 Vektor!
```
