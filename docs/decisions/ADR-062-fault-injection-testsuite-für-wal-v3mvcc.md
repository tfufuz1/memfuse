# ADR-062: Fault-Injection-Testsuite für WAL V3/MVCC (adaptiert aus chimeraDB SPEC-035)

*   **Datum**: 2026-09-05
*   **Status**: ✅ Final
*   **Entscheidung**:
    - Die Fault-Injection-Testsuite wird ausschließlich als Test-only Integrationstests (`tests/`) sowie ein Hilfsbinary (`examples/chaos_writer.rs`) in `crates/memfuse-store` umgesetzt.
    - Es wird KEIN neues Workspace-Crate angelegt und KEINE Änderung an Quellcode unter `crates/memfuse-store/src/**` vorgenommen.
- **Alternativen**:
    - *Eigenes `chimera-chaos`-artiges Crate mit Produktions-Hooks (`FaultInjector::inject_sync`)*: Verworfen, da dies ASK-pflichtige API- und Hot-Path-Änderungen erfordert hätte, ohne dass dafür ein belegter Bedarf existierte.
- **Explizit verworfene Szenarien**:
    - `IOLatency` und `NetworkDegradation`: Verworfen, da MemFuse keine Netzwerkschicht besitzt (ADR-010: stdio-only JSON-RPC) und kein belegter Slow-Disk-Use-Case vorliegt, der Hooks im Hot-Path rechtfertigen würde.
- **CI-Kadenz**:
    - Einzelne Fault-Injection-Tests laufen als reguläre Integrationstests in `cargo test --workspace`.
    - Die kombinierte Fault-Matrix (`chaos_matrix.rs`) läuft ausschließlich nightly, ist `#[ignore]`-gated und blockiert keine Pull Requests.
*   **Begründung**: Bietet gezielte Abdeckung verbleibender Crash-Resilienz-Lücken (SSTable Bit-Flips, echte Process-Kills, Task-Abbrüche, Memory-Pressure) ohne Beeinträchtigung der Produktionscode-Topologie oder der PR-Laufzeiten.
