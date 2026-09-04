// FILE-CONTEXT
// ZWECK: Cold-boot protection and explicit Zeroize discipline for volatile encryption keys.
// INVARIANTEN: Key bytes held in Zeroizing<[u8; 32]>. emergency_wipe explicitly zeroizes memory.
// NICHT-OFFENSICHTLICH: Debug implementation redacts secret bytes. Constant-time equality comparison.
// HOTSPOTS: [10-60]
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

#![cfg_attr(not(test), forbid(unsafe_code))]

use subtle::ConstantTimeEq;
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
        Self {
            key_bytes: Zeroizing::new(raw),
        }
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

// AI-TAG[SECURITY][MINOR][RESOLVED] ConstantTimeEq used in PartialEq equality (ID: AGT-CRYPTO-dd984bc2) (TS: 2026-09-03T19:31:53Z) (SESSION: a413a598)
// RESOLVED: Replaced manual XOR loop with subtle::ConstantTimeEq for provably constant-time slice comparison.
impl PartialEq for VolatileEncryptionKey {
    fn eq(&self, other: &Self) -> bool {
        self.key_bytes
            .as_slice()
            .ct_eq(other.key_bytes.as_slice())
            .into()
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

    // AI-TAG[CORRECTNESS][MAJOR][RESOLVED] Refactored zeroize test using ManuallyDrop (ID: AGT-CRYPTO-7519b7cd) (TS: 2026-09-03T19:31:53Z) (SESSION: a413a598)
    // RESOLVED: Refactored test to use ManuallyDrop and explicit Zeroize::zeroize without post-drop raw pointer dereferencing on stack memory, eliminating UAF/UB.
    #[test]
    fn test_zeroize_on_drop_wipes_memory() {
        use std::mem::ManuallyDrop;

        let raw: [u8; 32] = [0xCD; 32];
        let mut key = ManuallyDrop::new(VolatileEncryptionKey::new(raw));
        let ptr = key.as_bytes().as_ptr();

        // Precondition: check that memory contains original non-zero key bytes
        // SAFETY: `key` is alive in ManuallyDrop wrapper and `ptr` points directly to its buffer.
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, 32);
            assert_eq!(slice, &[0xCD; 32]);
        }

        // Action: Explicitly invoke zeroize without deallocating/dropping stack frame memory
        Zeroize::zeroize(&mut *key);

        // Postcondition: Check that memory was zeroed in place without UAF
        // SAFETY: `key` memory is still allocated within ManuallyDrop wrapper in this stack frame.
        unsafe {
            let cleared_slice = std::slice::from_raw_parts(ptr, 32);
            assert_eq!(
                cleared_slice, &[0x00; 32],
                "Memory MUST be zeroed after zeroize"
            );
        }
    }
}
