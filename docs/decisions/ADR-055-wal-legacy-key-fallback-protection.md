# ADR-055: WAL Legacy Key Fallback Protection

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Der Fallback auf den statischen `LEGACY_INTEGRITY_KEY` beim Replay alter Write-Ahead-Logs erfordert das explizite Flag `allow_legacy_integrity_key_fallback: bool` (Default: `false`).
*   **Begründung**: Schützt vor unbefugten Downgrade-Angriffen auf den WAL-Integritätsmechanismus.

---
