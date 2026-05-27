# REFACTOR-PLAN: memfuse-crypto
**Datei:** `docs/specs/REFACTOR-memfuse-crypto.md`
**Erstellt:** 2026-05-27
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 1 Stelle      | 0             |
| Test-Coverage      | ~85%          | >95%          |
| API-Vollständigkeit| 80%           | 100%          |
| Algo-Korrektheit   | RISKANT       | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-CRYPTO-001: Kritische Nonce-Reuse Gefahr in AES-GCM
**Typ:** Sicherheit / Kryptografischer Fehler
**Datei:** `crates/memfuse-crypto/src/crypto.rs`
**Zeile(n):** 46-51, 59-64
**Code (Kontext):**
```rust
let mut nonce_bytes = [0u8; 12];
nonce_bytes[4..12].copy_from_slice(&nonce_val.to_le_bytes());
let nonce = Nonce::from_slice(&nonce_bytes);
```
**Problem:** Der `KeyManager` wird in `memfuse-store` für mehrere Dateien (SSTables, WALs) verwendet. Dabei wird oft der Datei-Offset als `nonce_val` übergeben. Da mehrere Dateien denselben Offset 0 haben, wird dieselbe Nonce mit demselben Schlüssel für unterschiedliche Daten verwendet. Dies bricht die Sicherheit von AES-GCM vollständig (Nonce-Reuse-Angriff).
**Auswirkung:** Angreifer können den Schlüssel oder den XOR der Plaintexts rekonstruieren. Dies ist ein katastrophales Sicherheitsrisiko für ein verschlüsseltes System.

**Refaktorisierungsanweisung:**
```
1. Erweitere `KeyManager` um eine Methode `derive_file_key(&self, file_id: &[u8]) -> KeyManager`.
2. Nutze HKDF, um aus dem Master-Key und einer eindeutigen Datei-ID (z.B. Dateiname oder UUID) einen sub-key zu generieren.
3. Jedes `SSTable` und jedes `Wal` muss seinen eigenen `KeyManager` (mit sub-key) besitzen.
4. Alternativ: Erhöhe die Nonce auf 12 Byte und nutze die ersten 4 Byte für eine zufällige Datei-Salt/ID, die im Header der Datei gespeichert wird.
```

**Akzeptanzkriterien:**
- [ ] Dokumentation beweist, dass Nonces workspace-weit über alle Dateien hinweg eindeutig sind.
- [ ] Test `test_different_files_different_nonces` provoziert den Fall.

---

#### FIND-CRYPTO-002: Statisches Salt für Key Derivation (HKDF)
**Typ:** Sicherheit
**Datei:** `crates/memfuse-crypto/src/crypto.rs`
**Zeile(n):** 27, 39
**Code (Kontext):**
```rust
let salt = b"memfuse-encryption-salt-v1";
let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
```
**Problem:** Die Verwendung eines statischen Salts für alle Benutzer führt dazu, dass identische Passwörter zu identischen Schlüsseln führen. Dies erleichtert Rainbow-Table-Angriffe auf DB-Backups.
**Auswirkung:** Schwächere Passwort-Sicherheit.

**Refaktorisierungsanweisung:**
```
1. Generiere beim Erstellen einer neuen Datenbank ein zufälliges 32-Byte Salt.
2. Speichere dieses Salt in einer `MANIFEST`-Datei oder im Header des ersten SSTables.
3. Übergib das Salt beim Initialisieren des `KeyManager`.
```

**Akzeptanzkriterien:**
- [ ] `KeyManager::try_new` akzeptiert ein optionales Salt.
- [ ] Bei fehlendem Salt wird ein Default-Salt genutzt (Abwärtskompatibilität), aber eine Warnung geloggt.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-CRYPTO-003: Skelett-Fehlerkontext in IntegrityVerifier
**Typ:** Skeleton
**Datei:** `crates/memfuse-crypto/src/wal_crypto.rs`
**Zeile(n):** 113
**Code (Kontext):**
```rust
return Err(memfuse_core::MemFuseError::WalCorruption {
    offset: 0, // SKELETON: Immer 0
    reason: format!("HMAC mismatch for seq {}", entry.seq_no),
});
```
**Problem:** Der Offset bei WAL-Korruption wird hart auf 0 gesetzt, was die Lokalisierung des Fehlers in der Praxis unmöglich macht.
**Auswirkung:** Erschwerte Fehlerdiagnose bei korrupten Datenbanken.

**Refaktorisierungsanweisung:**
```
1. Füge dem `WalEntrySnapshot` ein Feld `offset: u64` hinzu.
2. Propagiere den tatsächlichen Datei-Offset bis zum `IntegrityVerifier`.
```

**Akzeptanzkriterien:**
- [ ] `WalCorruption` Error enthält den korrekten Offset.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-CRYPTO-001 (Kritisches Sicherheits-Fix)
Schritt 2: FIND-CRYPTO-002 (Salt-Management)
Schritt 3: FIND-CRYPTO-003 (Error context)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_sub_key_derivation
// Prüft ob km.derive_file_key("file1") != km.derive_file_key("file2")

// TEST-2: test_nonce_uniqueness
// Verifiziert dass bei gleichem Offset aber unterschiedlichen Sub-Keys 
// unterschiedliche Ciphertexts entstehen.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| `KeyManager::try_new` Param | Ja        | Passphrase + Salt übergeben.  |
| Sub-Key für SSTables        | Ja        | `SSTable::open` muss ID erhalten. |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Nonce-Reuse Risiko ist mathematisch/logisch ausgeschlossen.
- [ ] Salt ist nicht mehr statisch im Code verankert.
- [ ] Alle HMAC-Fehler liefern präzise Offsets.
- [ ] `just triple-test` 3× grün.
