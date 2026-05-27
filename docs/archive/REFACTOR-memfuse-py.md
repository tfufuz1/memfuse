# REFACTOR-PLAN: memfuse-py
**Datei:** `docs/specs/REFACTOR-memfuse-py.md`
**Erstellt:** 2026-05-27
**Priorität:** MEDIUM
**Geschätzter Aufwand:** 1 Tag
**Voraussetzung:** memfuse-db

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | ~60% (Rust)   | >80%          |
| API-Vollständigkeit| 95%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-PY-001: Verlusbehaftetes Error-Mapping
**Typ:** API-Qualität / Usability
**Datei:** `crates/memfuse-py/src/lib.rs`
**Zeile(n):** 96-98
**Code (Kontext):**
```rust
fn memfuse_err<E: std::fmt::Display>(e: E) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
}
```
**Problem:** Alle Rust-Fehler werden pauschal auf `PyRuntimeError` gemappt. Python-Nutzer können dadurch nicht gezielt auf bestimmte Fehlerzustände (z.B. `KeyNotFound`, `MemoryBudgetExceeded`) reagieren, ohne den Fehler-String zu parsen.
**Auswirkung:** Erschwerte Fehlerbehandlung in Python-Applikationen.

**Refaktorisierungsanweisung:**
```
1. Implementiere ein strukturiertes Mapping von `MemFuseError` auf native Python-Exceptions.
2. `MemFuseError::NotFound` -> `PyKeyError`.
3. `MemFuseError::InvalidInput` -> `PyValueError`.
4. `MemFuseError::MemoryBudgetExceeded` -> `PyMemoryError`.
5. Erzeuge ggf. eine eigene `MemFuseError` Exception-Klasse in Python für interne Fehler.
```

**Akzeptanzkriterien:**
- [ ] Python-Code `try: db.get("missing") except KeyError: ...` funktioniert.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-PY-002: Ineffizientes Cloning in Batch-Operationen
**Typ:** Performance
**Datei:** `crates/memfuse-py/src/lib.rs`
**Zeile(n):** 330, 355
**Code (Kontext):**
```rust
let v = vector.as_slice().map_err(...).to_vec(); // CLONE
batch.push((id.clone(), v, m));
```
**Problem:** Bei `insert_many` werden alle Vektoren von NumPy-Slices in neue Rust-`Vec<f32>` geklont, während das GIL noch gehalten wird (oder kurz davor). Bei sehr großen Batches führt dies zu hohem Speicher- und Zeit-Overhead.
**Auswirkung:** Langsame Batch-Importe in Python.

**Refaktorisierungsanweisung:**
```
1. Nutze `py.allow_threads` bereits während der Konvertierung, falls möglich.
2. Prüfe, ob `memfuse-db` Batches von Slices (`&[f32]`) statt `Vec<f32>` akzeptieren kann, um das Klonen ganz zu vermeiden (da die Daten bereits in NumPy-Arrays liegen).
```

**Akzeptanzkriterien:**
- [ ] Batch-Import-Performance steigt bei großen Datensätzen (>100k Vektoren).

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-PY-001 (Exception-Handling)
Schritt 2: FIND-PY-002 (Batch-Optimierung)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```python
# Python-seitige Tests (pytest)

# TEST-1: test_exception_mapping
# def test_not_found():
#     with pytest.raises(KeyError):
#         db.get("non-existent")

# TEST-2: test_batch_efficiency
# Messe Zeit für 10.000 Vektoren.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| Geänderte Exception-Typen   | Ja        | `except RuntimeError` zu `except KeyError` ändern. |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Alle Rust-Errors haben eine sinnvolle Entsprechung in Python.
- [ ] Batch-Konvertierungen sind GIL-schonend und speichereffizient.
- [ ] `just triple-test` (inkl. Python-Smoke-Tests) grün.
