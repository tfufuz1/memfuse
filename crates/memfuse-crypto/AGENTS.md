# AGENTS.md — memfuse-crypto
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- AES-256-GCM & HMAC-Chaining für Rest-Verschlüsselung und Integrität.
- KeyManager verwendet HKDF für Subkey-Derivierung basierend auf persistenten Salts/UUIDs.

## Bekannte Fallstricke
- Hardcodierte Keys verboten; Fallbacks dürfen nur für Replay historischer WAL-Logs genutzt werden.

## Relevante rules/*.md
- `rules/wal_crypto.md` — HMAC Chaining & Derivation Regeln

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:CRY-001] STATUS:OPEN — Nonce-Uniqueness verification bei paralleler Verschlüsselung
