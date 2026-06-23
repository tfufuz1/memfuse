# SDD Specification: `memfuse-crypto`

**Status:** DRAFT  
**Crate-Layer:** 1 (Security)  
**Souveränität:** Pure Rust, `#![forbid(unsafe_code)]`, No external C-deps.

---

## 1. Systemgrenzen & Verantwortlichkeit (MECE)

`memfuse-crypto` ist das einzige Modul, in dem Klartext-Daten die Persistenzgrenze in Richtung einer Engine (WAL/Storage) überschreiten dürfen.

### Verantwortlichkeiten:
- **Schlüsselmanagement:** Ableitung von hoch-entropischen PRKs aus Passwörtern via HKDF-SHA256 (`KeyManager`).
- **Domain-Separation:** Ableitung von Sub-Keys pro Datei/Stream zur Vermeidung von Nonce-Reuse.
- **Verschlüsselung:** Authentifizierte Verschlüsselung (AEAD) via AES-256-GCM-SIV.
- **Integrität:** HMAC-SHA256 für Anti-Tamper Schutz (WAL-Chaining).
- **Security-Lifetime:** RAII-basiertes Löschen von Schlüsseln (`Emergency Wipe`).

### Nicht-Verantwortlichkeiten:
- **Passwort-Hashing (Argon2):** Erfolgt auf Datenbank-Ebene (Layer 2), `crypto` erwartet bereits einen entropischen String oder PRK.
- **Key-Persistence:** Speichert keine Schlüssel auf Disk; erwartet Schlüssel-Injektion zur Laufzeit.

---

## 2. Kritische Invarianten & SDD-Garantien

| ID | Invariante | Beschreibung |
|---|---|---|
| **CRYPTO-INV-001** | **Nonce-Isolation** | Jedes `KeyManager` Objekt nutzt ein zufälliges 4-Byte Präfix + 8-Byte monotonen Counter für den GCM-Nonce. |
| **CRYPTO-INV-002** | **Key-Volatility** | `VolatileEncryptionKey` nutzt `Zeroize` (oder manuelle Wipes), um Schlüssel nach Gebrauch aus dem RAM zu tilgen. |
| **CRYPTO-INV-003** | **Pure-Rust-Policy** | Verbot jeglicher C-Bindings (`openssl`, `ring`-C). Nutzt `RustCrypto` Ökosystem. |

---

## 3. Schnittstellen-Spezifikation (High-Precision)

### 3.1 KeyManager (`crypto.rs`)
- **`try_new(passphrase, salt)`**: Initialisiert Master-PRK.
- **`derive_file_key(file_id)`**: Erzeugt `KeyManager` für spezifische Datei via HKDF-Expand.
- **`encrypt_auto_nonce(data)`**: Rückgabe von `(Ciphertext, [u8; 12])`.

### 3.2 Anti-Tamper (`anti_tamper.rs`)
- Stellt `VolatileEncryptionKey` bereit, der den tatsächlichen Key-Buffer kapselt.

### 3.3 WAL-Crypto-Adapter (`wal_crypto.rs`)
- Spezifische Implementierung für HMAC-Chained WAL Logs.

---

## 4. Codebase-Checklist (src/)

| Modul | Status | Bezug auf Spec |
|---|---|---|
| `lib.rs` | ✅ | Architektur-Dokumentation & Public API. |
| `crypto.rs` | ✅ | Kern-Implementierung von AEAD & HKDF. |
| `anti_tamper.rs` | ✅ | Memory-Protection (Wipe-on-Drop). |
| `wal_crypto.rs` | ✅ | Integration für `memfuse-store`. |

---

## 5. Verifikation (Triple-Gate)

- **I - Kompilierbarkeit:** `cargo check -p memfuse-crypto`
- **II - Stil:** `cargo clippy -p memfuse-crypto`
- **III - Verhalten:** 
  - `test_sub_key_derivation_prevents_nonce_reuse`: Verifiziert HKDF-Separation.
  - `test_key_manager_emergency_wipe`: Nachweis der Schlüssel-Tabula-Rasa.
