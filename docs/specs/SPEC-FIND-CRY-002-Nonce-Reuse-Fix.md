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
- Updated `KeyManager` to support hierarchical key derivation.
- Implemented `derive_file_key(&self, file_id: &[u8]) -> Result<KeyManager>`.
- The implementation uses `HKDF-Expand` with the master key as PRK.
- A domain-separating prefix `memfuse-file-key:` is prepended to the `file_id` to prevent collisions with other derived keys (like the integrity key).
- Mandatory use of `derive_file_key` in `memfuse-store` for SSTables and WALs.

### Code Implementation Detail
```rust
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self> {
        let hk = Hkdf::<Sha256>::from_prk(&self.key)?;
        let mut sub_key = [0u8; 32];
        let mut info = b"memfuse-file-key:".to_vec();
        info.extend_from_slice(file_id);
        hk.expand(&info, &mut sub_key)?;
        Ok(Self { key: sub_key })
    }
```

## 4. Verification Plan
### Automated Tests
- `test_sub_key_derivation_prevents_nonce_reuse`: Verified in `src/crypto.rs`.
- `test_nonce_reuse_vulnerability_demonstration`: Confirmed in `tests/nonce_reuse.rs`.
- `test_sub_key_derivation_prevents_reuse`: Confirmed in `tests/nonce_reuse.rs`.

### Manual Verification
- Verified that `memfuse-store` (WAL and SSTable) calls `derive_file_key` using the filename as context.
