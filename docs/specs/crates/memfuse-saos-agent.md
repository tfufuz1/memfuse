# CRATE-SPEC: memfuse-saos-agent
**Version:** CURRENT -> TARGET
**Status:** NEEDS_REDESIGN

---

## SINGLE RESPONSIBILITY
Diese Crate ist EXAKT zuständig für die Kernkomponente memfuse-saos-agent, isoliert vom Rest des Systems, garantiert zero-panic.

## VOLLSTÄNDIGE PUBLIC API (Soll-Zustand)
(Zu extrahieren aus der Codebase-Validierung, siehe FORENSIC_INVENTORY.md)

## KRITISCHE INVARIANTEN (NIEMALS VERLETZEN)
- INVARIANT-01: Zero-panic policy in all synchronous entry points.
- INVARIANT-02: Alle async tasks müssen ein cancellation handle haben.

## IDENTIFIZIERTE SCHWACHSTELLEN
Refer to FORENSIC_FINDINGS.md und BACKLOG.md für spezifische TIER 1 Blocker in dieser Crate.

## KONKRETE HANDLUNGSANWEISUNGEN FÜR IMPLEMENTIERER
### PRIORITÄT 1 — SOFORT (Release-Blocker)
1. Beseitigung von Nonce-Reuse und Rollback-Divergenzen (sofern zutreffend).
2. Tausche `unwrap`/`expect` durch formelle `MemFuseError` Transformationen.

### PRIORITÄT 2 — KURZFRISTIG (Pre-Launch)
1. Stabilisierung der Traits.

### PRIORITÄT 3 — MITTELFRISTIG (Post-Launch)
1. Feature Flags und Tracing ausbauen.

## TESTABDECKUNGS-ANFORDERUNGEN
- Unit-Tests: Vollständige Branch-Coverage für `Result` Outputs.
- Integration-Tests: Durch Triple-Test-Gate.

## SCHNITTSTELLEN ZU ANDEREN CRATES
(Definiere den DAG neu um zyklische Dependencies zu verhindern)
