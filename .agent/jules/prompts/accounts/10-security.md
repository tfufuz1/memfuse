# Account 10 — Security

## Rolle
Encryption at Rest. Kryptografie-Implementierung mit auditierten Crates.

## Fokus
`crates/memfuse-store/src/crypto.rs`, alle WAL/SSTable Paths

## Zuständigkeiten
- AES-256-GCM Block-Level Encryption
- HKDF-SHA256 Key-Derivation
- Nonce-Management (monotoner Counter)
- Opt-in via `LsmConfig { encryption_passphrase: Option<String> }`

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-3.2 | 🟡 MITTEL | WP-1.1 DONE (SSTable stabil) | Primary |

## Crypto-Regeln
- **KEIN self-made Crypto** — nur auditierte Crates
- **Erlaubte Dependencies:**
  - `aes-gcm = "0.10"` (AEAD, pure safe Rust)
  - `hkdf = "0.12"` (KDF)
  - `sha2 = "0.10"` (Hash für HKDF)
- **Keine neuen unsafe-Blöcke** — `aes-gcm` ist safe Rust

## NIEMALS
- Eigene Crypto-Algorithmen implementieren
- unsafe für Crypto verwenden
- Passphrase im Klartext loggen
- Nonces wiederverwenden

## Scheduled Task Slots (15/Tag) — Phase: WP-3.2

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Security-Audit: `cargo audit` auf Crypto-Dependencies |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-3.2-Encryption.md` |
| 4 | RED: `test_encrypt_decrypt_roundtrip` |
| 5 | RED: `test_wrong_key_fails` |
| 6 | RED: `test_encrypted_db_unreadable_without_key` |
| 7 | RED: `test_encrypted_db_survives_restart` |
| 8 | GREEN: `crypto.rs` — `encrypt_block()` + `decrypt_block()` |
| 9 | GREEN: HKDF Key-Derivation von Passphrase |
| 10 | GREEN: Nonce-Counter (monoton, per Block) |
| 11 | GREEN: SSTable-Integration (encrypt on write, decrypt on read) |
| 12 | GREEN: WAL-Integration (encrypt on append, decrypt on replay) |
| 13 | Triple-Test: `nix develop -c cargo test -p memfuse-store` × 3 |
| 14 | Clippy+Fmt + Workspace-Test |
| 15 | PR: `feat(store): WP-3.2 Encryption at Rest AES-256-GCM` |

## Validation
```bash
nix develop -c cargo test -p memfuse-store   # 3× — alle alten + neue Crypto-Tests
nix develop -c cargo test --workspace        # Keine Regressionen
cargo audit                   # Keine CVEs in Crypto-Deps
```
