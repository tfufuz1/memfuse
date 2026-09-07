# ADR-008: Embedding-Backend — ONNX (memfuse-embed) → Ollama HTTP (memfuse-ollama)

*   **Datum**: 2026-08-22
*   **Status**: ✅ Final (Ersetzt ADR-007 bzgl. lokaler ONNX-Inferenz)
*   **Entscheidung**: Ollama via `memfuse-ollama` als primäres Embedding-Backend. `memfuse-embed` wird vollständig aus Workspace-Dependencies und Features entfernt.
*   **Alternativen**: ONNX In-Process Embeddings (`memfuse-embed`).
*   **Begründung**:
    - Ollama dient im KMU-Desktop-Szenario bereits als LLM-Runtime.
    - Modell-Tausch ohne Code-Änderung (Ollama-Modell-Name konfigurierbar).
    - Apple-Silicon ARM-Optimierung durch Ollama nativ vorhanden.
    - Reduziert C++ Native Build-Komplexität (kein ONNX-Runtime-Vendoring).
*   **Kosten & Konsequenzen**:
    - Höhere Latenz pro Embedding vs. In-Process-ONNX (mitigiert durch parallele Embedding-Batch-Requests in `memfuse-ollama`).
    - Harte Laufzeit-Abhängigkeit von lokalem Ollama-Prozess.
    - `memfuse-ollama` als shared Crate bereitgestellt für `memfuse-tauri`, `memfuse-mcp` und `memfuse-py`.

---
