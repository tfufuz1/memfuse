# REFACTOR-PLAN: memfuse-crypto
**Datei:** `docs/specs/REFACTOR-memfuse-crypto.md`
**Erstellt:** 2026-05-28
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 1 Tag
**Voraussetzung:** memfuse-core (für Error-Mapping)

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | ~70%          | >90%          |
| API-Vollständigkeit| 90%           | 100%          |
| Algo-Korrektheit   | ❌ GEFÄHRLICH | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFOT)

#### FIND-CRY-002: AES-GCM Nonce-Reuse
**Typ:** Sicherheit (Kritisch)
**Datei:** `crates/memfuse-crypto/src/crypto.rs`
**Zeilen:** 69–71
**Code (Kontext):**
```rust
let mut nonce_bytes = [0u8; 12];
nonce_bytes[4..12].copy_from_slice(&nonce_val.to_le_bytes());
let nonce = Nonce::from_slice(&nonce_bytes);
```
**Problem:** Die ersten 4 Bytes des Nonce sind statisch Null. Wenn derselbe `KeyManager` für verschiedene Dateien (SSTables/WALs) verwendet wird, kollidieren die Nonces bei gleichem `nonce_val` (z.B. Block-Offset 0). Nonce-Reuse in AES-GCM erlaubt das Brechen der Vertraulichkeit (XOR-Distance recovery).
**Auswirkung:** Vollständiger Verlust der Verschlüsselungs-Sicherheit bei mehreren Dateien.

**Refaktorisierungsanweisung:**
```
1. Erweitere KeyManager um ein zufälliges 4-Byte Prefix (context_id), das bei Instanziierung generiert wird.
2. Nutze context_id für nonce_bytes[0..4].
3. Alternativ: Erzwinge die Nutzung von derive_file_key() für jede Datei und stelle sicher, dass der file_id-Parameter eindeutig ist.
4. Empfehlung: Nutze eine Kombination aus zufälligem Prefix und derive_file_key.
```

**Akzeptanzkriterien:**
- [ ] `km.encrypt(data, 0)` erzeugt bei unterschiedlichen `KeyManager`-Instanzen (oder Sub-Keys) unterschiedliche Ciphertexte.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-CRY-001: Hardcoded / Static HKDF Salt
**Typ:** Sicherheit
**Datei:** `crates/memfuse-crypto/src/crypto.rs`
**Zeilen:** 31, 54
**Problem:** Der Salt `memfuse-encryption-salt-v1` ist hardcodiert.
**Auswirkung:** Anfälligkeit für Rainbow-Table-Angriffe gegen Passphrasen. Verringerte Entropie bei Key-Derivation.

**Refaktorisierungsanweisung:**
```
1. Entferne den statischen Default-Salt.
2. Fordere einen Salt in `try_new`.
3. Speichere den Salt im File-Header (SSTable/WAL) neben dem Ciphertext.
4. Nutze für die Integrity-Key-Expansion ebenfalls einen dynamischen Salt.
```

**Akzeptanzkriterien:**
- [ ] Tests beweisen, dass unterschiedliche Salts zu unterschiedlichen Keys führen (bereits vorhanden, muss aber Standard werden).

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-CRY-002 (Kritisches Sicherheitsleck schließen)
Schritt 2: FIND-CRY-001 (Salt-Handling modernisieren)
```

## NEUE TESTS

```rust
// TEST-1: test_nonce_collision_resistance
// Prüft: Zwei KeyManager mit gleicher Passphrase aber unterschiedlicher context_id erzeugen 
// bei gleichem Nonce-Value unterschiedliche Ciphertexte.

// TEST-2: test_encryption_fails_without_salt
// Prüft: Das System verweigert die Arbeit ohne expliziten Salt (Safety).
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] FIND-CRY-002 behoben (Eindeutige Nonces garantiert).
- [ ] Keine hardcodierten Salts mehr im Code.
- [ ] `just triple-test -p memfuse-crypto` grün.
- [ ] Dokumentation der Nonce-Struktur in `crypto.rs` aktualisiert.
