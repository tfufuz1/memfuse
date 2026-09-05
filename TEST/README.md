# MemFuse TEST & Evaluation Environment (Vollversion)

Willkommen im zentralen Test- und Evaluierungsordner für die **MemFuse-Optimierung**.

Hier wurden alle relevanten Komponenten aus deinen Projekten:
1. **Project Chimera (`chimeraDB`)**
2. **Structured Process Orchestration Spec (`GMAS-FACTORY`)**
3. **Atlas Neural Operating System (`atlas`)**
4. **TextForge (`textforge`)**
5. **Template-Tauri (`template-tauri`)**

isoliert, strukturiert und dokumentiert abgelegt, damit sie vor der Übernahme in den Hauptcode von MemFuse eingehend überprüft, gebenchmarkt und getestet werden können.

---

## Detaillierter Master-Integrationsplan
👉 **[MASTER_INTEGRATION_PLAN.md](./MASTER_INTEGRATION_PLAN.md)** enthält den vollständigen Architektur-Blueprint, die technische Begründung jedes Moduls und den Implementierungsfahrplan.

---

## Verzeichnisübersicht nach strategischen Hebeln

### 🚀 [Hebel 1: Chimera Storage Turbo](./hebel_1_chimera_storage_turbo/README.md)
Technologische Beschleunigung des MemFuse Storage- und Index-Layers:
- **[Zero-Copy Serialization (rkyv)](./hebel_1_chimera_storage_turbo/zero_copy_rkyv/README.md):** `aliased_bytes.rs`, `rkyv_types.rs`, `rkyv_tx_buffer.rs`, `rkyv_hnsw_persist.rs`, `rkyv_metadata_index.rs`, `rkyv_lsm_storage.rs`.
- **[SIMD-Beschleunigung (AVX-512 / AVX2 / portable-simd)](./hebel_1_chimera_storage_turbo/simd_vector_dispatch/README.md):** `distance.rs`, `distance_bench.rs`, `SPEC-001`, `ADR-011`.
- **[Memory Pressure & Budgeting (SPEC-025 / SPEC-032)](./hebel_1_chimera_storage_turbo/memory_pressure_budgeting/README.md):** Lock-free `budget.rs`, `adaptive_allocator.rs`, `SPEC-025`, `SPEC-032`, `SPEC-048`.
- **[Tri-Hybrid Retrieval (RRF-Fusion k=60)](./hebel_1_chimera_storage_turbo/tri_hybrid_rrf_query/README.md):** `fusion.rs`, `planner.rs`, `hybrid.rs`, RRF-Konzept & Query-Engine Spezifikation.

---

### 🛡️ [Hebel 2: Orchestration Governance (SPO)](./hebel_2_orchestration_governance/README.md)
Prozessuale Absicherung für 60–100 Commits/Tag ohne Qualitätsverlust:
- **[CoVe-Gates (T-05)](./hebel_2_orchestration_governance/cove_gates_t05/README.md):** 4-Phasen Verifikationsvertrag, `cove_pr_gate_workflow.yml` (CI-Prüfung gegen Context Rot & Namenskollisionen wie `CompactionStrategy`).
- **[Minimal Necessary Context (MNC)](./hebel_2_orchestration_governance/minimal_necessary_context_mnc/README.md):** CE-01 Injektionsschema, Worker- und Verifier-Agenten-Manifeste (T-02).
- **[Deterministische Fehler-Matrix](./hebel_2_orchestration_governance/deterministic_error_matrix/README.md):** `error_matrix.yaml`, `error_matrix_mapper.rs` für standardisierte Aktionen in Tauri, Python und Rust.
- **[Metakognitiver Checkpoint (T-07)](./hebel_2_orchestration_governance/metacognitive_checkpoint_t07/README.md):** Per-Step PDCA Loop & Mutation-Enforced Retries gegen Silent Error Propagation.
- **[Reference Specs](./hebel_2_orchestration_governance/reference_specs/):** Originale SPO Master Framework Dokumente.

---

### 🧠 [Hebel 3: Vertikale Integration mit Atlas OS](./hebel_3_atlas_os_integration/README.md)
MemFuse als universelle L4-Memory-Engine und Realwelt-Härtetest:
- **[DaaB L4 Memory Layer](./hebel_3_atlas_os_integration/daab_l4_memory_layer/README.md):** Originaler Atlas Speicher-Stack (SQLite + LanceDB), DDL-Schema und Dokumentation.
- **[Atlas MemFuse Adapter](./hebel_3_atlas_os_integration/atlas_memfuse_adapter/README.md):** `memfuse_daab_provider.py` (Drop-in Ersatz für LanceDB/SQLite), `atlas_rag_memfuse.py` (FastMCP Gateway).
- **[Real-World Agent Testbed](./hebel_3_atlas_os_integration/realworld_agent_testbed/README.md):** `specialized_agents_memfuse.py`, `stress_test_atlas_memfuse.py` (paralleler 10-Agenten Benchmark für Durchsatz und P95-Latenz).

---

### 📦 [Hebel 4: In-Process GGUF Tensor Engine](./hebel_4_gguf_tensor_engine/README.md)
Autarkes lokales Embedding via Candle + GGUF Quantisierung (aus `chimeraDB` SPEC-041):
- **`model.rs`**: GgufEmbeddingModel für direkte CPU-Inferenz ohne ONNX / Ollama.
- **`embedder.rs`**: Batch-Embedding Pipeline mit asynchronem Mutex.
- **`autolinker.rs`**: Automatische semantische Kantenbildung für den CSR-Graph.

---

### 💥 [Hebel 5: Chaos Engineering & Resilienz](./hebel_5_chaos_resilience/README.md)
Extreme Belastungstests für MemFuse WAL V3 und MVCC Store (aus `chimeraDB` SPEC-035):
- **`chaos_engine.rs`**: 10 Chaos-Szenarien (Torn Writes, Bit-Flips, OOM, Disk Full).
- **`memfuse_chaos_test.rs`**: Spezifische Test-Suite für MemFuse Crash-Recovery.

---

### 📋 [Hebel 6: Desktop Context Capture & Sandbox](./hebel_6_desktop_context_capture_sandbox/README.md)
Automatisierte Kontext-Erfassung & sichere Skriptausführung (aus `textforge` & `template-tauri`):
- **`source_app.rs`**: Quell-App-Erkennung via KDE DBus, Wayland und X11.
- **`clipboard_monitor.rs`**: Event-basierter Clipboard-Monitor.
- **`quickjs_sandbox.rs`**: Isolierte QuickJS Sandbox mit 3s Timeout-Guard.
- **`memfuse_clipboard_ingestion.rs`**: Hintergrund-Ingestion-Service für MemFuse Collections.

---

### 🎨 [Hebel 7: Live Agent-to-UI (A2UI) Streaming](./hebel_7_a2ui_streaming_protocol/README.md)
Progressives UI-Rendering für Agenten und RAG-Treffer (aus `atlas`):
- **`builder.py` & `models.py`**: Type-safe Builder für Karten, Badges, Tabellen und Zeilen.
- **`a2ui_memfuse_card.py`**: Wandelt MemFuse 4-Signal Suchergebnisse in interaktive A2UI-Karten um.
