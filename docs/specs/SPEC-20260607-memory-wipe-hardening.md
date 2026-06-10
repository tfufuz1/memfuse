# SPEC-20260607-memory-wipe-hardening

## 🎯 1. Das Ziel (Context & "Why")
Sicherstellen, dass kryptografische Schlüssel im RAM bei einem `emergency_wipe()` oder beim Droppen der Struktur deterministisch und unwiderruflich mit Nullen überschrieben werden, um Cold-Boot-Angriffe zu verhindern.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-CRY-001]**: Der RAM-Bereich, der den Schlüssel enthält, muss nach `emergency_wipe()` ausschließlich den Wert `0x00` enthalten.
- **[INV-CRY-002]**: Die Löschung muss durch die Verwendung des `zeroize` Crates gegen Compiler-Optimierungen (Dead Store Elimination) abgesichert sein.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-crypto`
- **File**: `src/crypto.rs`, `src/anti_tamper.rs`

```rust
// In src/anti_tamper.rs:
impl VolatileEncryptionKey {
    #[cfg(test)]
    pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32] {
        &self.key_bytes
    }
}

// In src/crypto.rs:
pub struct KeyManager {
    key: VolatileEncryptionKey, // Migration von [u8; 32]
    // ...
}
```

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- Keine spezifischen neuen Fehler, da Zeroization in-place passiert.

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Ein Test erstellt einen `VolatileEncryptionKey` mit Daten ungleich Null, ruft `emergency_wipe()` auf und verifiziert mittels `inspect_key_bytes_for_test()`, dass alle 32 Bytes Null sind.
