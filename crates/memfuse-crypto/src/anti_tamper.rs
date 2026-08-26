use zeroize::{Zeroize, Zeroizing};

/// Defines a cryptographic key that is explicitly zeroed out when dropped
/// or when an emergency trigger is activated, protecting against cold-boot attacks.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct VolatileEncryptionKey {
    key_bytes: Zeroizing<[u8; 32]>, // AES-256 Key
}

impl VolatileEncryptionKey {
    /// Creates a new volatile key from a raw 32-byte array.
    pub fn new(raw: [u8; 32]) -> Self {
        Self { key_bytes: Zeroizing::new(raw) }
    }

    /// Emergency Trigger: Explicitly wipes the key from memory.
    /// This is to be hooked into hardware sensors (e.g. crash/voltage).
    pub fn emergency_wipe(&mut self) {
        // Der Zeroize-Trait überschreibt den RAM-Bereich mit Nullen
        // und verhindert Compiler-Dead-Store-Elimination.
        self.key_bytes.zeroize();
    }

    /// Accessor for cryptographic operations.
    pub fn as_bytes(&self) -> &[u8] {
        self.key_bytes.as_slice()
    }

    /// Test-only method to inspect key bytes.
    /// Used to verify zeroing in integration tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32] {
        &self.key_bytes
    }
}

impl std::fmt::Debug for VolatileEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolatileEncryptionKey")
            .field("key_bytes", &"*** REDACTED ***")
            .finish()
    }
}

impl PartialEq for VolatileEncryptionKey {
    fn eq(&self, other: &Self) -> bool {
        let mut diff = 0u8;
        for (a, b) in self.key_bytes.iter().zip(other.key_bytes.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emergency_wipe_zeros_key_bytes() {
        let raw: [u8; 32] = [0xAA; 32];
        let mut key = VolatileEncryptionKey::new(raw);

        // Precondition: Key contains expected value
        assert_eq!(key.inspect_key_bytes_for_test(), &[0xAA; 32]);

        // Action: Trigger emergency wipe
        key.emergency_wipe();

        // Proof: Key is fully zeroed
        assert_eq!(
            key.inspect_key_bytes_for_test(),
            &[0x00; 32],
            "All key bytes MUST be zeroed after emergency_wipe()"
        );
    }

    #[test]
    fn test_emergency_wipe_is_idempotent() {
        let raw: [u8; 32] = [0xFF; 32];
        let mut key = VolatileEncryptionKey::new(raw);

        key.emergency_wipe();
        assert_eq!(key.inspect_key_bytes_for_test(), &[0x00; 32]);

        // Second wipe must be safe and idempotent
        key.emergency_wipe();
        assert_eq!(key.inspect_key_bytes_for_test(), &[0x00; 32]);
    }

    #[test]
    fn test_key_debug_redacts_key() {
        let raw: [u8; 32] = [0xAB; 32];
        let key = VolatileEncryptionKey::new(raw);
        let debug_str = format!("{key:?}");
        assert!(!debug_str.contains("171")); // 0xAB in decimal
        assert!(debug_str.contains("REDACTED"));
    }

    #[test]
    fn test_zeroize_on_drop_wipes_memory() {
        let raw: [u8; 32] = [0xCD; 32];
        let ptr: *const u8;
        {
            let key = VolatileEncryptionKey::new(raw);
            ptr = key.as_bytes().as_ptr();
            // Precondition: check that memory contains original non-zero key bytes
            // SAFETY: `key` is alive in this scope and `ptr` points directly to its heap/stack buffer.
            unsafe {
                let slice = std::slice::from_raw_parts(ptr, 32);
                assert_eq!(slice, &[0xCD; 32]);
            }
            // `key` goes out of scope here and its drop/zeroize handler is invoked.
        }

        // SAFETY: We dereference `ptr` immediately after `key` is dropped in this single-threaded, controlled unit test frame
        // to inspect that the drop handler ran and zeroed out the underlying memory array before stack reuse.
        unsafe {
            let cleared_slice = std::slice::from_raw_parts(ptr, 32);
            assert_eq!(cleared_slice, &[0x00; 32], "Memory MUST be zeroed after drop");
        }
    }
}
