# Minimal Necessary Context (MNC) für MemFuse

## 1. Problem
Wenn Entwickler-Agenten bei 60–100 Commits/Tag das gesamte MemFuse-Repository (inklusive MVCC-Store, WAL-Logik, HNSW-Graph, ONNX-Reranker, Tauri-Frontend, PyO3-Bindings) im Prompt erhalten, passiert unweigerlich Folgendes:
- **Lost-in-the-Middle:** Wichtige Modul-Invarianten gehen in der Token-Menge unter.
- **Cross-Layer Halluzinationen:** Ein Agent, der nur an einer Tauri-UI-Komponente arbeitet, modifiziert plötzlich interne Transaktionslogik im Rust-Core.
- **Divergierende Typen:** Neue Duplikate bestehender Typen werden erzeugt.

## 2. Das MNC-Prinzip
1. **Chirurgische Präzision:** Ein Agent erhält nur den Scope des aktuellen Crates (z. B. `crates/memfuse-embed/`).
2. **Interface Boundaries statt Implementierungsdetails:** Andere Crates werden ausschließlich über Signaturen/Traits injiziert.
3. **MNC-Paketaufbau (CE-01):**
   - Tier 1: Anchor Context (System-Rolle & Constitution, max 500 Tokens)
   - Tier 2: Target Specification (Erlaubte Dateien & ACs, max 1500 Tokens)
   - Tier 3: Interface Signatures (Traits & Typdefinitionen)
   - Tier 4: JIT Workspace State (Letzter Test-Error / Diff)

## 3. Enthaltene Dateien
- [`mnc_injection_template.md`](./mnc_injection_template.md): Formales Schema zur Erzeugung von JIT-Kontext-Paketen für Agenten.
- [`memfuse_worker_manifest.md`](./memfuse_worker_manifest.md): Manifest für Worker-Agenten (Coding-Grenzen & Inhibit-Regeln).
- [`memfuse_verifier_manifest.md`](./memfuse_verifier_manifest.md): Manifest für Verifier-Agenten (strikte Trennung von Autor und Prüfer).
