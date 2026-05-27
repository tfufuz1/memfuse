# MemFuse Master Specification & LLM Playbook
> **Status:** 🔴 ACTIVE - Pflichtlektüre für ALLE Coding Agenten (Jules Accounts)
> **Autor:** Lead Architect

## 🎯 1. Produktvision (Das Endprodukt)
MemFuse ist das "SQLite für KI-Agenten" – eine in-process, einbettbare und extrem performante Vektor- und Hybrid-Suchdatenbank, speziell konzipiert für lokale LLM-RAG-Systeme. Die Architektur erzwingt absolute Stabilität und Speichersicherheit bei der Verarbeitung hochdimensionaler Vektoren und textueller Metadaten.

---

## 🧩 2. Rekursive Systemzerlegung (Top-Down Breakdown)

Das Endprodukt wird iterativ von Coding Agenten gebaut und ist in isolierte Crates (DOMAINS) unterteilt. Es gilt ein striktes Directed Acyclic Graph (DAG) Dependency-Modell.

### Level 3: Interface (User-Facing)
- **`memfuse-py` (WP-3.1)**: PyO3/Maturin basierte Python-Bindings. Dies ist das primäre Interface für Endnutzer. Setzt strikt auf `memfuse-db` auf.

### Level 2: Die Orchestrierung & API-Schicht
- **`memfuse-db` (WP-1.2, WP-4.2)**: Orchestriert die Zugriffe auf den Store, den Vector-Index und den Text-Index. Kapselt die interne Komplexität und abstrahiert die `Collections` (Namespaces). Enthält die Hybrid-Search Facade, die RRF (Reciprocal Rank Fusion) auf die Resultate von `index` und `text` anwendet.
- **`memfuse-checkpoint`**: Snapshot Registry.
- **`memfuse-sandbox`**: WASM Tool Sandbox.
- **`memfuse-saos-agent`**: Task/Workflow Engine.

### Level 1: Die Sub-Engines (Isoliert, kommunizieren NIE direkt miteinander)
- **`memfuse-store` (WP-1.1, WP-4.1)**: Die Persistenzschicht. Implementiert als Log-Structured Merge Tree (LSM) mit Background Compaction. Verwaltet MemTables, Write-Ahead-Logs (WAL) und SSTables. In Zukunft optimiert mit Memory-Mapped I/O.
- **`memfuse-index` (WP-2.2, WP-4.3, WP-7.2)**: Die Vektor-Engine. Implementiert HNSW-Graphen für Approximate Nearest Neighbor (ANN) Search. Beinhaltet SIMD-optimierte Distanzfunktionen und Scalar Quantization (SQ8) für RAM-Reduzierung.
- **`memfuse-text` (WP-2.1)**: Die Volltext-Engine. Stellt einen Inverted Index mit BM25-Scoring zur Verfügung, der mit Tokenizern arbeitet.
- **`memfuse-graph`**: CSR-Graph für Entity-Relation.
- **`memfuse-crypto` (WP-3.2)**: Encryption-at-Rest.

### Level 0: Der Shared Kernel (Das Rückgrat)
- **`memfuse-core` (WP-0.0)**: Enthält `MemFuseError` (für Zero-Panic), `TxBuffer`, `MemBank`, Paging-Strukturen und Snapshot-Isolation (MVCC). Dies ist die einzige Crate, die von allen anderen Domänen importiert werden darf. Keine anderen Crates dürfen Abhängigkeiten aus Level 1 oder 2 haben.

---

## 🤖 3. Der LLM Agent Workflow (The Playbook)

Als Code Agent (z.B. "Jules Account X") bist du verpflichtet, jeden Entwicklungsschritt präzise nach folgendem **Sovereign Core TDD-Loop** auszuführen:

### Phase 1: Context & Debt Audit
Bevor Code geschrieben wird, evaluierst du das System:
1. Prüfe den Workspace: `just debt-audit`
2. **STOPP**: Wenn der `debt-audit` fehlschlägt, ist das Beheben dieser Schulden Prio 1! **Kein neues Feature vor Null-Schuld.**
3. Identifiziere dein aktuelles Work Package (WP).

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
3. Ändere den Status in `AGENTS.md`.

---

## 🛤️ 4. Ausführungs-Roadmap: Vorrangschaltung (Prioritäten)

*LLM Agent, verfolge strikt diese Reihenfolge bei der Abarbeitung:*

1. **PHASE 1: Foundation (WP-0.0, WP-1.x)**
   - Eliminierung jeglicher Altlasten. Storage (LSM) und Namespaces.
2. **PHASE 2: Search & Retrieval (WP-2.x)**
   - Hybrid Search (BM25) + SQ8 Quantization.
3. **PHASE 3: Interface & Security (WP-3.x)**
   - Python API und Encryption.
4. **PHASE 4: Hyper-Scale (WP-4.x)**
   - Out-of-Core Operations, mmap.
5. **PHASE 5-7: SAOS, GS, RAG**
   - Aktuell eingefroren.

*End of Spec - Agent, please acknowledge and proceed.*
