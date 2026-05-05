# SPEC-[DATUM]-[NAME]

## 🎯 1. Das Ziel (Context & "Why")
*Ein bis maximal zwei Sätze. Warum wird diese Funktion/Änderung gebraucht? (z.B. "Implementiert Namespaces in der Store-Crate für Multi-Tenancy.")*

---

## 🛡️ 2. Die Invariante(n) (The "Law")
*Was muss nach der Ausführung zwingend unumstößlich wahr sein? Formuliert als strenges Gesetz.*
- **[INV-NAME-1]**: *Das Gesetz hier...*

---

## 📍 3. Speicherort & API-Signatur
*Welche Datei / welches Modul wird modifiziert? Wenn neue Structs/Enums entstehen, zeige kurz die Signatur.*
- **Crate**: `memfuse-XYZ`
- **File**: `src/pfad/zur/datei.rs`

```rust
// Erwartete Signatur / Neues Struct:
```

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
*Unter welchen Umständen MUSS die Funktion fehlschlagen und welchen Error gibt sie zurück?*
- Wenn X passiert -> `Err(MemFuseError::...)`

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
*Kurze Beschreibung, wie der fehlschlagende Test konzipiert sein muss, der ZUERST geschrieben wird.*
- Der Test muss Methode X aufrufen und ein `Ok()` erwarten, während er Daten Y einspeist.
