# Atomic Spec: SEC-001 AES-256-GCM-SIV Migration

## 1. Problem Statement
The current implementation uses `AES-256-GCM`. While it has some nonce-reuse mitigation via random prefixes and sub-key derivation, `AES-GCM` is fundamentally fragile. If a nonce is ever reused with the same key, the security is catastrophically broken. 
`AES-256-GCM-SIV` (Synthetic Initialization Vector) is nonce-misuse resistant. If a nonce is reused, it only reveals if the same plaintext was encrypted, but does not compromise the key or other ciphertexts.

## 2. Proposed Solution
Migrate `memfuse-crypto` from `aes-gcm` to `aes-gcm-siv`.
Keep the sub-key derivation (`derive_file_key`) as an additional layer of defense-in-depth.

## 3. Technical Changes
- Update `crates/memfuse-crypto/Cargo.toml` to use `aes-gcm-siv`.
- Update `crates/memfuse-crypto/src/crypto.rs` to use `Aes256GcmSiv`.
- Maintain the current API where possible to avoid breaking `memfuse-store`.

## 4. Verification Plan
### Automated Tests
- Run existing tests in `memfuse-crypto`.
- Add a specific test for nonce-misuse resistance: `test_gcm_siv_nonce_misuse_safety`.
- Verify that encryption/decryption still works after the switch.
