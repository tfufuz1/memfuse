# MemFuse Adapter für Atlas OS

## 1. Architektur-Übersicht
Dieser Adapter schließt die Brücke zwischen der High-Performance Rust-Speicherengine **MemFuse** und dem AI-Desktop-OS **Atlas**.

```
┌────────────────────────────────────────────────────────┐
│             Atlas AI Desktop OS (Frontend & UI)        │
│          Tauri Desktop · A2UI Streaming · Chat         │
└───────────────────────────┬────────────────────────────┘
                            │ gRPC / IPC
┌───────────────────────────▼────────────────────────────┐
│            Atlas Python Kernel / MCP Gateway           │
│         LangGraph Workflows · Specialized Agents       │
└───────────────────────────┬────────────────────────────┘
                            │ PyO3 Bindings
┌───────────────────────────▼────────────────────────────┐
│              MemFuse DaaB Provider (Python)            │
│            memfuse_daab_provider.py                    │
└───────────────────────────┬────────────────────────────┘
                            │ FFI (Rust Core)
┌───────────────────────────▼────────────────────────────┐
│                   MemFuse Engine                       │
│    HNSW (SIMD) + BM25 + Graph CSR + MVCC WAL V3        │
│           Lock-Free Resource Budgeting (SPEC-032)      │
└────────────────────────────────────────────────────────┘
```

## 2. Enthaltene Adapter-Komponenten
1. [`memfuse_daab_provider.py`](./memfuse_daab_provider.py): Vollwertiger Python-Provider, der die abstrakte Speicher-Schnittstelle von Atlas implementiert und direkt mit `memfuse-py` kommuniziert.
2. [`atlas_rag_memfuse.py`](./atlas_rag_memfuse.py): MCP-Gateway RAG Server, der Atlas-Agenten Tool-Calls (`remember_context`, `retrieve_context`) bereitstellt.
3. [`atlas_rag_original.py`](./atlas_rag_original.py): Originaler Atlas RAG-Server als Referenz.
