# AGENTS.md — memfuse-crypto
> Layer 1 | Encryption-at-Rest, HMAC-Chaining, Zeroize | ~2300 LOC

## 1. Zweck & Architekturrolle

Verantwortlich für Encryption-at-Rest (AES-256-GCM-SIV) und Datenintegrität 
(HMAC-Chaining im WAL). Kapselt die Key-Derivation (HKDF), Zeroize-Speicherhygiene 
und den Anti-Tamper-Schutz der WAL-Einträge.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]` (mit Ausnahme im Test-Modul für Memory-Inspektion) |
| `crypto.rs` | `KeyManager` — HKDF Subkey Derivation, AES-256-GCM-SIV Ver-/Entschlüsselung |
| `wal_crypto.rs` | `WalHmac`, `IntegrityVerifier`, `EncryptedWal` — HMAC-Chaining Protokoll |
| `anti_tamper.rs` | `VolatileEncryptionKey`, Speicherschutz (Zeroize) |

## 3. Kritische Invarianten

### Key Derivation Kette
Schlüssel MÜSSEN zwingend durch die `KeyManager` HKDF-Pipeline abgeleitet werden.
**Pfad**: Passphrase + Salt → Master Key → `derive_file_key(file_id)` → Subkey.
Hardcodierte Schlüssel sind absolut **VERBOTEN** (SECURITY BLOCKER).

### HMAC Chaining (WAL Integrität)
Jeder WAL-Eintrag (`WalEntrySnapshot`) muss kryptographisch mit dem vorherigen Eintrag 
verkettet werden (HMAC-Chain). Ein Bruch in der Kette indiziert Manipulation oder 
Korruption und muss mit `MemFuseError::WalCorruption` hart abbrechen.

### Zeroize-Garantie
Kryptographisches Schlüsselmaterial (`VolatileEncryptionKey`) implementiert den 
`ZeroizeOnDrop` Trait. Schlüsselmaterial darf den Scope nicht als Klartext (`String` 
oder `Vec<u8>`) verlassen, sondern MUSS in `zeroize::Zeroizing` oder dem dedizierten
Volatile-Wrapper gekapselt sein.

### Nonce-Uniqueness (AES-256-GCM-SIV)
Verschlüsselungsoperationen (`encrypt_auto_nonce`) erzeugen zufällige Nonces.
Auch wenn AES-GCM-SIV resistent gegen Nonce-Reuse ist, MUSS für jede Verschlüsselung
eine neue, kryptographisch sichere Zufallszahl (`OsRng`) generiert werden.

## 4. Public API Quick-Reference

```rust
// === Key Management (crypto.rs) ===
pub struct KeyManager { ... }
impl KeyManager {
    pub fn try_new(passphrase: &str, salt: &[u8]) -> Result<Self>;
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self>;
    pub fn encrypt_auto_nonce(&self, data: &[u8]) -> Result<(Vec<u8>, [u8; 12])>;
    pub fn decrypt_auto_nonce(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>>;
}

// === WAL HMAC (wal_crypto.rs) ===
pub struct IntegrityVerifier { ... }
impl IntegrityVerifier {
    pub fn verify_and_update_v3(&mut self, entry: &WalEntrySnapshot, offset: u64) -> Result<()>;
}

// === Anti-Tamper (anti_tamper.rs) ===
pub struct VolatileEncryptionKey { ... }
impl VolatileEncryptionKey {
    pub fn emergency_wipe(&mut self);
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Plaintext-Vektor für Keys nutzen:
let key: Vec<u8> = vec![...];
// ✅ KORREKT — Zeroize Wrapper nutzen:
let key = zeroize::Zeroizing::new(vec![...]);

// ❌ FALSCH — Keys hart kodieren:
let key = b"hardcoded_key_32bytes___________"; 
// ✅ KORREKT:
let key = load_or_create_integrity_key(&path)?;

// ❌ FALSCH — HMAC-Fehler ignorieren:
let _ = verifier.verify_and_update(...);
// ✅ KORREKT:
verifier.verify_and_update(...)
    .map_err(|e| MemFuseError::WalCorruption { ... })?;
```

## 6. Concurrency & Lock-Hierarchie

Verschlüsselung und HMAC-Operationen sind rein synchron und Thread-Safe (CPU-bound).
Keys und Verifier halten keine Locks. Instanzen wie `IntegrityVerifier` müssen 
pro WAL-Writer mutabel gehalten werden (was durch den `write_lock` im `LsmStorage`
gewährleistet wird).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-store` (L1 Peer), `memfuse-db` (L2)
- **Genutzt von**: `memfuse-store` (für WAL & SSTable Encryption), `memfuse-mcp` (für Sandbox-Isolierung)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| `rules/wal_crypto.md` | HMAC Chaining & Derivation Regeln |
| `AGENTS.md §4` | `unsafe` Ausnahme für Memory-Wipe Verifikation |
