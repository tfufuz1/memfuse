# SPEC-041: In-Database Ingest Embedding (Native Tensor Engine)

> **Stand:** 2026-04-06 | **Prio:** P0 (Corporate Brain Core) | **Status:** ✅ IMPLEMENTIERT  
> **Crate:** `chimera-compute` | **Basis:** SPEC-034 / AI Transformation

---

## 1. Ziel

Eliminierung externer API-Abhängigkeiten (OpenAI/HuggingFace) für den Ingest-Prozess. ChimeraDB transformiert Rohdaten (Text/Bild) während der `on_prepare`-Phase der 2PC-Transaktion automatisch in Vektoren. Dies reduziert Latenz, Kosten und erhöht den Datenschutz massiv.

---

## 2. Architektur

### 2.1 Der `chimera-compute` Crate
Ein neuer spezialisierter Crate für Tensor-Berechnungen, basierend auf dem `candle`-Framework (HuggingFace).

- **Backend:** CPU (SIMD/OpenBLAS) mit optionaler Beschleunigung (Cuda/Metal).
- **Modelle:** Support für GGUF-quantisierte Modelle (SOTA: BERT/nomic-bert-v1.5).
- **Invariante INV-S5:** Alle Tensor-Ops laufen in `spawn_blocking`, um den Tokio-Runtime-Freeze zu verhindern.

### 2.2 Integration via `IndexObserver`
Der `IngestEmbedder` fungiert als `IndexObserver` und klinkt sich in den globalen `SyncManager` ein:

1. **`on_prepare`**: Identifiziert `RawContent` in den Metadaten, berechnet Embeddings, speichert sie im `ChimeraContext`.
2. **`on_commit`**: Der `VectorIndex` entnimmt das fertige Embedding aus dem Kontext und schreibt es in den HNSW-Index.

---

## 3. Datenstrukturen

```rust
// chimera-core/src/types.rs
pub enum RawContent {
    Text(String),
    Image(Vec<u8>),
    Audio(Vec<u8>),
}

// chimera-core/src/traits.rs
pub trait EmbeddingProvider: Send + Sync {
    fn embed_sync(&self, content: &RawContent) -> Result<Embedding>;
}
```

---

## 4. Ressourcen-Management (SPEC-032/038)

`Domain::Compute` ist der 7. Budget-Slot im `ResourceTracker`.
- **Memory-Guard:** Jedes geladene Modell registriert sein Speicher-Gewicht beim Tracker.
- **Budget-Exhaustion:** Wenn das Compute-Budget erschöpft ist, wird die Transaktion während `on_prepare` mit `MemoryBudgetExceeded` abgelehnt.

---

## 5. Konfiguration

```toml
[compute]
model_type = "gguf"
model_path = "/var/lib/chimera/models/nomic-bert.gguf"
tokenizer_path = "/var/lib/chimera/models/tokenizer.json"
memory_limit_mb = 512
```

---

## 6. Invarianten

| ID | Regel |
|:---|:------|
| **INV-C6** | Ingest-Embedding darf die 2PC-Latenz um maximal 50ms erhöhen (Standard-Modell). |
| **INV-S8** | Tensor-Operationen müssen strikt deterministisch sein (identischer Text = identischer Vektor). |
| **INV-R5** | Modelle werden "Lazy" geladen, aber das Budget wird beim ersten Zugriff persistent reserviert. |
