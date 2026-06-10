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
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
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
