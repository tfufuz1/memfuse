# SPEC-20260607-CryptoWipeSecurity

## 🎯 1. Das Ziel (Context & "Why")
Sicherstellung, dass kryptografische Schlüssel im RAM physisch gelöscht werden (`emergency_wipe`), um Cold-Boot-Angriffe zu verhindern. Erweiterung der `Zeroize`-Abdeckung auf den `KeyManager`.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-CRY-WIPE-01]**: Nach Aufruf von `emergency_wipe()` oder beim Drop des Objekts MUSS der Speicherbereich des Schlüssels mit Nullen (0x00) überschrieben sein.
- **[INV-CRY-TEST-01]**: Tests dürfen den Speicherzustand nur über eine explizite `#[cfg(test)]`-Methode prüfen, um UB und Compiler-Optimierungen in der Produktions-Logik zu vermeiden.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-crypto`
- **Files**: `src/crypto.rs`, `src/anti_tamper.rs`

```rust
// In crates/memfuse-crypto/src/crypto.rs
impl KeyManager {
    pub fn emergency_wipe(&mut self);
    
    #[cfg(test)]
    pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32];
}

// In crates/memfuse-crypto/src/anti_tamper.rs
impl VolatileEncryptionKey {
    // key_bytes wird private
    #[cfg(test)]
    pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32];
}
```

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- N/A (Operation ist unfehlbar/void).

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Modifiziere `test_emergency_wipe_zeros_key_bytes` in `anti_tamper_integration.rs` so, dass `inspect_key_bytes_for_test()` genutzt wird.
- Erstelle einen neuen Test in `crypto.rs`, der `emergency_wipe()` auf einem `KeyManager` aufruft und via `inspect_key_bytes_for_test()` prüft, ob die Bytes 0 sind.
- Der Test MUSS fehlschlagen, da `KeyManager` aktuell kein `emergency_wipe` besitzt.
