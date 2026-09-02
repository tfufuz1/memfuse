# AGENTS.md — memfuse-text
> Layer 1 | BM25 Inverted Index & Morphologische Tokenisierung | ~5000 LOC

## 1. Zweck & Architekturrolle

Lexikalische Volltextsuch-Engine (Signal 2 der 4-Signal-Fusion). Implementiert einen
transaktionalen Inverted Index (`InvertedIndex`), BM25-Scoring (`Bm25Scorer`) und
eine erweiterte Tokenisierungs-Pipeline mit morphologischer Analyse (insbesondere
für die DACH-Region: `GermanCompoundSplitter`, Umlaut-Normalisierung).
Implementiert den `TextIndex` Trait aus `memfuse-core`.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklaration, `#![forbid(unsafe_code)]`, `Bm25Scorer` Facade |
| `inverted.rs` | `InvertedIndex` — Transaktionaler Index (Persistenz via `StorageEngine`) |
| `bm25.rs` | `BM25` — Score-Berechnung (IDF, TF, doc_len) |
| `tokenizer.rs` | `Tokenizer` Trait, `DefaultTokenizer`, `GermanMorphTokenizer` |
| `morphology.rs` | `GermanCompoundSplitter`, Umlaut-Normalisierung, Stopword-Filterung |

## 3. Kritische Invarianten

### Determinismus der Tokenisierung
Tokenisierung **MUSS** deterministisch sein. Der Query-Pfad muss exakt dieselbe
Tokenisierungs-Pipeline durchlaufen wie der Indexierungs-Pfad. Eine Diskrepanz
führt zu Silent-Recall-Drops (Wörter werden indexiert, aber nicht gefunden).

### Snapshot Isolation (search_at)
`InvertedIndex` implementiert `search_at` und wertet die Sequence Number aus.
Dies erfordert, dass bei der Suche die Index-Einträge aus der LSM-StorageEngine
mit `get_at_seq()` oder `scan_prefix_at()` geladen werden, um MVCC-Korrektheit zu wahren.

### Transaction-Aware Storage
Der `InvertedIndex` persistiert keine eigenen Dateien, sondern nutzt die
`StorageEngine` aus `memfuse-core`. Alle Mutationen (`upsert_document`, `delete_document`)
müssen die `TxId` an den zugrundeliegenden Storage weitergeben, damit sie atomar
mit Vektor- und Graph-Updates committet werden.

## 4. Public API Quick-Reference

```rust
// === Bm25Scorer (lib.rs) — Implementiert TextIndex ===
pub struct Bm25Scorer<S: StorageEngine> { ... }
impl<S> Bm25Scorer<S> {
    pub fn new(storage: Arc<S>, namespace: &str) -> Self;
    // Traits: search, search_at, insert, delete, commit, rollback
}

// === Tokenisierung & Morphologie (tokenizer.rs, morphology.rs) ===
pub trait Tokenizer: Send + Sync {
    fn tokenize(&self, text: &str) -> Vec<String>;
}
pub struct GermanMorphTokenizer { ... }
pub struct GermanCompoundSplitter { ... }
pub fn normalize_umlauts(input: &str) -> String;

// === Inverted Index (inverted.rs) ===
pub struct InvertedIndex<S: StorageEngine> { ... }
impl<S> InvertedIndex<S> {
    pub async fn search_bm25_at(&self, query_tokens: &[String], k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>>;
    pub async fn upsert_document(&self, tx: TxId, doc_id: DocId, text: &str) -> Result<()>;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Tokenizer nur bei Ingestion verwenden:
let tokens = text.split_whitespace(); // im Query-Pfad
// ✅ KORREKT — Denselben Tokenizer für Query und Ingestion:
let tokens = self.tokenizer.tokenize(query_text);

// ❌ FALSCH — Direkter I/O im InvertedIndex:
let file = tokio::fs::File::create("index.dat").await?;
// ✅ KORREKT — Alle Persistenz geht über `self.storage` (LSM).

// ❌ FALSCH — Tombstones (gelöschte Dokumente) im Score ignorieren:
// ✅ KORREKT — resolve_tombstones() oder tombstone-aware scoring nutzen.
```

## 6. Concurrency & Lock-Hierarchie

`Bm25Scorer` delegiert direkt an `InvertedIndex`. MVCC und Lock-Free Storage werden
durch die darunterliegende `StorageEngine` garantiert. Der `InvertedIndex` selbst
hält keine langlebigen Mutexe. Cache-Strukturen (DF-Counts, Avg-Doc-Len) nutzen
`parking_lot::RwLock` für In-Memory-Updates.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-store` (L1 Peer), `memfuse-index` (L1 Peer)
- **Implementiert**: `TextIndex` aus `memfuse-core`

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| `rules/test_quality.md` | Deterministic Search Recall Verification (ANCHOR-TXT-001) |
| ADR-024 | Snapshot Isolation bei `search_at` |
