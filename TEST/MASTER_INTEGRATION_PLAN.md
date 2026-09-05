# MemFuse Master Integration & Extraction Plan (Vollversion)

> **Stand:** 2026-09-05  
> **Zielverzeichnis:** `/home/freddy/Projekte/memfuse/TEST/`  
> **Quellprojekte:** `Project Chimera (chimeraDB)`, `Structured Process Orchestration Spec (GMAS-FACTORY)`, `Atlas Neural Operating System (atlas)`, `TextForge (textforge)`, `Template-Tauri (template-tauri)`  
> **Fokus:** Umfassende Bereitstellung aller relevanten Komponenten zur Vorab-Validierung vor der finalen MemFuse-Implementierung.

---

## 1. Executive Summary & Strategische Vision

Die vertiefte Analyse aller deiner Projekte offenbart insgesamt **7 strategische Hebel** zur massiven Optimierung von **MemFuse**:

| Hebel | Quelle | Technologischer Durchbruch für MemFuse |
|:---|:---|:---|
| **Hebel 1: Chimera Storage Turbo** | `chimeraDB` | AVX-512/AVX2 SIMD-Distanz, `rkyv` Zero-Copy, Lock-Free OOM-Schutz (SPEC-025/032), RRF Fusion ($k=60$) |
| **Hebel 2: Orchestration Governance** | `GMAS-FACTORY` | 4-Phasen CoVe-Gates (T-05), MNC JIT-Scoping, T-07 PDCA Checkpoints, Deterministische 4-Klassen Fehlermatrix |
| **Hebel 3: Vertikale Atlas OS Integration** | `atlas` | MemFuse als universeller L4-Ersatz für SQLite+LanceDB, Multi-Agent Validierungs-Testbett |
| **Hebel 4: In-Process GGUF Tensor Engine** | `chimeraDB` | Candle + GGUF Quantisierung (SPEC-041) für autarkes Embedded RAG ohne ONNX / Ollama |
| **Hebel 5: Chaos Engineering & Resilienz** | `chimeraDB` | 10 Chaos-Szenarien (SPEC-035) zur Validierung von WAL V3 Torn-Writes, Bit-Flips und Crash-Recovery |
| **Hebel 6: Desktop Context Capture & Sandbox** | `textforge` & `template-tauri` | Quell-App-Erkennung (KDE DBus, Wayland, X11) für automatischen Clipboard-Ingest + QuickJS Sandbox |
| **Hebel 7: Live Agent-to-UI (A2UI) Streaming** | `atlas` | Progressives Rendering von RAG-Ergebnissen und Provenance-Karten als native UI-Komponenten |

---

## 2. Vollständiges Komponenten-Inventar im Testordner

```
/home/freddy/Projekte/memfuse/TEST/
│
├── MASTER_INTEGRATION_PLAN.md                      # Dieser Master-Plan
├── README.md                                       # Schnellübersicht & Index
│
├── hebel_1_chimera_storage_turbo/                  # [Hebel 1] Chimera Storage & Index Turbo
│   ├── README.md                                   # Hebel-1 Übersicht & Benchmark-Ziele
│   ├── zero_copy_rkyv/                             # Zero-Copy Serialisierung (WAL & MVCC Hot-Path)
│   │   ├── README.md                               # Integrationsanleitung für MemFuse
│   │   ├── aliased_bytes.rs                        # bytecheck-geprüfter Safe-Casting Wrapper (chimera-core)
│   │   ├── rkyv_types.rs                           # rkyv-fähige Namespace-, Id- und Datentypen
│   │   ├── rkyv_tx_buffer.rs                       # Zero-Copy Transaktionspuffer
│   │   ├── rkyv_hnsw_persist.rs                    # HNSW Index Persistenzstrukturen
│   │   ├── rkyv_metadata_index.rs                  # Metadaten-Index mit Zero-Copy Filterung
│   │   └── rkyv_lsm_storage.rs                     # LSM Storage Payload & MemTable Serialisierung
│   ├── simd_vector_dispatch/                       # SIMD Vektorbeschleunigung (AVX-512, AVX2, portable-simd)
│   │   ├── README.md                               # SIMD Architektur & Dispatch-Details
│   │   ├── distance.rs                             # Vollständige Distanz-Engine (Cosine, L2, Dot)
│   │   ├── distance_bench.rs                       # Criterion Benchmarks (4x–8x Speedup)
│   │   ├── SPEC-001_simd_distance.md               # Spezifikation zu Vektordistanzen
│   │   └── ADR-011_distance_dispatcher.md          # Architecture Decision Record zu O(1) CPUID-Dispatch
│   ├── memory_pressure_budgeting/                  # Edge-Memory-Pressure & OOM-Resilienz
│   │   ├── README.md                               # Leitfaden zur Speicherdrosselung & Load-Shedding
│   │   ├── budget.rs                               # Lock-Free Atomic ResourceTracker & ResourceBudget
│   │   ├── adaptive_allocator.rs                   # Dynamischer Speicherallokator
│   │   ├── SPEC-025_memory_pressure.md             # Spezifikation: Write-Stalling & OOM-Resilienz
│   │   ├── SPEC-032_resource_budget.md             # Spezifikation: Resource Budget Enforcement
│   │   └── SPEC-048_physical_memory_invariants.md  # Physikalische Speicher-Invarianten
│   └── tri_hybrid_rrf_query/                       # Vereinheitlichte RRF-Fusion & Query-Planner
│       ├── README.md                               # RRF (k=60) und Early-Exit Dokumentation
│       ├── fusion.rs                               # Weighted Reciprocal Rank Fusion Engine
│       ├── planner.rs                              # Hybrid Query Planner mit Pruning
│       ├── hybrid.rs                               # Ausführungspipeline
│       ├── reciprocal_rank_fusion.md               # Mathematische Grundlagen zu RRF
│       └── 09_query_engine.md                      # Gesamtarchitektur der Query Engine
│
├── hebel_2_orchestration_governance/               # [Hebel 2] Prozessuale Absicherung (SPO)
│   ├── README.md                                   # Hebel-2 Übersicht & Governance-Regeln
│   ├── cove_gates_t05/                             # CoVe 4-Phasen PR-Gates
│   │   ├── README.md                               # Schutz gegen Context Rot & Drift (z.B. CompactionStrategy)
│   │   ├── cove_verification_contract_t05.md       # Formale Spezifikation T-05 + E-5 Confidence Gate
│   │   └── cove_pr_gate_workflow.yml               # GitHub Actions CI Workflow
│   ├── minimal_necessary_context_mnc/              # JIT Context Loading & Scoping
│   │   ├── README.md                               # MNC Prinzipien & Anti-Halluzinations-Filter
│   │   ├── mnc_injection_template.md               # CE-01 MNC Injektions-Schema
│   │   ├── memfuse_worker_manifest.md              # T-02 Manifest für Worker-Agenten
│   │   └── memfuse_verifier_manifest.md            # T-02 Manifest für Verifier-Agenten
│   ├── deterministic_error_matrix/                 # 4 standardisierte Fehlerklassen
│   │   ├── README.md                               # Standardisierte FFI/IPC Fehlerbehandlung
│   │   ├── error_matrix.yaml                       # Deklarative Matrix (Transient, Logical, Fatal, Architectural)
│   │   └── error_matrix_mapper.rs                  # Rust Mapper für MemFuseError & MemFuseErrorDto
│   ├── metacognitive_checkpoint_t07/               # Per-Step PDCA Checkpoint
│   │   ├── README.md                               # Vermeidung von Silent Error Propagation
│   │   └── metacognitive_checkpoint_t07.md         # T-07 Checkpoint Schema
│   └── reference_specs/                            # Originale Gesamtspezifikationen
│       ├── STRUCTURED_PROCESS_ORCHESTRATION_SPEC.md
│       ├── SPO_MASTER_FRAMEWORK.md
│       ├── TEMPLATES.md
│       └── GitHub-Communication-System-SPO.md
│
├── hebel_3_atlas_os_integration/                   # [Hebel 3] Atlas OS als universelles Testbett
│   ├── README.md                                   # Hebel-3 Übersicht & Integrations-Architektur
│   ├── daab_l4_memory_layer/                       # Originale Atlas DaaB Schicht
│   │   ├── README.md                               # Analyse der Fragmentierung (SQLite + LanceDB)
│   │   ├── core.py, interface.py, models.py        # Kernkomponenten
│   │   ├── hybrid_search.py, memory_manager.py     # Hybrid-Suche & Gedächtnis
│   │   ├── sqlite_manager.py, snapshots.py         # SQLite Pool & Checkpoints
│   │   ├── isolation.py, init_daab.sql, AGENTS.md  # Isolation, DDL-Schema & Doku
│   ├── atlas_memfuse_adapter/                      # MemFuse Adapter für Atlas OS
│   │   ├── README.md                               # Adapter-Architektur & PyO3-Schnittstelle
│   │   ├── memfuse_daab_provider.py                # Drop-in Python Provider für Atlas DaaB
│   │   ├── atlas_rag_memfuse.py                    # FastMCP RAG Server mit MemFuse
│   │   └── atlas_rag_original.py                   # Originaler RAG Server
│   └── realworld_agent_testbed/                    # Realwelt Stresstest & Validierung
│       ├── README.md                               # Testbed Anleitung & Metriken
│       ├── specialized_agents_memfuse.py           # LangGraph Agent Memory Harness
│       ├── stress_test_atlas_memfuse.py            # Paralleler 10-Agenten Stresstest
│       └── specialized_agents_original.py          # Originale Spezialagenten
│
├── hebel_4_gguf_tensor_engine/                     # [Hebel 4] In-Process GGUF & Quantized Tensor Engine
│   ├── README.md                                   # Candle + GGUF Embedding Guide (SPEC-041)
│   ├── model.rs                                    # GgufEmbeddingModel via Candle
│   ├── embedder.rs                                 # Batch-Embedding Engine
│   ├── autolinker.rs                               # Wissensgraph-Autolinker
│   └── SPEC-041_ingest_embedding.md                # Vollständige Spezifikation
│
├── hebel_5_chaos_resilience/                       # [Hebel 5] Chaos Engineering & Crash-Resilienz
│   ├── README.md                                   # Chaos Testing Guide (SPEC-035)
│   ├── chaos_engine.rs                             # 10 Szenarien (Torn Writes, Bit-Flips, OOM)
│   ├── SPEC-035_chaos_engineering.md               # Spezifikation
│   └── memfuse_chaos_test.rs                       # MemFuse-spezifische Test-Suite
│
├── hebel_6_desktop_context_capture_sandbox/        # [Hebel 6] Desktop Context Capture & Sandbox
│   ├── README.md                                   # Quell-App Erkennung & QuickJS Sandbox Guide
│   ├── source_app.rs                               # KDE DBus / Wayland / X11 Quell-App Erkennung
│   ├── clipboard_monitor.rs                        # Event-basierte Zwischenablagen-Überwachung
│   ├── quickjs_sandbox.rs                          # Sichere QuickJS Sandbox mit Timeout-Guard
│   ├── tauri_system_api.md                         # Baukasten System-APIs aus Template-Tauri
│   └── memfuse_clipboard_ingestion.rs              # Auto-Ingest Dienst für MemFuse
│
└── hebel_7_a2ui_streaming_protocol/                # [Hebel 7] Live Agent-to-UI Streaming
    ├── README.md                                   # A2UI Protokoll & Komponentenbäume
    ├── builder.py, models.py, emitter.py           # A2UI Builder, Models & Emitter
    ├── stream_manager.py, QUICK_REFERENCE.md       # Streaming-Manager & Schnellreferenz
    └── a2ui_memfuse_card.py                        # Generator für MemFuse Provenance-Karten
```

---

## 3. Konkreter Fahrplan zur Umsetzung

1. **Sofort-Schritt 1: SIMD & OOM-Budgeting aktivieren**
   - Kopiere [`distance.rs`](./hebel_1_chimera_storage_turbo/simd_vector_dispatch/distance.rs) und [`budget.rs`](./hebel_1_chimera_storage_turbo/memory_pressure_budgeting/budget.rs) nach `memfuse-core`.
2. **Sofort-Schritt 2: CI CoVe-Gate scharf schalten**
   - Kopiere [`cove_pr_gate_workflow.yml`](./hebel_2_orchestration_governance/cove_gates_t05/cove_pr_gate_workflow.yml) nach `.github/workflows/cove_pr_gate.yml`. Löst die `CompactionStrategy`-Kollision.
3. **Schritt 3: Autarkes GGUF-Embedding (Hebel 4)**
   - Candle GGUF Backend aus [`model.rs`](./hebel_4_gguf_tensor_engine/model.rs) in `memfuse-embed` integrieren.
4. **Schritt 4: Auto-Context Ingestion (Hebel 6)**
   - In `memfuse-tauri` Quell-App-Erkennung aus [`source_app.rs`](./hebel_6_desktop_context_capture_sandbox/source_app.rs) einbinden.
5. **Schritt 5: Atlas als produktiven Client verbinden (Hebel 3 & 7)**
   - In Atlas den [`memfuse_daab_provider.py`](./hebel_3_atlas_os_integration/atlas_memfuse_adapter/memfuse_daab_provider.py) aktivieren und Suchtreffer via [`a2ui_memfuse_card.py`](./hebel_7_a2ui_streaming_protocol/a2ui_memfuse_card.py) streamen.
