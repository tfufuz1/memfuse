# MemFuse — Priorisierter Fix-Plan

**Basis:** Audit vom 23.08.2026, Commit `d96daf1`
**Ziel:** Von den im Audit gefundenen Befunden zu einer stabilen, effizienten RAG-Engine

---

## Phase 1 — Funktionsbrecher (vor jedem produktiven Einsatz zwingend)

### 1.1 MCP-Server: Nullvektor-Bug beheben
**Datei:** `crates/memfuse-mcp/src/lib.rs:166`
**Aufwand:** Mittel (0.5–1 Tag) — Architekturentscheidung nötig, kein reiner Bugfix

**Problem:** `memfuse_insert` schreibt `vec![0.0; dim]` statt eines echten Embeddings. Vektorsuche über MCP-eingefügte Dokumente ist wirkungslos.

**Empfohlener Fix:**
1. Ollama-Client aus `memfuse-tauri/src/ollama.rs` in einen eigenen, wiederverwendbaren Crate extrahieren (z. B. `crates/memfuse-ollama`), damit sowohl `memfuse-tauri` als auch `memfuse-mcp` ihn nutzen können, ohne Code zu duplizieren oder eine App-Binary als Library-Dependency zu missbrauchen.
2. `McpServerState` um `embedder: Arc<dyn EmbeddingProvider>` erweitern.
3. `handle_insert` auf `state.embedder.embed(text).await?` umstellen.
4. Bis Schritt 1–3 umgesetzt sind: **Kurzfristiger Notfall-Fix** — `memfuse_insert` mit klarem Fehler ablehnen, wenn kein Embedder konfiguriert ist, statt still einen Nullvektor zu persistieren. Ein falscher, aber sichtbarer Fehler ist besser als silent data corruption.

```rust
// Sofortmaßnahme, falls Embedder-Integration noch nicht fertig ist:
return Err("memfuse_insert: kein Embedder konfiguriert — Vektorsuche würde fehlschlagen".into());
```

### 1.2 `namespace.rs` reparieren oder entfernen
**Datei:** `crates/memfuse-db/src/lib.rs`, `crates/memfuse-db/src/namespace.rs`
**Aufwand:** Klein (Entfernen) bis Mittel (Anbinden)

**Entscheidung nötig:** Wird Multi-Tenant-Namespace-Isolation für das KMU-Zielbild gebraucht (z. B. Trennung HR/Vertrieb/Geschäftsleitung)?

- **Falls ja:** `pub mod namespace;` in `lib.rs` ergänzen, fehlende Variante `MemFuseError::NamespaceViolation` in `memfuse-core/src/error.rs` nachtragen, `NamespaceRegistry` tatsächlich in den Collection-Zugriffspfad verdrahten, Cross-Namespace-Checks bei jedem `hybrid_search`/`insert` aufrufen.
- **Falls nein:** Datei löschen, um toten Code und künftige Verwirrung zu vermeiden.

### 1.3 `FusionWeights` an die Fusion anschließen
**Dateien:** `crates/memfuse-db/src/fusion.rs`, `crates/memfuse-db/src/collection.rs:940-1027`
**Aufwand:** Mittel (1–2 Tage inkl. Tests)

**Fix:** `hybrid_search()` um einen `weights: Option<FusionWeights>`-Parameter erweitern. Zwei Wege:
- **Einfach:** Gewichtete Summe der Einzel-Scores statt reinem RRF (verlangt normalisierte Scores pro Signal — Vektor-Distanz vs. BM25-Score vs. Graph-Decay-Score sind aktuell nicht auf derselben Skala, das muss mit gelöst werden).
- **RRF-kompatibel:** Gewichteter RRF, bei dem der Beitrag jedes Signals mit dem jeweiligen `FusionWeights`-Faktor multipliziert wird, bevor aufsummiert wird — ändert die RRF-Formel minimal, bleibt aber rank-basiert und robust gegenüber unterschiedlichen Score-Skalen.

```rust
// fusion.rs — gewichtete Variante
pub fn weighted_reciprocal_rank_fusion(
    result_sets: Vec<(Vec<SearchResult>, f32)>, // (Ergebnisse, Gewicht)
    max_results: usize,
) -> Vec<SearchResult> {
    let k = 60;
    let mut fused: HashMap<String, (f32, Option<serde_json::Value>)> = HashMap::new();
    for (cur_set, weight) in result_sets {
        for (rank, cur_doc) in cur_set.into_iter().enumerate() {
            let score = weight / ((k + rank + 1) as f32);
            let entry = fused.entry(cur_doc.id).or_insert((0.0, cur_doc.metadata));
            entry.0 += score;
        }
    }
    // ... Rest wie gehabt
}
```

Danach `HybridQuery`/`FusionWeightsBuilder` tatsächlich bis in `Collection::hybrid_search` und die Python-Bindings durchreichen.

---

## Phase 2 — Effizienz für Produktions-Workloads

### 2.1 Ingestion parallelisieren
**Datei:** `crates/memfuse-tauri/src/ingestion/pipeline.rs:74`
**Aufwand:** Klein (Stunden) — größter Effizienzgewinn pro Aufwand im gesamten Projekt

```rust
use futures_util::stream::{self, StreamExt};

let results: Vec<_> = stream::iter(chunks.into_iter().enumerate())
    .map(|(idx, chunk)| {
        let embedder = self.embedder.clone();
        async move {
            let embedding = embedder.embed(&chunk.content).await;
            (idx, chunk, embedding)
        }
    })
    .buffer_unordered(8) // Konfigurierbare Konkurrenz, z.B. via Settings
    .collect()
    .await;
```
Reduziert die Ingestion-Zeit für Dokumente mit vielen Chunks um einen Faktor nahe der gewählten Konkurrenz (z. B. 5–8×), ohne Ollama selbst ändern zu müssen.

### 2.2 `memfuse-embed`: Blockierende Inferenz von Tokio entkoppeln
**Datei:** `crates/memfuse-embed/src/lib.rs:26-28`
**Aufwand:** Klein

```rust
#[async_trait]
impl TextEmbeddingEngine for TextEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.to_string();
        let this = /* Arc<Self> erforderlich, falls nicht bereits vorhanden */;
        tokio::task::spawn_blocking(move || this.embed(&text))
            .await
            .map_err(|e| MemFuseError::Internal(format!("Embedding task panicked: {e}")))?
    }
}
```
Voraussetzung: `TextEmbedder` muss `Arc`-fähig aufgerufen werden können (Session ist bereits `Mutex`-geschützt, das passt).

### 2.3 MemTable: Sharding wie bereits bei `TxBuffer` vorbildlich vorgemacht
**Datei:** `crates/memfuse-store/src/memtable.rs`
**Aufwand:** Groß (2–4 Tage inkl. Migrationstests, da MVCC-Semantik über Shard-Grenzen sauber bleiben muss)

Vorschlag: `MemTable` intern in N Shards (z. B. nach Hash des Keys) aufteilen, jeder mit eigenem `RwLock<BTreeMap>`. `get_at_seq`/`iter`/`iter_latest` müssen über alle Shards aggregieren — Mehraufwand für globale Operationen, aber deutlich weniger Lock-Contention beim parallelen Schreiben, was bei Massen-Ingestion (Kernszenario!) der eigentliche Engpass ist. Das im Projekt bereits vorhandene `TxBuffer`-Sharding-Muster (`memfuse-core/src/tx_buffer.rs`) ist eine gute Vorlage.

### 2.4 `flush()`: Lesbarkeit während Flush erhalten
**Datei:** `crates/memfuse-store/src/lsm.rs:692-693`
**Aufwand:** Mittel

Der exklusive `state.write().await` zu Beginn von `flush()` blockiert `get()` unnötig lange. Da `old_memtable` bereits atomar via `std::mem::replace` isoliert wird (Zeile 705), könnte der Write-Lock nur für den kurzen Replace-Moment gehalten werden, während das eigentliche SSTable-Schreiben (Zeilen 716-732, der teure Teil) außerhalb jedes Locks läuft — das ist im Code strukturell bereits fast so angelegt (`drop(state)` bei Zeile 714), sollte aber verifiziert und mit einem gezielten Lock-Contention-Benchmark abgesichert werden.

### 2.5 ARM/NEON-Pfad ergänzen
**Datei:** `crates/memfuse-index/src/distance.rs`
**Aufwand:** Groß (spezialisiertes SIMD-Wissen nötig, 3–5 Tage inkl. Determinismus-Tests gegen den bestehenden `±1e-6`-Toleranztest)

Relevant, weil die Desktop-App (`memfuse-tauri`) explizit auf Endanwender-Hardware zielt und Apple Silicon (M-Chips) im KMU-Umfeld verbreitet ist. Vorlage: dieselbe Tiered-Fallback-Struktur wie AVX2/AVX-512, mit `std::arch::aarch64::*` und `#[target_feature(enable = "neon")]`.

### 2.6 SSTable-Prefix-Scan: Key-Range-Vorabcheck
**Datei:** `crates/memfuse-store/src/lsm.rs` (Aufrufstelle von `scan_prefix`)
**Aufwand:** Klein

Vor jedem `sst.scan_prefix(prefix)`-Aufruf prüfen, ob `prefix` überhaupt im `[first_key, last_key]`-Bereich der SSTable-Metadaten liegen kann (analog zum bereits vorhandenen Check in `SstableReader::get()`, Zeile 944). Vermeidet unnötige Binary-Searches auf offensichtlich irrelevanten SSTables — hilft besonders der BM25-Textsuche, die denselben Pfad für jede Posting-Liste nutzt.

### 2.7 Token-Schätzung für Deutsch kalibrieren
**Datei:** `crates/memfuse-db/src/context.rs:118-123`
**Aufwand:** Klein bis Mittel

```rust
pub fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    // TODO: Sprache erkennen oder konfigurierbar machen.
    // Deutsch (Komposita, Subword-Tokenizer-Verhalten) braucht einen höheren Faktor als Englisch.
    let factor = 1.6; // statt 1.3 — empirisch gegen echten Ollama-Tokenizer validieren
    ((words as f64) * factor).ceil() as usize
}
```
Besser noch: den tatsächlichen Tokenizer des konfigurierten Ollama-Modells anfragen (`/api/show` liefert z.T. Tokenizer-Infos) oder eine lokale `tiktoken`-artige Bibliothek für eine echte Zählung statt Heuristik nutzen — besonders wichtig, da harte Chunk-Limits das Kontextfenster des lokalen LLM nicht überschreiten dürfen.

---

## Phase 3 — Aufräumen & Dokumentationskonsistenz

| Punkt | Datei | Aufwand |
|---|---|---|
| `HnswConnectivityDegraded`-Variante entfernen oder tatsächlich auslösen (aktuell toter Code, falsches `%`-Format) | `memfuse-core/src/error.rs:58-59` | Klein |
| u8-Overflow in `compute_u8` beheben (eigenes TODO des Teams) | `memfuse-core/src/types/domain.rs:224-270` | Klein — `u32`→`u64`-Akkumulation oder Dimension-Grenze validieren |
| `memfuse-checkpoint`: "FROZEN/SAOS"-Kommentar korrigieren, da Crate aktiv genutzt wird | `memfuse-checkpoint/src/lib.rs:1-8` | Trivial |
| ADR-008 um den tatsächlichen Ersatz (Ollama) und die Kosten-Nutzen-Abwägung ergänzen | `DECISIONS.md` | Trivial |
| MCP-Tool-Beschreibung "vector + BM25 + metadata" korrigieren (tatsächlich: vector + BM25 + graph) | `memfuse-mcp/src/lib.rs:26` | Trivial |
| `GermanCompoundSplitter`-Doku klarstellen: Input muss lowercased sein | `memfuse-text/src/morphology.rs:71` | Trivial |
| `delete_prefix`-Default-Implementierung durch echten Batch-Tombstone ersetzen (betrifft `traits.rs` und `lsm.rs` gleichermaßen) | `memfuse-core/src/traits.rs:89-97`, `memfuse-store/src/lsm.rs:538-546` | Mittel |

---

## Empfohlene Reihenfolge

1. **1.1 → 1.3** (Funktionsbrecher) — ohne diese ist "RAG-Engine" im MCP-Kontext und bei gewichteter Suche irreführend beworben.
2. **2.1** (Ingestion-Parallelisierung) — größter Effizienzgewinn, kleinster Aufwand, sollte direkt nach Phase 1 kommen.
3. **2.2, 2.6, 2.7** — kleine, unabhängige Verbesserungen, gut parallel zu anderer Arbeit erledigbar.
4. **1.2** (Namespace-Entscheidung) — Architekturentscheidung treffen, dann klein umsetzen.
5. **2.3 – 2.5** — größere strukturelle Arbeiten, für spätere Sprints, wenn Nutzungsdaten (echte Lastprofile) zeigen, dass sie den Engpass darstellen.
6. **Phase 3** — laufend nebenbei, senkt Onboarding-Aufwand für neue Contributor spürbar.
