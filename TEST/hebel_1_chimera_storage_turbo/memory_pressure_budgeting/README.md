# Memory Pressure & Resource Budgeting (SPEC-025 / SPEC-032) — Integration Guide für MemFuse

## 1. Technischer Hintergrund & Synergie
MemFuse wird primär auf lokaler Entwickler- und Edge-Hardware eingesetzt (z. B. auf Laptops mit Ollama, Tauri-Desktop-Apps oder Edge-Servern mit 8–16 GB RAM). Unter hoher Last durch autonome LLM-Agenten (hunderte parallele Kontext-Updates, parallele Vektor-Embeddings, HNSW-Graph-Modifikationen) besteht akute Gefahr von **Out-of-Memory (OOM) Crashes**, wenn Schreibvorgänge ungebremst den Heap fluten.

**Project Chimera** hat dieses Problem mit einer formalen Budgeting-Architektur spezifiziert und implementiert:
- **Lock-Free Atomic Tracking (SPEC-032):** Kein Mutex-Lock im Allokationspfad. `ResourceTracker` nutzt `AtomicU64` mit Compare-and-Swap (`compare_exchange_weak`), um Speicherverbrauch atomar zu limitieren.
- **3-Stufen-Statusmodell:**
  1. `BudgetStatus::Normal` (< 80% des Limits): Ungebremster Betrieb.
  2. `BudgetStatus::Stall` (80% – 95% des Limits): Asynchrones Backpressure/Write-Stalling verlangsamt neue Writes, damit Flush/Compaction hinterherkommen.
  3. `BudgetStatus::Reject` (> 95% des Limits): Harter Schutz gegen OOM-Killer. Nicht-kritische Schreibvorgänge werden mit `ChimeraError::BudgetExceeded` (in MemFuse: `MemFuseError::MemoryBudgetExceeded`) sofort abgewiesen.
- **Domänen-Partitionierung & Adaptive Allocator:** Dynamische Umverteilung von Speicherbudgets zwischen HNSW-Index, Storage-MemTable, Metadaten und Cache.

## 2. Extrahierte Chimera-Komponenten

| Datei | Quelle | Relevanz für MemFuse |
|:---|:---|:---|
| [`budget.rs`](./budget.rs) | `chimera-core/src/budget.rs` | Lock-Free `ResourceTracker`, `ResourceBudget`, `Domain` Partitionierung |
| [`adaptive_allocator.rs`](./adaptive_allocator.rs) | `chimera-core/src/adaptive_allocator.rs` | Dynamische adaptive Speicherumverteilung basierend auf Nutzungsgrad |
| [`SPEC-025_memory_pressure.md`](./SPEC-025_memory_pressure.md) | `docs/specs/SPEC-025_memory_pressure.md` | Formale Spezifikation für Memory Pressure & OOM-Resilienz |
| [`SPEC-032_resource_budget.md`](./SPEC-032_resource_budget.md) | `docs/specs/SPEC-032_resource_budget.md` | Spezifikation zur Durchsetzung von Speicher- und CPU-Limits |
| [`SPEC-048_physical_memory_invariants.md`](./SPEC-048_physical_memory_invariants.md) | `docs/specs/SPEC-048_physical_memory_invariants.md` | Physikalische Speicherinvarianten (INV-P1 bis INV-P5) |

## 3. Kern-Code-Auszug: Lock-Free Atomic Resource Tracking
Aus [`budget.rs`](./budget.rs):
```rust
impl ResourceTracker {
    pub fn try_allocate(&self, bytes: u64) -> Result<()> {
        let limit = self.budget.memory_limit;
        let mut current = self.memory_used.load(Ordering::Relaxed);
        loop {
            let new = current.saturating_add(bytes);
            if new > limit {
                return Err(ChimeraError::BudgetExceeded {
                    domain: "Global".to_string(),
                    used: new,
                    limit,
                });
            }
            match self.memory_used.compare_exchange_weak(
                current,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.memory_peak.fetch_max(new, Ordering::Relaxed);
                    return Ok(());
                }
                Err(actual) => current = actual,
            }
        }
    }
}
```

## 4. Implementierungsplan für MemFuse
1. Kopiere `budget.rs` in `crates/memfuse-core/src/budget.rs`.
2. Hänge `ResourceTracker` in die `Collection`- und `StorageEngine`-Instanzen von `memfuse-db` ein.
3. Vor jedem `insert()` oder `bulk_insert()`: `tracker.try_allocate(payload_size)`.
4. Bei Erreichen von 80% Speichernutzung: Auslösen von `tokio::time::sleep` (Write Throttling), bei > 95%: Rückgabe von `MemFuseError::MemoryBudgetExceeded`.
