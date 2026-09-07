# ADR-048: WAL Legacy-Key Feature-Gating & Downgrade Protection

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Die automatische Fallback-Entschlüsselung / Integritätsprüfung alter Write-Ahead-Logs mittels hartkodiertem `LEGACY_INTEGRITY_KEY` wird hinter das explizite Konfigurations-Flag `allow_legacy_integrity_key_fallback: bool` (Default: `false`) in `WalConfig` gestellt. Der Standardpfad in `Wal::open()` weist alte WAL-Dateien ohne explizites Opt-In als fehlerhaft zurück (`MemFuseError::wal_corruption`).
*   **Alternativen**:
    - Beibehaltung des automatischen Fallbacks: Verworfen, da ein Angreifer alte WAL-Dateien unterschieben und einen Silent Downgrade herbeiführen könnte.
    - Vollständiges Entfernen von `LEGACY_INTEGRITY_KEY`: Verworfen, um Migrationstools das Auslesen alter Logdateien weiterhin zu ermöglichen.
*   **Begründung**: Verhindert unbefugte Downgrade-Angriffe auf den WAL-Integritätsmechanismus, wahrt aber Abwärtskompatibilität bei expliziter Migration.

---
