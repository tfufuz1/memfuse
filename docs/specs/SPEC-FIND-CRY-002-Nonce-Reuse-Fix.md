# Atomic Spec: FIND-CRY-002 AES-GCM Nonce-Reuse Fix

## 1. Problem Statement
The current `KeyManager` implementation in `memfuse-crypto` uses a static key derived from a passphrase for all encryption operations. 
When multiple files (SSTables, WALs) use the same `KeyManager`, and they use their internal offsets as nonces, it leads to Nonce-Reuse if they share the same offsets (e.g., both start at offset 0).

## 2. Proposed Solution
Implement hierarchical key derivation. 
1. The master `KeyManager` (initialized with the passphrase) will serve as the root.
2. For each specific file or logical data stream, a sub-key will be derived using HKDF-SHA256, taking a `file_id` (or similar unique identifier) as info.
3. This ensures that even if the same offset is used as a nonce, the underlying key is different for each file, preventing Nonce-Reuse.

## 3. Technical Changes
- Add `derive_file_key(&self, file_id: &[u8]) -> Result<KeyManager>` to `KeyManager`.
- The new `KeyManager` will have a key derived via:
  `HKDF-Expand(master_key, info=file_id)`
- Update existing documentation to mandate the use of `derive_file_key` for any file-backed storage.

## 4. Verification Plan
### Automated Tests
- `test_nonce_reuse_prevention`: Verify that the same data encrypted with the same `nonce_val` but different `file_id`s results in different ciphertexts.
- `test_sub_key_integrity`: Verify that a sub-key derived from a sub-key is consistent and correct.

### Manual Verification
- Ensure `memfuse-store` (where files are handled) actually calls `derive_file_key`. (This might be a follow-up task FIND-CRY-002-PART2).
