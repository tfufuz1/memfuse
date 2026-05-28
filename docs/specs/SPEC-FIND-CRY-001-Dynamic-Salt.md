# Atomic Spec: FIND-CRY-001 Dynamic HKDF Salt

## 1. Problem Statement
The current `KeyManager` uses a hardcoded salt `b"memfuse-encryption-salt-v1"` for HKDF key derivation. This makes the system vulnerable to rainbow table attacks if multiple users use the same passphrase.

## 2. Proposed Solution
1. Modify `KeyManager::try_new` to accept an `Option<&[u8]>` as salt.
2. If `Some(salt)` is provided, use it.
3. If `None` is provided, use the legacy hardcoded salt (for backward compatibility) but log a warning (or handle it gracefully).
4. In higher layers (`memfuse-store`), generate a random salt upon database creation and persist it.

## 3. Technical Changes
- `KeyManager::try_new(passphrase: &str, salt: Option<&[u8]>) -> Result<Self>`
- Update `integrity_key` to also use the salt if we want consistency, OR just ensure `try_new` uses it for the master key derivation. (Wait, `integrity_key` derives from `self.key`, so it doesn't strictly need the salt again if `self.key` is already salted).

## 4. Verification Plan
- `test_different_salts_different_keys`: Verify that the same passphrase with different salts results in different `KeyManager` keys.
- `test_legacy_salt_compatibility`: Verify that providing `None` as salt results in the same key as the old hardcoded version.
