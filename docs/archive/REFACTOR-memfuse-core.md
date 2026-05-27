# REFACTOR-PLAN: memfuse-core
**Datei:** `docs/specs/REFACTOR-memfuse-core.md`
**Erstellt:** 2026-05-27
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 1 Tag
**Voraussetzung:** Keine (Layer 0)

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 5 Methoden    | 0             |
| Test-Coverage      | ~75%          | >90%          |
| API-Vollständigkeit| 90%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-CORE-001: Gefährliche No-Op Default-Implementierungen in Traits
**Typ:** Skeleton / Datenverlust / Algo-Fehler
**Datei:** `crates/memfuse-core/src/traits.rs`
**Zeile(n):** 98, 107, 72, 142
**Code (Kontext):**
```rust
async fn rollback(&self, _tx_id: TxId) -> Result<()> {
    Ok(()) // SKELETON: Tut nichts, suggeriert Erfolg
}

async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
    Ok(()) // SKELETON: Kritischer Datenverlust-Pfad ignoriert
}

async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
    let _ = seq;
    self.get(key).await // Ignoriert MVCC-Parameter
}
```
**Problem:** Die Traits `StorageEngine` und `VectorIndex` bieten Default-Implementierungen für kritische Operationen (Rollback, MVCC-Read, Filtered Search) an, die nichts tun oder Parameter ignorieren. Ein Implementierer, der diese Methoden vergisst, erzeugt ein System, das scheinbar funktioniert, aber bei Fehlern keine Transaktionssicherheit bietet oder falsche Daten (neueste statt MVCC-Version) zurückgibt.
**Auswirkung:** Dateninkonsistenz nach Rollback-Versuchen; Verletzung der MVCC-Isolation; Falsche Suchergebnisse bei gefilterter Suche.

**Refaktorisierungsanweisung:**
```
1. Entferne alle Default-Implementierungen für Methoden, die für die Korrektheit essenziell sind (rollback, rollback_to_tx, get_at_seq, insert, delete, search).
2. Falls eine Default-Implementierung für Mocks/Tests nötig ist, sollte sie `Err(MemFuseError::Internal("Not implemented".into()))` zurückgeben statt `Ok(())`.
3. Markiere Methoden, die optional sind, explizit (z.B. durch Rückgabe eines leeren Vektors bei scan, falls das das gewünschte Verhalten für eine leere Engine ist).
4. Entferne `#[allow(async_fn_in_trait)]` und stelle sicher, dass die Traits korrekt für dyn-Kompatibilität oder statisches Dispatching ausgelegt sind.
```

**Akzeptanzkriterien:**
- [ ] `cargo check` schlägt bei Implementierungen fehl, die `rollback` nicht überschreiben.
- [ ] Alle Implementierer (LsmStorage, HnswIndex) implementieren die Methoden explizit.
- [ ] Keine "Silent Skeletons" mehr im Trait-Kontrakt.

---

#### FIND-CORE-002: Atomarer Underflow in ResourceTracker::release_memory
**Typ:** Panic (Wrap) / DoS
**Datei:** `crates/memfuse-core/src/types/budget.rs`
**Zeile(n):** 57-60
**Code (Kontext):**
```rust
pub fn release_memory(&self, bytes: u64) {
    self.memory_used
        .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
}
```
**Problem:** `fetch_sub` auf einem `AtomicU64` führt bei Unterlauf (wenn `bytes > memory_used`) zu einem Wrap-around auf einen Wert nahe `u64::MAX`.
**Auswirkung:** Sobald der Tracker in diesen Zustand gerät, schlagen alle zukünftigen `consume_memory`-Aufrufe fehl, da das Budget als massiv überschritten gilt. Das System ist effektiv im DoS-Zustand.

**Refaktorisierungsanweisung:**
```
1. Ersetze `fetch_sub` durch eine CAS-Schleife (compare_exchange).
2. Implementiere `saturating_sub` Logik: Wenn `bytes > current`, setze auf 0.
3. Füge ein `tracing::warn!` hinzu, falls versucht wird, mehr Speicher freizugeben als belegt ist (Indikator für Leaks oder Doppel-Freigabe).
```

**Akzeptanzkriterien:**
- [ ] Test `test_release_memory_underflow_safety` existiert und provoziert den Fall.
- [ ] `memory_used` wird niemals ein astronomisch hoher Wert durch Underflow.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-CORE-003: Diskrepanz zwischen Architektur-Dokumentation und Realität
**Typ:** Technischer Schulden / Dokumentationsfehler
**Datei:** `crates/memfuse-core/src/lib.rs`
**Zeile(n):** 18
**Code (Kontext):**
```rust
// INVARIANTE: Kein I/O, kein async, kein Netzwerk — reine Datentypen + Traits.
```
**Problem:** Die Invariante behauptet "kein async", aber die Crate nutzt `tokio`, `async-trait` und implementiert einen Hintergrund-Thread (`orphan_reaper`) via `tokio::spawn`. Dies führt zu Verwirrung bei Entwicklern und bricht das Versprechen einer "reinen Typ-Crate".
**Auswirkung:** Architektur-Erosion; Erschwerte Portierbarkeit (z.B. nach WASM ohne Tokio-Support).

**Refaktorisierungsanweisung:**
```
1. Korrigiere die Dokumentation in `lib.rs` und `traits.rs`.
2. Extrahiere den `orphan_reaper` und die `tokio`-Abhängigkeit idealerweise in eine separate Crate (z.B. `memfuse-runtime` oder `memfuse-db`), damit `core` rein für Datentypen bleibt.
3. Falls die Extraktion zu aufwendig ist: Markiere `tokio`-abhängige Features in `core` als optionales Cargo-Feature `runtime`.
```

**Akzeptanzkriterien:**
- [ ] Dokumentation stimmt mit Code überein.
- [ ] (Optional) `core` kompiliert ohne `tokio`-Feature.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-CORE-002 (Kritischer Bugfix für Stabilität)
Schritt 2: FIND-CORE-001 (Erzwingen korrekter Implementierungen)
Schritt 3: Anpassung von memfuse-store und memfuse-index an die neuen Trait-Kontrakte.
Schritt 4: FIND-CORE-003 (Dokumentation & Feature-Gating)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_release_memory_underflow_safety
// Testet: tracker.release_memory(100) bei Stand 50.
// Assert: tracker.memory_used() == 0 (kein Wrap-around).

// TEST-2: test_trait_completeness_storage
// Testet: Eine Dummy-Struktur implementiert StorageEngine.
// Prüft ob der Compiler zur Implementierung von rollback zwingt.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| Entfernen von Trait-Defaults| Ja        | Alle Implementierer müssen die Methoden explizit definieren. |
| Feature-Gating von Tokio    | Eventuell | Cargo.toml der Downstream-Crates anpassen. |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Alle BLOCKING-Findings behoben.
- [ ] `just triple-test` in `memfuse-core` 3× grün.
- [ ] `cargo check -p memfuse-db` erfolgreich (Integrationstest).
- [ ] Keine Underflow-Gefahr im ResourceTracker mehr.
- [ ] `traits.rs` enthält keine unsicheren Default-No-Ops mehr.
