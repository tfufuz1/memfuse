# Hebel 1: Technologische Beschleunigung durch Project Chimera

Dieser Ordner enthält alle aus **Project Chimera** extrahierten Hochleistungs-Module, Architektur-Dokumente und Spezifikationen für die Integration in **MemFuse**.

## Übersicht der extrahierten Unterbereiche

```
hebel_1_chimera_storage_turbo/
├── zero_copy_rkyv/             # Zero-Copy Serialisierung mit rkyv (WAL & MVCC Hot-Path)
│   ├── aliased_bytes.rs        # Sicherer bytecheck-geprüfter Zero-Copy Wrapper
│   ├── rkyv_types.rs           # rkyv-kompatible Primitive & Types
│   ├── rkyv_tx_buffer.rs       # Atomic TxBuffer mit Zero-Copy Archive
│   ├── rkyv_hnsw_persist.rs    # HNSW Index Persistenzstrukturen
│   ├── rkyv_metadata_index.rs  # Metadaten-Index mit rkyv Speicherformat
│   ├── rkyv_lsm_storage.rs     # LSM MemTable & Payload Zero-Copy Serialisierung
│   └── README.md
├── simd_vector_dispatch/       # SIMD Kernel: AVX-512 + AVX2 + portable-simd
│   ├── distance.rs             # Vollständiger Vektordistanz-Dispatcher & Intrinsics
│   ├── distance_bench.rs       # Criterion Benchmarks (Cosine, Euclidean, Dot)
│   ├── SPEC-001_simd_distance.md
│   ├── ADR-011_distance_dispatcher.md
│   └── README.md
├── memory_pressure_budgeting/  # Edge-Memory-Pressure & OOM-Resilienz (SPEC-025 / SPEC-032)
│   ├── budget.rs               # Lock-Free Atomic ResourceTracker & Budget
│   ├── adaptive_allocator.rs   # Dynamischer Speicher-Allokator
│   ├── SPEC-025_memory_pressure.md
│   ├── SPEC-032_resource_budget.md
│   ├── SPEC-048_physical_memory_invariants.md
│   └── README.md
└── tri_hybrid_rrf_query/       # Einheitliche RRF-Fusion (k=60) & Query Planner
    ├── fusion.rs               # Weighted Reciprocal Rank Fusion Engine
    ├── planner.rs              # Hybrid Query Planner mit Short-Circuit Pruning
    ├── hybrid.rs               # Ausführungspipeline
    ├── reciprocal_rank_fusion.md
    ├── 09_query_engine.md
    └── README.md
```

## Sofortige Wirkung auf MemFuse
1. **Latenz-Drop im Hot-Path:** Reduziert Speicher-Allokationen bei Vektorsuchen und Metadaten-Scans um bis zu 80%.
2. **4x–8x Beschleunigung von Vektordistanzen:** Direkte Hardware-Nutzung von AVX-512 und AVX2.
3. **Keine OOM-Crashes auf Edge-Systemen:** Automatisches Write-Stalling und Rejection bei hohem RAM-Druck.
4. **Code-Konsolidierung:** Beseitigung redundanter RRF-Codeblöcke.
