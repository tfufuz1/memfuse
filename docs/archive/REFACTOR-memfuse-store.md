# REFACTOR-PLAN: memfuse-store
**Datei:** `docs/specs/REFACTOR-memfuse-store.md`
**Erstellt:** 2026-05-27
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 3 Tage
**Voraussetzung:** memfuse-core, memfuse-crypto

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 1 Stelle      | 0             |
| Test-Coverage      | ~80%          | >90%          |
| API-Vollständigkeit| 85%           | 100%          |
| Algo-Korrektheit   | DUBIOS (Rollback) | VERIFIED  |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-STORE-001: Rollback-Inkonsistenz (SSTables werden ignoriert)
**Typ:** Datenverlust / Architektur-Fehler
**Datei:** `crates/memfuse-store/src/lsm.rs`
**Zeile(n):** 287-288
**Code (Kontext):**
```rust
// 2. Clear current memtable (it might have data > target_tx)
state.memtable = Arc::new(MemTable::new());
state.immutable_memtables.clear();
// ... SSTables bleiben unberührt ...
```
**Problem:** `rollback_to_tx` (die Basis für Time-Travel Debugging) löscht zwar MemTables und kürzt das WAL, lässt aber bereits geflushte SSTables unberührt. Wenn Daten einer Transaktion > `target_tx` bereits in ein SSTable geschrieben wurden, bleiben sie nach dem Rollback sichtbar.
**Auswirkung:** Inkonsistenter DB-Zustand nach Rollback; Time-Travel Debugging liefert falsche (zu neue) Daten.

**Refaktorisierungsanweisung:**
```
1. SSTables müssen beim Rollback ebenfalls berücksichtigt werden.
2. Jedes SSTable sollte in seinem Metadaten-Block die `max_tx_id` speichern.
3. Bei `rollback_to_tx` müssen alle SSTables, deren `min_tx_id > target_tx`, physisch gelöscht werden.
4. SSTables, die Überlappen (`min <= target < max`), müssen entweder neu geschrieben oder beim Lesen gefiltert werden.
```

**Akzeptanzkriterien:**
- [ ] Test `test_rollback_after_flush` beweist, dass Daten aus geflushten SSTables nach Rollback verschwinden.

---

#### FIND-STORE-002: CompactionEngine ignoriert Resource-Budget
**Typ:** Performance / Stabilität
**Datei:** `crates/memfuse-store/src/compaction.rs`
**Zeile(n):** 60-70 (Instanziierung)
**Problem:** Die `CompactionEngine` hat keinen Zugriff auf den `ResourceTracker` (`budget`). Sie kann während des Mergens von SSTables unbegrenzt RAM allozieren, ohne dass Backpressure angewendet wird oder das Budget respektiert wird.
**Auswirkung:** OOM-Crashes bei intensiver Compaction auf Systemen mit wenig RAM.

**Refaktorisierungsanweisung:**
```
1. Übergib `Arc<ResourceTracker>` an den `CompactionEngine` Konstruktor.
2. Nutze `budget.consume_memory()` während der Merge-Iteratoren.
3. Wende `budget.apply_backpressure().await` in der Compaction-Schleife an.
```

**Akzeptanzkriterien:**
- [ ] Compaction bricht ab oder pausiert, wenn das globale Memory-Budget erschöpft ist.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-STORE-003: Redundante Längen-Präfixe im WAL (Verschlüsselung)
**Typ:** Effizienz / Design
**Datei:** `crates/memfuse-store/src/wal.rs`
**Zeile(n):** 227-233
**Code (Kontext):**
```rust
let payload = &bytes[4..]; // Schneidet altes Längenpräfix ab
// ...
new_bytes.extend_from_slice(&(encrypted.len() as u32).to_le_bytes()); // Fügt neues hinzu
```
**Problem:** Die WAL-Logik fügt bei Verschlüsselung ein neues Längenpräfix hinzu, während das alte (aus `to_bytes`) ignoriert wird. Dies führt zu einer inkonsistenten Blockstruktur zwischen Plaintext- und Ciphertext-WALs.
**Auswirkung:** Komplexerer Recovery-Code; leicht erhöhte Disk-Usage.

**Refaktorisierungsanweisung:**
```
1. Vereinheitliche das WAL-Entry Format: `[Len][Flags][Data]`.
2. Flags geben an, ob `Data` verschlüsselt ist.
3. Vermeide doppeltes Wrapping/Unwrapping von Längenfeldern.
```

**Akzeptanzkriterien:**
- [ ] WAL-Entry Format ist konsistent dokumentiert und implementiert.

---

#### FIND-STORE-004: Fehlende Reparatur-Logik bei WAL-Korruption (HIGH-001)
**Typ:** Robustheit
**Datei:** `crates/memfuse-store/src/wal.rs`
**Zeile(n):** 365
**Problem:** Bei einer CRC-Fehlermeldung in der Mitte des WALs bricht `Wal::open` sofort mit einem Error ab. Dies verhindert den Start der DB, selbst wenn der Rest des WALs (oder SSTables) valide ist.
**Auswirkung:** DB bleibt nach Hardware-Fehler/Bitrot unstartbar.

**Refaktorisierungsanweisung:**
```
1. Implementiere einen "Recovery-Mode" in `Wal::open`.
2. Bei CRC-Fehler: Optionales Abschneiden des WALs an der Fehlerstelle (Truncate) mit Logging/Warnung.
```

**Akzeptanzkriterien:**
- [ ] DB startet nach "Middle-Corruption" im WAL (mit Datenverlust bis zum Fehlerpunkt).

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-STORE-001 (Korrektes Rollback inkl. SSTables)
Schritt 2: FIND-STORE-002 (Resource Budget Integration)
Schritt 3: FIND-STORE-004 (WAL Recovery Robustheit)
Schritt 4: FIND-STORE-003 (WAL Format Cleanup)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_compaction_respects_budget
// Provonziere Budget-Limit während Compaction und prüfe auf Backpressure/Abbruch.

// TEST-2: test_wal_middle_corruption_recovery
// Schreibe korruptes Byte in die Mitte des WALs und prüfe ob DB (mit Warnung) startet.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| `CompactionEngine::new`     | Nein (intern)| —                             |
| SSTable Metadata Format     | Ja        | Bestehende SSTables inkompatibel. |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Rollback-Operationen sind über MemTable, WAL und SSTables hinweg konsistent.
- [ ] Compaction Engine nutzt das globale Resource-Budget.
- [ ] WAL Recovery überlebt Bitrot/Mittel-Korruption.
- [ ] `just triple-test` 3× grün.
