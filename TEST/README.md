# MemFuse Testing & Fault-Injection Integration

Dieses Verzeichnis enthält die Dokumentation und Ressourcen für die Fault-Injection- und Chaos-Testsuite von MemFuse.

## Aktiver Hebel

- **Hebel 5 — Chaos Engineering & Crash-Resilienz**
  - Adaptiert aus `chimeraDB` SPEC-035.
  - Dokumentiert im [`MASTER_INTEGRATION_PLAN.md`](MASTER_INTEGRATION_PLAN.md) und in [`rules/chaos_testing.md`](../rules/chaos_testing.md).
  - Umgesetzt in `crates/memfuse-store` (Layer 1) als Test-only Integrationstests und `examples/chaos_writer.rs`.

*Hinweis:* Die ursprünglichen Hebel 1–4, 6 und 7 wurden im Rahmen der Architektur-Review vom 2026-09-05 verworfen und aus dem Testplan entfernt.
