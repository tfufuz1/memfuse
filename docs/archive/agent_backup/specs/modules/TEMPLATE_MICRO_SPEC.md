# KI-MS: [BEREICH-ID]-[TITEL]

**Status:** DRAFT | APPROVED | IMPLEMENTED | DEPRECATED
**Erstellt:** [DATE]
**Abhängigkeiten:** [LIST]

## 🎯 1. Zielsetzung (Context)
*Was löst dieses Modul/Feature? Welchen technischen Wert hat es?*

---

## 🛡️ 2. Invarianten (The Law)
*Nicht verhandelbare architektonische Gesetze.*
- **[INV-001]**: Kein `.unwrap()` / `.expect()` (Strict `MemFuseError` propagation).
- **[INV-002]**: Einhaltung der DAG-Layer-Invariante.
- **[INV-003]**: [Feature-Spezifische Invariante]

---

## 📍 3. Schnittstellenvertrag (Contracts)
*API-Signaturen und Datenstrukturen.*

```rust
// Signatur / Typen:
```

---

## 🛑 4. Fehlerverhalten (Edge Cases)
- **Error E-001**: [Bedingung] -> `MemFuseError::...`
- **Edge-001**: [Randfall] -> [Handler]

---

## ✅ 5. Akzeptanzkriterien (Gherkin/BDD)
```gherkin
Feature: [Name]
  Szenario: [Happy Path]
    Gegeben [Vorbedingung]
    Wenn [Aktion]
    Dann [Erfolgreich]

  Szenario: [Fail Case]
    Gegeben [Fehlerbedingung]
    Wenn [Aktion]
    Dann [Error]
```

---

## 🛠️ 6. Implementierung (TDD-Plan)
1.  **Red-Phase**: Erstelle Test `tests/unit/test_...rs`.
2.  **Implementation**: Crate `memfuse-XYZ`.
3.  **Verification**: `just triple-test`.
