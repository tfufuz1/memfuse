# MemFuse Master Specification & LLM Playbook
> **Status:** 🔴 ACTIVE - Pflichtlektüre für ALLE Coding Agenten (Jules Accounts)
> **Autor:** Lead Architect

## 🎯 1. Produktvision (Das Endprodukt)
MemFuse ist das "SQLite für KI-Agenten" – eine in-process, einbettbare und extrem performante Vektor- und Hybrid-Suchdatenbank, speziell konzipiert für lokale LLM-RAG-Systeme. Die Architektur erzwingt absolute Stabilität und Speichersicherheit bei der Verarbeitung hochdimensionaler Vektoren und textueller Metadaten.

---

## 🧩 2. Rekursive Systemzerlegung (Top-Down Breakdown)

Das Endprodukt wird iterativ von Coding Agenten gebaut und ist in isolierte Crates (DOMAINS) unterteilt. Es gilt ein striktes Directed Acyclic Graph (DAG) Dependency-Modell.

### Level 0: Das User-Interface & Security
- **`memfuse-py` (WP-3.1)**: PyO3/Maturin basierte Python-Bindings. Dies ist das primäre Interface für Endnutzer. Setzt strikt auf `memfuse-db` auf.
- **`memfuse-crypto` / Features (WP-3.2)**: "Encryption at Rest". Verschlüsselung der Persistenzschicht (AES-GCM oder ChaCha20).

### Level 1: Die Orchestrierung & API-Schicht
- **`memfuse-db` (WP-1.2, WP-4.2)**: Orchestriert die Zugriffe auf den Store, den Vector-Index und den Text-Index. Kapselt die interne Komplexität und abstrahiert die `Collections` (Namespaces). Enthält die Hybrid-Search Facade, die RRF (Reciprocal Rank Fusion) auf die Resultate von `index` und `text` anwendet.

### Level 2: Die Sub-Engines (Isoliert, kommunizieren NIE direkt miteinander)
- **`memfuse-store` (WP-1.1, WP-4.1)**: Die Persistenzschicht. Implementiert als Log-Structured Merge Tree (LSM) mit Background Compaction. Verwaltet MemTables, Write-Ahead-Logs (WAL) und SSTables. In Zukunft optimiert mit Memory-Mapped I/O.
- **`memfuse-index` (WP-2.2, WP-4.3, WP-7.2)**: Die Vektor-Engine. Implementiert HNSW-Graphen für Approximate Nearest Neighbor (ANN) Search. Beinhaltet SIMD-optimierte Distanzfunktionen und Scalar Quantization (SQ8) für RAM-Reduzierung. **NEU:** mmap-basierte HNSW-Persistence.
- **`memfuse-embed` (WP-6.6)**: Lokale Inferenz-Engine via ONNX Runtime. Ermöglicht Air-Gap Deployments ohne externe API-Key Abhängigkeit.
- **`memfuse-text` (WP-2.1)**: Die Volltext-Engine. Stellt einen Inverted Index mit BM25-Scoring zur Verfügung, der mit Tokenizern arbeitet.

### Level 3: Der Shared Kernel (Das Rückgrat)
- **`memfuse-core` (WP-0.0)**: Enthält `MemFuseError` (für Zero-Panic), `TxBuffer`, `MemBank`, Paging-Strukturen und Snapshot-Isolation (MVCC). Dies ist die einzige Crate, die von allen anderen Domänen importiert werden darf. Keine anderen Crates dürfen Abhängigkeiten aus Level 1 oder 2 haben.

---

## 🤖 3. Der LLM Agent Workflow (The Playbook)

Als Code Agent (z.B. "Jules Account X") bist du verpflichtet, jeden Entwicklungsschritt präzise nach folgendem **Sovereign Core TDD-Loop** auszuführen:

### Phase 1: Context & Debt Audit
Bevor Code geschrieben wird, evaluierst du das System:
1. Prüfe den Workspace: `just debt-audit`
2. **STOPP**: Wenn der `debt-audit` fehlschlägt (z.B. durch neu eingeführte `.unwrap()` oder blockierende `std::fs` calls), ist das Beheben dieser Schulden Prio 1! **Kein neues Feature vor Null-Schuld.**
3. Identifiziere dein aktuelles Work Package (WP) aus `AGENTS.md`.

### Phase 2: Atomic Spec Creation & Review
Wenn du eine Funktionalität beginnst, generiere die Spezifikation:
1. Führe `just spec [WP-Name]` aus.
2. Fülle die `docs/specs/...` exakt nach Vorlage auf (Ziel, Invarianten, Fail-Cases, TDD).
3. Bestätige die Invarianten: Was muss bei einem Crash auf der Disk garantiert gleich bleiben?

### Phase 3: Triple-Test-Gate (Red Phase)
1. Schreibe Tests, **bevor** du die Funktionalität in `src/` ausbaust.
2. Die Tests müssen fehlschlagen (Red).
3. Berücksichtige Edge-Cases (z.B. Concurrent Reads, File Corruption, Interruptions).

### Phase 4: Implementation (Green Phase)
Beim Schreiben des Codes gelten die **Absoluten Gesetze**:
1. **Zero-Panic:** Verwende NIEMALS `.unwrap()`, `.expect()` oder `panic!()`. Propagiere Fehler über `?` in `memfuse_core::MemFuseError`.
2. **Zero-Blocking I/O:** Nutze ausschließlich `tokio::fs` oder `tokio::io`. Ein `std::fs` Call in einem async-Kontext führt zu CI-Fehlern.
3. **Safe-Rust Isolation:** `unsafe` ist strikt verboten, es sei denn, es handelt sich um SIMD/FFI, die per `// SAFETY:` Kommentar gerechtfertigt sind.
4. Schalte niemals Clippy-Warnungen über `#allow(...)` ab, es sei denn in Abstimmung für FFI. Behebe alle Lints!

### Phase 5: Validation & Done (Refactor)
1. Führe format, clippy und check aus: `just check`
2. Prüfe auf deterministischen Ablauf: `just triple-test` (Die Tests müssen 3x nacheinander ohne Heisenbugs bestehen).
3. Ändere den Status des WP in `AGENTS.md` (oder rufe den Benutzer zur Abnahme).

---

## 🛤️ 4. Ausführungs-Roadmap: Vorrangschaltung (Prioritäten)

*LLM Agent, verfolge strikt diese Reihenfolge bei der Abarbeitung:*

1. **PHASE 0: Tech Debt Annihilation (WP-0.0)**
   - Eliminierung jeglicher Altlasten. Refactoring hin zu 100% Zero-Panic und asynchronem I/O.
2. **PHASE 1: Core Stabilität & LSM (WP-1.1, WP-1.2, WP-4.1)**
   - Implementierung der Background Compaction für die LSM Trees. Storage und Namespaces müssen ohne Memory Leaks und Tombstone-Überschreitungen laufen.
3. **PHASE 2: Hybrid Search & RAG Pipeline (WP-2.1, WP-2.2, WP-7.1)**
   - Aufbau des Inverted Text-Indexes (BM25) und SIMD Quantization (SQ8). Implementierung des Markdown Chunker für semantisches Retrieval.
4. **PHASE 3: Python API, Security & Connectivity (WP-3.1, WP-3.2, WP-7.3)**
   - Bereitstellung der `PyO3` Bindings. Implementierung von Encryption-at-Rest und MCP-Server Support.
5. **PHASE 4: Hyper-Scale & Persistence (WP-4.2, WP-4.3, WP-7.2)**
   - Out-of-Core Vector Search. HNSW Persistence via mmap zur Eliminierung von RAM-Bottlenecks und Cold-Starts.

*End of Spec - Agent, please acknowledge and proceed.*
