# MemFuse Implementierungsplan: Phase B (Produktions-Skalierung)

> [!NOTE]
> **Kontext**
> MemFuse hat Phase A (Single-Node Stabilität) erfolgreich abgeschlossen und besitzt eine panikfreie, sichere "Sovereign Core" Architektur. Dieser Plan skizziert die technischen Schritte, um die horizontale Verteilung und Skalierung in **Phase B** umzusetzen, ohne die systemischen Invarianten der Architektur zu verletzen.

---

## Übersicht der Workstreams (Phase B)

```mermaid
gantt
    title MemFuse Phase B Roadmap
    dateFormat  YYYY-MM-DD
    axisFormat  Q%q
    
    section 1. Consensus
    REP-001: Raft Integration (memfuse-cluster) :2026-07-01, 30d
    REP-002: RPC & Orchestration                 :2026-07-15, 30d
    
    section 2. Performance
    SIMD-001: AVX-512 u8 Kernels (memfuse-index)  :2026-08-01, 20d
    IPC-001: FlatBuffers Zero-Copy                :2026-08-15, 20d
    
    section 3. Features
    EMB-001: ONNX Runtime (memfuse-embed)         :2026-09-01, 25d
```

---

## 1. Verteilte Replikation (REP-001/002)

**Ziel:** Raft-basierte Konsensfindung für Hochverfügbarkeit über mehrere Server-Knoten hinweg (Leader-Follower Architektur).
**Zuständig:** `memfuse-cluster` (Neue Crate)

### Architektur-Spezifikation
MemFuse wird von einem Single-Node-System in ein verteiltes System mit starker Konsistenz überführt. Der Raft-Konsens-Algorithmus steuert künftig das Write-Ahead-Log (WAL). 

> [!IMPORTANT]
> **WAL-First Invariante** bleibt erhalten: Writes werden zuerst über den Raft-Cluster repliziert, bevor sie in der MemTable (und SSTable) des jeweiligen Knotens sichtbar werden.

### Implementierungsschritte
1. **Crate Setup (`memfuse-cluster`)**:
   - Initialisierung gemäß Sovereign Core Doctrine (`#![forbid(unsafe_code)]`).
   - Integration des `openraft` Crates als zuverlässiger Raft-Framework-Standard.
2. **State Machine Anbindung**:
   - Das vorhandene `Wal` und `LsmStorage` (aus `memfuse-store`) wird als Underlying State Machine in `openraft` implementiert. 
   - Einbindung des bestehenden `memfuse-checkpoint` Mechanismus für Raft-Log-Compaction (Snapshotting).
3. **Netzwerk-Protokoll (gRPC & TLS)**:
   - Implementierung des `RaftNetwork` via `tonic` (gRPC). 
   - Absicherung der Kommunikation via mTLS (Mutual TLS). Hardcodierte Zertifikate/Keys sind verboten.
4. **Leader-Routing & Orchestrierung (`memfuse-db`)**:
   - API-Aufrufe (Insert/Update/Delete) werden vom Cluster-API-Gateway abgefangen und asynchron an den Leader weitergeleitet.
   - Read-Scale-Out: Lesezugriffe können (unter Inkaufnahme eines konfigurierbaren Consistency Levels / Stale reads) an Follower delegiert werden.

---

## 2. Zero-Copy IPC (IPC-001/002)

**Ziel:** Vermeidung redundanter Speicherkopien beim Datenaustausch zwischen Rust-Core und Agenten / Sandboxen via FlatBuffers.
**Zuständig:** `memfuse-py`, `memfuse-sandbox`

### Implementierungsschritte
1. **Schema-Design (`.fbs`)**:
   - Definition der FlatBuffer-Schemata für kritische Datentypen: `ScoredDocument`, `Embedding`, `VectorIndexUpdate`, und Graph-Einträge.
2. **Tooling & Build-Logik**:
   - Integration des `flatbuffers` Rust-Crates und Setup in `build.rs` für die automatische Neu-Kompilierung von Schema-Änderungen.
3. **Python Bindings (`memfuse-py`)**:
   - Anpassung der Pyo3 Returns. Statt tiefes JSON/Dict-Copy der Vektor- und Dokumentendaten zu generieren, liefert Rust direkt FlatBuffer-Bytes, die via Python `memoryview` null-kopierend (zero-copy) von Numpy / Pydantic verarbeitet werden.
4. **Sandbox Host Functions (`memfuse-sandbox`)**:
   - Die in Phase A via JSON/MessagePack mock-implementierten Serialisierungen in Wasmtime (AirGap Host Funktionen) werden durch FlatBuffer Pointer ersetzt. Reduktion der Deserialisierungs-Latenz auf O(1).

---

## 3. In-Process Embeddings (EMB-001/002)

**Ziel:** Automatische Erzeugung von Dokument-Embeddings innerhalb des nativen Memory-Spaces mittels ONNX Runtime, ohne externe REST/API Call Hops.
**Zuständig:** `memfuse-embed` (Neue Crate) und `memfuse-py`

### Implementierungsschritte
1. **Crate Setup (`memfuse-embed`)**:
   - Neue Crate mit Abhängigkeit zu `ort` (Rust bindings für ONNX Runtime). 
   - Die Runtime wird statisch gelinkt (Execution Provider: primär CPU, optional CUDA via feature-flag).
2. **Modell-Management (`hf-hub`)**:
   - Asynchroner Download / Caching von Standardmodellen (z.B. `all-MiniLM-L6-v2`, `E5-small`) ins lokale Dateisystem (verschlüsselt falls System-Policy greift).
3. **Rust Tokenizer / Pipeline**:
   - Kombination des bestehenden `memfuse-text` Tokenizers (oder des `tokenizers` Crates) zur Erzeugung der Token-IDs + Attention Masks, gefolgt von der Forward-Pass Inferenz im ONNX Model (Mean-Pooling als Standard-Strategy).
4. **DB-Integration (`memfuse-db` & `memfuse-py`)**:
   - Die `memfuse-db::Collection` erhält eine `insert_text_only` (oder auto-embed) API. Ist ein Embeddingmodell referenziert, generiert das System den Feature-Vector und den BM25 Text synchron.

---

## 4. Hardware-Optimierung (SIMD-001/002)

**Ziel:** Ausreizen der Hardware-Fähigkeiten durch extrem schnelle `AVX-512` Intrinics für 8-bit Quantisierte Daten (SQ8).
**Zuständig:** `memfuse-index`

> [!WARNING]  
> **SIMD Exception Policy**
> Das Schreiben von `unsafe` Code ist für das Projekt untersagt. Die **einzige Ausnahme** bildet Crate `memfuse-index` speziell für SIMD Instruktionen. Es gilt: Jeglicher Unsafe-Code darf nicht mutieren und muss mittels `#![deny(unsafe_op_in_unsafe_fn)]` präzise mit `// SAFETY:` annotiert werden!

### Implementierungsschritte
1. **AVX-512 (VNNI) Kernels (`distance.rs`)**:
   - Implementierung der Dot-Product und L2-Distanz für `u8` Arrays mittels 512-bit Registerbreite (z.B. `_mm512_dpbusd_epi32`). 
   - Diese Instruktion (VNNI) verarbeitet 64x 8-bit Integer (u8) Berechnungen (zuzüglich Accumulation in einem Takt) simultan. 
2. **Dynamic CPU Dispatch**:
   - `std::arch::is_x86_feature_detected!("avx512vnni")` und `"avx2"` Macros nutzen, um zur Laufzeit auf den performantesten Kernel zurückzufallen, ohne separate Binares ausliefern zu müssen.
3. **Fallback-Sicherung**:
   - Skalare Rust-Iterators (die Auto-Vektorisierung durch LLVM erfahren) bleiben als garantierte Fallback-Baseline (speziell für ARM64/Apple Silicon) bestehen.
4. **Distance Benchmarking (`criterion`)**:
   - Recall- und Throughput-Zahlen der SQ8 Distance Metrics (AVX-512 vs AVX2 vs Scalar) unter Continuous Integration via `just triple-test` festhalten. Regressionen werden als build-breaker formuliert.
