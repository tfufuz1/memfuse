# AGENTS.md — memfuse-index
> Layer 1 | HNSW Vektor-Index, SIMD Distanzen, SQ8-Quantisierung | ~10800 LOC

## 1. Zweck & Architekturrolle

Vektorsuch-Engine des MemFuse-Systems (Signal 1 der 4-Signal-Fusion). Implementiert 
Hierarchical Navigable Small World (HNSW) Graphen, 8-Bit Skalar-Quantisierung (SQ8) und
Hardware-beschleunigte SIMD-Distanzmetriken (AVX-512, AVX2, NEON).
Implementor des `VectorIndex` Traits aus `memfuse-core`.

**Invariante:** HNSW-Graphen liegen primär im RAM und werden asynchron via Checkpoints persistiert, 
bzw. memory-mapped geladen (via `persistence::MmapIndex`).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklaration, `#![deny(unsafe_code)]` mit strikten Ausnahmen |
| `hnsw.rs` | `HnswIndex`, Graph-Traversal, Layer-Verwaltung, Heuristic Node Selection |
| `distance.rs` | SIMD-Distanz-Intrinsics (Cosine, Euclidean, DotProduct), Hardware-Dispatch |
| `quantize.rs` | `ScalarQuantizer` (SQ8), Asymmetrische/Symmetrische Distanz-Approximation |
| `persistence.rs` | `MmapIndex`, `HnswHeader`, Binärformat-Serialisierung & Memory-Mapping |
| `diskann.rs` | DiskANN Out-of-Core-Suche (hinter `experimental-diskann` Feature-Gate) |

> `experimental-diskann` is opt-in. Enable with `features = ["experimental-diskann"]`.
> Do NOT add it to `default` — it remains experimental by definition.

## 3. Kritische Invarianten

### SIMD-Hardware-Dispatch
Distanzberechnung in `distance.rs` wählt zur Laufzeit die besten verfügbaren Intrinsics.
Hierarchie: **AVX-512 > AVX2 > NEON > Skalar**.
Fallback auf Skalar muss immer exakt die gleichen mathematischen Ergebnisse liefern.

### unsafe Scope & Mmap (ADR-017)
- `distance.rs`: Darf `#![allow(unsafe_code)]` verwenden (für SIMD-Intrinsics).
- `diskann.rs` & `persistence.rs`: Dürfen exakt ein `unsafe { Mmap::map(&file) }` enthalten.
- **Pflicht:** Jedes `unsafe` MUSS von einem `// SAFETY:` Beweis-Kommentar begleitet sein.
- Modul-weites `#![allow(unsafe_code)]` ist im gesamten Crate **VERBOTEN**.

### Atomic Rename Pattern (File Writes)
Wie in `memfuse-store` (aber hier für Index-Snapshots):
1. Schreibe nach `.tmp`
2. `fsync` die Datei
3. `rename(tmp, final)`
4. `fsync` das Parent-Directory

### bounds-checked Neighbor Load
Beim Laden von Graph-Knoten (`load_node`) MUSS `neighbor_count` gegen `max_degree` (`M` / `M_max0`) 
geprüft werden. Ein Out-of-Bounds bedeutet Datei-Korruption -> `Err` zurückgeben, nie stumm abschneiden.

### Quantisierungs-Drift
`ScalarQuantizer` muss bei jedem Insert den Wertebereich prüfen (`expand_bounds_to_fit`).
Ein Drift von >15% löst asynchronen Index-Rebuild aus.

## 4. Public API Quick-Reference

```rust
// === HnswIndex (hnsw.rs) — Implementiert VectorIndex ===
pub struct HnswIndex { ... }
impl HnswIndex {
    pub async fn open(config: HnswConfig) -> Result<Self>;
    // Traits: insert, search, search_at, stats
}

pub struct HnswConfig {
    pub dimension: usize,
    pub m: usize,                  // Max edges per node (>0)
    pub ef_construction: usize,    // Search depth during insertion
    pub space: DistanceMetric,     // Cosine, Euclidean, DotProduct
}

// === Quantization (quantize.rs) ===
pub struct ScalarQuantizer { ... }
impl ScalarQuantizer {
    pub fn try_train(batch: &[&[f32]], dimension: usize) -> Result<Self>;
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8>;
    pub fn asymmetric_dist(&self, q_raw: &[f32], v_quant: &[u8]) -> f32;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — unsafe ohne SAFETY-Kommentar (Gate 4 schlägt an!):
unsafe { std::arch::x86_64::_mm256_loadu_ps(ptr) }
// ✅ KORREKT:
// SAFETY: caller guarantees ptr points to 8 floats
unsafe { std::arch::x86_64::_mm256_loadu_ps(ptr) }

// ❌ FALSCH — HNSW-Parameter hart codieren:
let conf = HnswConfig { m: 16, ef_construction: 200, ... };
// ✅ KORREKT — Aus DB-Config oder defaults laden.

// ❌ FALSCH — DiskANN in Produktion integrieren:
let idx = DiskAnnIndex::open(...); // Nur experimentell!
// ✅ KORREKT — DiskANN bleibt isoliert hinter feature-gate.

// ❌ FALSCH — Snapshot-Isolation bei search_at ignorieren:
// ✅ KORREKT — Visibility (SequenceLog) bei HNSW-Suche prüfen.
```

## 6. Concurrency & Lock-Hierarchie

`HnswIndex` verwendet stark konkurrierendes Lock-Free- bzw. feingranulares Locking (via `parking_lot`).
- `RwLock` auf dem Knoten-Array: Darf nur cực kurz gehalten werden.
- NIEMALS `tokio::sync::RwLock` im HNSW-Hotpath verwenden (nur `parking_lot`).
- Locks niemals über `.await`-Punkte halten. (Verletzung == Deadlock bei Vektorsuche).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0), `memfuse-graph` (L1 Peer, via `#[cfg(feature="graph")]` für HNSW-Wissensgraph-Hybride)
- **Verbotene Imports**: `memfuse-db` (L2), `memfuse-store` (L1 Peer — wir speichern Indexdateien direkt)
- **Implementiert**: `VectorIndex` aus `memfuse-core`.

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-017 | `unsafe`-Einschränkungen (nur SIMD/Mmap) |
| ADR-013 | `experimental-diskann` Feature-Flag |
| `rules/simd_safety.md` | Fallbacks und SIMD-Intrinsics |
| `rules/async-io.md` | Datei-Operationen für Index-Speicherung |
