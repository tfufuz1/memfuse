# MemFuse Architecture Context (The Sovereign Core)

## Das Ziel (The "Why")
MemFuse ist eine in-process, einbettbare und extrem performante Vektor/Hybrid-Suchdatenbank für lokale LLM-RAG-Systeme ("SQLite for AI Agents").

## The doctrine: Zero-Panic & Async Safety
1. **Kein `unwrap()`, kein `expect()`, kein `panic!()`** in Hot-Paths. Alles muss über das zentrale `memfuse_core::MemFuseError` via `?` propagiert werden.
2. **Keine blockierende I/O.** In `memfuse-store` ist ausschließlich `tokio::fs` erlaubt.
3. **Unsicheres Rust (`unsafe`) nur isoliert:** `unsafe` Blöcke sind nur für FFI oder SIMD in `memfuse-index` gestattet und MÜSSEN durch einen formalen Kommentar `// SAFETY: [Reasoning]` begründet werden.

## Crate-Hierarchie & Abhängigkeiten
- Die Architektur muss strickt hierarchisch bleiben (DAG):
  `memfuse-core` -> wird von allen genutzt.
  `memfuse-store` & `memfuse-index` greifen auf `memfuse-core` zu, aber niemals aufeinander.
  `memfuse-db` orchestriert beide Systeme (`store` und `index`) und reicht die API nach außen (`memfuse` root).
