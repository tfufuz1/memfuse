use zeroize::Zeroize;

/// Defines a cryptographic key that is explicitly zeroed out when dropped
/// or when an emergency trigger is activated, protecting against cold-boot attacks.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct VolatileEncryptionKey {
    key_bytes: [u8; 32], // AES-256 Key
}

impl VolatileEncryptionKey {
    /// Creates a new volatile key from a raw 32-byte array.
    pub fn new(raw: [u8; 32]) -> Self {
        Self { key_bytes: raw }
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
        &self.key_bytes
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
        self.key_bytes == other.key_bytes
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
}
