# MemFuse — Jules Agent Context
> Version: 1.0 | Stand: 2026-08-26 | Permanent Ambient Context für Jules Sessions
>
> ⚠️ **WICHTIGE FRISCHEGARANTIE**: Diese Datei kann veraltet sein (statischer Snapshot). Vor Nutzung IMMER gegen `WORKING_STATE.md` (autogeneriert, aktueller) und den tatsächlichen Code gegenprüfen. Bei Widerspruch gilt `WORKING_STATE.md` + Code, nicht diese Datei.
>
> Diese Datei wird ZUSÄTZLICH zu AGENTS.md gelesen. Sie enthält jules-spezifische
> Kontext-Informationen, die den allgemeinen AGENTS.md-Regeln übergeordnet sind.
 
---
 
## 🎯 Aktuelle Strategische Ausrichtung
 
MemFuse entwickelt sich von einem "4-Signal-Speicher-Engine" zu einem
**Cognitive Operating System für LLM-Agenten** — vollständig lokal, air-gapped,
Pure-Rust, ohne Cloud-Dependencies.
 
### Neue Kernziele (Sprint 1–5 / Q3-Q4 2026)
 
| Sprint | Ziel | Crates | Status |
|--------|------|--------|--------|
| **Sprint 1** | Contextual Retrieval (Anthropic-Pattern) | core, ollama, text | 🔄 Aktiv |
| **Sprint 2** | Cross-Encoder Reranking (ONNX) | embed, db | 📋 Geplant |
| **Sprint 3** | Multi-Step Query Engine | db | 📋 Geplant |
| **Sprint 4** | Context Compaction | db, core | 📋 Geplant |
| **Sprint 5** | MCP Sandbox Isolation | mcp, crypto | 📋 Geplant |
| **Sprint 6** | Cognitive Memory Types | core (new types) | 🔮 Zukunft |
| **Sprint 7** | Session DAG Branching | graph, checkpoint | 🔮 Zukunft |
 
---
 
## 📐 Architektur-Invarianten (Jules MUSS diese kennen)
 
### Crate DAG (gerichteter azyklischer Graph — NIEMALS verletzen)
 
```
memfuse-core
├── memfuse-store (→ core, crypto)
├── memfuse-index (→ core)
├── memfuse-text (→ core)
├── memfuse-crypto (→ core)
├── memfuse-graph (→ core)
└── memfuse-checkpoint (→ core)
    └── memfuse-db (→ alle Layer-1 Crates)
        ├── memfuse-py (→ db, core)  [DAG-003: erlaubte Ausnahme]
        ├── memfuse-ollama (→ core)
        ├── memfuse-embed (→ core)   [optional, feature=onnx]
        └── memfuse-agent (→ db, graph, checkpoint, store, core)
            ├── memfuse-mcp (→ db, ollama, optional agent)
            └── memfuse-tauri (→ db, graph, ollama)
```
 
**NIEMALS**: Layer-4 importiert Layer-4, Layer-1 importiert Layer-2+
 
### Kritische ADRs (Jules muss diese kennen BEVOR Code geschrieben wird)
 
| ADR | Entscheidung | Konsequenz |
|-----|-------------|-----------|
| **ADR-010** | MCP = stdio JSON-RPC 2.0 ONLY | Kein axum, kein HTTP in memfuse-mcp |
| **ADR-016** | TxId-Domänen: Collection [1,10^12] vs INTERNAL [INTERNAL_BASE, u64::MAX] | Niemals SystemTime als TxId |
| **ADR-017** | unsafe ONLY in distance.rs, diskann.rs, persistence.rs (Index-Crate) | Jedes unsafe braucht `// SAFETY:` Beweis |
| **ADR-018** | Doppelstrategie: PyPI (memfuse-py) + Desktop (memfuse-tauri) | Beide Pfade gleichwertig |
| **ADR-019** | Contextual Retrieval via `combined_text_owned()` | Präfix nicht im Originalinhalt überschreiben, Serde #[serde(default)] |
| **ADR-020** | Cognitive Operating System als Produktvision / Wiederherstellung `memfuse-agent` | Gedächtnistypen & temporale Graphen als Zielvision; agent-Restore |
| **ADR-021** | Multi-Signal RAG-Pipeline (Contextual → RRF → Reranking) | Gestaffelte additiv-degradierende RAG-Pipeline |
 
---
 
## 🔍 Aktuelle Codebase-Fakten (Ist-Stand 2026-08-25)
 
### Was BEREITS vollständig implementiert ist (kein Code nötig)
 
```
memfuse-db/src/fusion.rs        ✅ RRF (k=60, gewichtet, Proptests)
memfuse-db/src/collection.rs   ✅ hybrid_search() BM25+HNSW+Graph
memfuse-checkpoint/src/lib.rs  ✅ CheckpointGuard<S> RAII
memfuse-crypto/src/crypto.rs   ✅ AES-256-GCM-SIV + Zeroize
memfuse-embed/src/lib.rs       ✅ ONNX SessionPool (feature=onnx)
memfuse-mcp/src/lib.rs         ✅ stdio JSON-RPC 2.0 (ADR-010)
memfuse-db/src/chunker.rs      ✅ MarkdownChunker
memfuse-ollama/src/client.rs   ✅ embed() + embed_batch() + exponential backoff
memfuse-graph/src/csr.rs       ✅ CSR-Graph mit BFS-Traversal
```
 
### Was FEHLT (Neue Sprint-Ziele)
 
```
memfuse-core/src/types/saos.rs         ❌ ContextChunk.contextual_prefix (Sprint 1)
memfuse-ollama/src/client.rs           ❌ generate_text() non-streaming (Sprint 1)
memfuse-ollama/src/context_prefixer.rs ❌ Neue Datei: ContextPrefixEngine (Sprint 1)
memfuse-embed/src/reranker.rs          ❌ Cross-Encoder ONNX (Sprint 2)
memfuse-db/src/multistep.rs            ❌ Multi-Step Query Loop (Sprint 3)
memfuse-db/src/compaction.rs           ❌ Context Compaction (Sprint 4)
memfuse-graph/src/session_dag.rs       ❌ Session DAG (Sprint 7, Zukunft)
```
 
### Kritische API-Fakten (falsch dokumentierte Blueprints korrigiert!)
 
```rust
// ContextChunk ist in memfuse-core/src/types/saos.rs (NICHT neu erstellen)
// → Feld contextual_prefix: Option<String> hinzufügen
 
// OnnxSessionPool ist NICHT pub — der pub Typ ist TextEmbedder
// → Reranker muss als separater pub struct mit eigenem Session-Pool
 
// CheckpointGuard<S: StorageEngine> ist NICHT klonbar (RAII)
// → Arc<Mutex<CheckpointStore>> für Session-DAG verwenden
 
// petgraph ist NICHT im Workspace
// → CsrGraph erweitern ODER petgraph zu memfuse-graph/Cargo.toml addieren
//   (braucht Human-Approval wegen "ASK"-Regel für neue Dependencies)
 
// generate_text() existiert NICHT in OllamaClient
// → Neue pub async fn generate_text() in memfuse-ollama/src/client.rs
```
 
---
 
## 🛡️ Jules Session Protocol (ERGÄNZUNG zu AGENTS.md §6)
 
### Vor JEDER Jules-Session ausführen
 
```bash
# Schritt 1: Aktuellen State lesen
cat WORKING_STATE.md
 
# Schritt 2: Offene kritische Tags prüfen
grep -rn 'AI-TAG\[SMELL\]\[CRITICAL\]' crates/ --include='*.rs' | grep -v RESOLVED
 
# Schritt 3: Letzte 3 ADRs lesen
grep -A 5 "^## ADR-" DECISIONS.md | tail -40
 
# Schritt 4: Aktuellen Sprint-Status prüfen
grep -A 3 "Sprint" WORKING_STATE.md | head -30
```
 
### Nach JEDER Jules-Session ausführen
 
```bash
# PFLICHT: Working State updaten
# Format: Datum, Session-Ziel, Was wurde geändert, Offene Issues
 
# Quality Gate
cargo test --workspace --exclude memfuse-tauri 2>&1 | tail -10
cargo clippy --workspace --exclude memfuse-tauri -- -D warnings 2>&1 | grep "^error" | wc -l
```
 
---
 
## 📊 Sprint-Priorisierung für Jules
 
### Reihenfolge nach Risiko & Abhängigkeiten
 
```
Tier 1 — Sofort (Infrastruktur für alle weiteren Sprints)
  Sprint 1: Contextual Prefix (memfuse-core → memfuse-ollama → memfuse-text)
  → Reihenfolge: core zuerst (kein Breaking Change), dann ollama, dann text
 
Tier 2 — Nach Tier 1 (Retrieval-Qualität)
  Sprint 2: ONNX Reranker (memfuse-embed)
  Sprint 3: Multi-Step (memfuse-db)
  → Unabhängig voneinander, können parallelisiert werden
 
Tier 3 — Nach Tier 2 (Agentische Features)
  Sprint 4: Context Compaction (memfuse-db)
  Sprint 5: MCP Sandbox (memfuse-mcp + memfuse-crypto)
  → Sprint 5 benötigt Sprint 4 für Token-Budget-Tracking
 
Tier 4 — Strategie (Q1 2027)
  Sprint 6: Cognitive Memory Types (neue Core-Typen)
  Sprint 7: Session DAG (memfuse-graph Erweiterung)
```
 
---
 
## 🚫 MemFuse-spezifische Jules-Verbote
 
Diese Regeln gelten ZUSÄTZLICH zu AGENTS.md §5:
 
```
NIEMALS in Jules-Sessions:
  - petgraph als dependency hinzufügen OHNE Human-Approval
  - OnnxSessionPool direkt exportieren (ist pub(crate))
  - CheckpointGuard klonen (ist RAII, kein Clone)
  - SessionTime/SystemTime für TxId (ADR-016)
  - axum in memfuse-mcp hinzufügen (ADR-010)
  - generate_text() in OllamaClient übersehen (nicht vorhanden, muss neu)
  - contextual_prefix als separaten Typ statt ContextChunk-Feld
 
IMMER in Jules-Sessions:
  - combined_text_owned() statt direkter String-Konkatenation
  - Serde-Backward-Compatibility (#[serde(default)]) für neue Felder
  - SAFETY: Kommentar für jeden unsafe Block in memfuse-index
  - allow_threads() für jeden block_on() in memfuse-py
```
 
---
 
## 🔢 Zahlen & Metriken (Stand 2026-08-25)
 
```
Crates total:    14 (13 Kern + 1 optional)
Rust-Dateien:    ~130
Sprint-Status:   Sprint 3 abgeschlossen (WORKING_STATE.md)
Offene AI-TAGs:  0 (alle RESOLVED 2026-08-25)
Test Coverage:   Proptests in fusion.rs, DocId, BM25
Performance:     Hybrid-Query <50ms auf 1M Vektoren (k=10)
Security:        AES-256-GCM-SIV + HMAC-SHA256 + HKDF (vollständig)
```
 
---
 
## 📚 Referenz-Dokumente (on-demand, nicht ambient)
 
| Dokument | Wann lesen |
|----------|-----------|
| `DECISIONS.md` | Vor jedem ADR-relevanten Change |
| `CONSTITUTION.md` | API-Design, Security-Entscheidungen |
| `docs/SOURCE_OF_TRUTH.md` | Crate-Inventar, Layer-Status |
| `docs/ARCHITECTURE.md` | Layer-Grenzen, Invarianten |
| `rules/simd_safety.md` | Vor unsafe-Code in memfuse-index |
| `rules/wal_crypto.md` | Vor WAL/Crypto-Änderungen |
| `rules/llm_protocol.md` | Vor Ollama/MCP-Änderungen |
