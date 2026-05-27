# REFACTOR-PLAN: memfuse-index
**Datei:** `docs/specs/REFACTOR-memfuse-index.md`
**Erstellt:** 2026-05-27
**Priorität:** HIGH
**Geschätzter Aufwand:** 3 Tage
**Voraussetzung:** memfuse-core, memfuse-graph

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | ~85%          | >95%          |
| API-Vollständigkeit| 95%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-INDEX-001: Lückenhafte SAFETY-Dokumentation für SIMD
**Typ:** Sicherheit / Sovereign Core Compliance
**Datei:** `crates/memfuse-index/src/distance.rs`
**Zeile(n):** Diverse (ca. 42 Stellen)
**Problem:** Viele `unsafe` Blöcke für AVX2/AVX-512 Intrinsics verfügen über keine oder nur sehr vage `SAFETY:` Kommentare. Dies verstößt gegen die Sovereign Core Doctrine und verhindert eine qualifizierte Sicherheitsüberprüfung.
**Auswirkung:** Erhöhtes Risiko für Buffer Overflows bei ungeraden Vektordimensionen; Schwer wartbarer Code.

**Refaktorisierungsanweisung:**
```
1. Jeder `unsafe` Block MUSS einen `// SAFETY:` Kommentar erhalten.
2. Der Kommentar muss beweisen, warum der Zugriff sicher ist (z.B. "Bounds checked in caller", "SIMD alignment verified").
3. Entferne `#![allow(unsafe_code)]` auf Crate-Ebene und nutze stattdessen gezielte `allow` Attribute an den Funktionen, wo intrinsics unvermeidbar sind.
```

**Akzeptanzkriterien:**
- [ ] `grep -c "unsafe {"` entspricht der Anzahl der `// SAFETY:` Kommentare.
- [ ] Alle Intrinsics sind gegen Out-of-Bounds geschützt (Tail-Handling für Vektoren, deren Länge kein Vielfaches von 8/16 ist).

---

#### FIND-INDEX-002: Fehlende NaN/Inf Validierung (Poisoning Gefahr)
**Typ:** Algo-Korrektheit
**Datei:** `crates/memfuse-index/src/hnsw.rs`
**Zeile(n):** 900 (insert_internal)
**Problem:** Eingehende Vektoren werden nicht auf `NaN` oder `Infinity` geprüft. Gelangen solche Werte in den HNSW-Index, werden Distanzberechnungen zu `NaN`, was die Greedy-Suche bricht (Abbruchbedingungen schlagen fehl) und zu Endlosschleifen oder falschen Ergebnissen führt.
**Auswirkung:** Index-Poisoning; Absturz der Suchfunktionalität für alle Nutzer.

**Refaktorisierungsanweisung:**
```
1. Füge eine Validierung in `insert` und `insert_batch` hinzu.
2. Nutze `v.iter().all(|x| x.is_finite())`.
3. Gib `MemFuseError::InvalidInput` zurück, falls der Vektor nicht finit ist.
```

**Akzeptanzkriterien:**
- [ ] Test `test_reject_nan_vector` provoziert den Fehler.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-INDEX-003: Ineffizienter Rebuild-Schwellenwert
**Typ:** Performance
**Datei:** `crates/memfuse-index/src/hnsw.rs`
**Zeile(n):** 90 (HnswConfig Default)
**Code (Kontext):**
```rust
rebuild_threshold: 0.8, // 80% deleted nodes
```
**Problem:** Ein Rebuild-Schwellenwert von 80% ist viel zu hoch. Ab ca. 20-30% gelöschten Knoten (Tombstones) sinkt der Recall und die Suchgeschwindigkeit massiv, da der Graph fragmentiert.
**Auswirkung:** Schlechte Performance im Long-Tail Betrieb.

**Refaktorisierungsanweisung:**
```
1. Senke den Default-Wert für `rebuild_threshold` auf 0.2 (20%).
2. Stelle sicher, dass der Hintergrund-Rebuild das Memory-Budget (ResourceTracker) respektiert.
```

**Akzeptanzkriterien:**
- [ ] Default ist 0.2.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-INDEX-002 (Schutz vor Poisoning)
Schritt 2: FIND-INDEX-001 (Safety Audit)
Schritt 3: FIND-INDEX-003 (Rebuild-Optimierung)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_simd_alignment_boundary
// Testet Vektoren mit Längen 7, 8, 9, 15, 16, 17 um SIMD Tail-Handling zu prüfen.

// TEST-2: test_recall_after_deletions
// Lösche 25% der Daten und verifiziere, dass der Rebuild korrekt triggert und der Recall stabil bleibt.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| `insert` Validierung        | Nein      | Liefert nun Error bei NaN.    |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Alle `unsafe` Blöcke dokumentiert und verifiziert sind.
- [ ] Kein NaN/Inf Vektor den Index vergiften kann.
- [ ] Rebuild-Logik ist performant und budget-bewusst.
- [ ] `just triple-test` 3× grün.
