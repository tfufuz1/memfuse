//! Integration-Test: Verifikation des Anti-Tamper Schlüsselauslöschens
//!
//! Testziel (aus dem Verifikationsplan):
//! Nachweis, dass `emergency_wipe()` den RAM-Bereich vollständig nullt
//! und der Compiler-Optimizer dies nicht wegoptimiert (dank `zeroize`).

use memfuse_crypto::anti_tamper::VolatileEncryptionKey;

/// Überprüft, dass `emergency_wipe()` alle Key-Bytes auf 0x00 setzt.
/// Dies ist der direkte In-Process-Nachweis des Cold-Boot-Schutzes.
#[test]
fn test_emergency_wipe_zeros_key_bytes() {
    let raw: [u8; 32] = [0xAA; 32];
    let mut key = VolatileEncryptionKey::new(raw);

    // Vorbedingung: Schlüssel enthält erwarteten Wert
    assert_eq!(
        key.inspect_key_bytes_for_test(),
        &[0xAA; 32],
        "Key must be initialized correctly"
    );

    // Aktion: Notfalllöschung auslösen
    key.emergency_wipe();

    // Nachweis: Schlüssel ist vollständig genullt
    assert_eq!(
        key.inspect_key_bytes_for_test(),
        &[0x00; 32],
        "All key bytes MUST be zeroed after emergency_wipe()"
    );
}

/// Smoke-Test: Mehrfache Lösch-Operationen sind idempotent und panizieren nicht.
#[test]
fn test_emergency_wipe_is_idempotent() {
    let raw: [u8; 32] = [0xFF; 32];
    let mut key = VolatileEncryptionKey::new(raw);

    // Erstes Wipe
    key.emergency_wipe();
    assert_eq!(key.inspect_key_bytes_for_test(), &[0x00; 32]);

    // Zweites Wipe auf bereits genulltem Schlüssel — darf nicht panizieren
    key.emergency_wipe();
    assert_eq!(key.inspect_key_bytes_for_test(), &[0x00; 32]);
}
