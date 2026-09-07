# ADR-036: unsafe-Scope-Erweiterung für test-only crypto anti_tamper

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: AGENTS.md §4 wird um den test-only unsafe-Ausnahmefall in `memfuse-crypto/src/anti_tamper.rs` ergänzt (Zeroize-Drop-Semantik-Verifikation). Im Produktionsbuild bleibt `memfuse-crypto` vollständig unsafe-frei (`#![cfg_attr(not(test), forbid(unsafe_code))]`).
*   **Begründung**: AUD-01 aus Audit 2026-08-28 dokumentierte Doku-Drift zwischen tatsächlichem Code und AGENTS.md. Governance-Dokumente müssen Realität abbilden, nicht verbergen.

---
