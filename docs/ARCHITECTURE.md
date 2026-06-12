# MemFuse Architektur — Kurzreferenz

## Kern-Philosophie
MemFuse ist eine **air-gapped, zero-panic, 100% Safe-Rust Embedded Vector Engine**.
Der "Souveräne Kern" garantiert den Betrieb ohne externe C-Laufzeitumgebungen oder Cloud-Dienste.

## Schichtmodell (DAG)

1.  **Foundation (memfuse-core)**: Globale Invarianten, Types und Fehler-Enums.
2.  **Engines (Layer 1)**:
    *   `memfuse-store`: LSM-Tree Persistenz (WAL + SSTables).
    *   `memfuse-index`: Vektor-Suche (HNSW + SIMD).
    *   `memfuse-text`: Volltext-Suche (BM25).
    *   `memfuse-crypto`: Verschlüsselung & Integrität (AES-GCM, HMAC).
3.  **Orchestration (memfuse-db)**: Collections, Snapshot-Isolation, 4-Signal Fusion.
4.  **Integration (Layer 3)**:
    *   `memfuse-py`: Python Bindings.
    *   `memfuse-embed` (Optional): ONNX Runtime Integration.
    *   `memfuse-cluster` (Optional): Raft-basierte Verteilung.

## Verifizierte Invarianten

| Invariante | Status | Beweismethode |
|---|---|---|
| **Souveränität** | ✅ Verifiziert | Cargo-Build ohne C-Crates im Default-Profil. |
| **Zero-Panic** | ✅ Verifiziert | 100% Logic-Path Coverage im Core, Linting-Audit. |
| **Determinismus** | ✅ Verifiziert | Cross-Check SIMD vs. Skalar (Epsilon 1e-4). |
| **Crash-Consistency**| ✅ Verifiziert | Fault-Injection im WAL (Partial Writes). |
| **Atomarität** | ✅ Verifiziert | Stress-Test mit 64+ parallelen Transaktionen. |

## Sicherheit
*   **HKDF Key Derivation**: Eigener kryptographischer Kontext pro Datei.
*   **HMAC Chaining**: WAL-Integrität gegen Manipulation geschützt.
*   **Namespace Isolation**: Vollständige Trennung von Collections auf Storage-Ebene.

---
*Status: 2026-06 — Sovereign-Core Audit abgeschlossen.*
