# REFACTOR-PLAN: memfuse-db
**Datei:** `docs/specs/REFACTOR-memfuse-db.md`
**Erstellt:** 2026-05-27
**Priorität:** HIGH
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core, memfuse-store, memfuse-index, memfuse-text

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 1 Stelle      | 0             |
| Test-Coverage      | ~70%          | >85%          |
| API-Vollständigkeit| 90%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-DB-001: Hybrid Search ignoriert Signal-Gewichtung (FusionWeights)
**Typ:** Architektur-Lücke / Inkorrektheit
**Datei:** `crates/memfuse-db/src/collection.rs`
**Zeile(n):** 760-763
**Code (Kontext):**
```rust
let text_results = self.hydrate_from_tuples(bm25_results).await?;
let vector_results = self.search(vector, k).await?;
Ok(crate::fusion::reciprocal_rank_fusion(
    vec![vector_results, text_results],
    k,
))
```
**Problem:** Die `hybrid_search` Funktion nutzt zwar RRF, ignoriert aber die in `memfuse-core` definierten `FusionWeights`. Alle Signale (Vektor, Text) werden immer mit dem gleichen Gewicht (1.0) fusioniert. Nutzer können die Relevanz nicht feinsteuern.
**Auswirkung:** Mangelnde Flexibilität bei der Such-Optimierung; `HybridQuery` Parameter werden ignoriert.

**Refaktorisierungsanweisung:**
```
1. Erweitere `reciprocal_rank_fusion`, um eine Liste von Gewichten zu akzeptieren.
2. In `hybrid_search`: Nutze die Gewichte aus der Query (oder Default-Werte), um die RRF-Scores zu skalieren: `score = weight * (1.0 / (k + rank))`.
3. Stelle sicher, dass auch der Graph-Signal (Signal 3) in die Fusion einfließt, falls vorhanden.
```

**Akzeptanzkriterien:**
- [ ] Test `test_hybrid_search_respects_weights` beweist, dass unterschiedliche Gewichte die Ranking-Reihenfolge beeinflussen.

---

#### FIND-DB-002: Gefahr von Key-Collisions im default Namespace
**Typ:** Logik-Fehler / Datenkorruption
**Datei:** `crates/memfuse-db/src/collection.rs`
**Zeile(n):** 90-94
**Code (Kontext):**
```rust
let prefix = if name == "default" {
    b"".to_vec()
} else {
    format!("__col:{}:\x00", name).into_bytes()
};
```
**Problem:** Der `default` Namespace nutzt keinen Prefix. Falls ein Benutzer Keys eingibt, die mit `__col:` beginnen, können diese mit internen Keys anderer Namespaces kollidieren.
**Auswirkung:** Daten-Leckage zwischen Namespaces; Korruption interner Index-Strukturen.

**Refaktorisierungsanweisung:**
```
1. Gib auch dem `default` Namespace einen eindeutigen Prefix (z.B. `__col:default:\x00`).
2. Implementiere eine Migrations-Routine oder sorge für Abwärtskompatibilität, indem alte Keys ohne Prefix weiterhin gefunden werden (Read-Fallback), aber neue Keys immer mit Prefix geschrieben werden.
```

**Akzeptanzkriterien:**
- [ ] Interner Prefix-Raum (`__col:`, `__txt:`, `__tx_intent:`) ist strikt von User-Daten getrennt.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-DB-003: Fehlendes Tracing auf kritischen API-Pfaden
**Typ:** Observability Debt
**Datei:** `crates/memfuse-db/src/lib.rs` & `collection.rs`
**Problem:** Zentrale Methoden wie `insert`, `commit`, `hybrid_search` und `repair_on_open` haben keine `tracing::instrument` Annotationen. Latenz-Analysen in Produktion sind dadurch unmöglich.
**Auswirkung:** Blindheit bei Performance-Problemen; erschwerte Fehlersuche in verteilten Systemen.

**Refaktorisierungsanweisung:**
```
1. Füge `#[tracing::instrument(skip(self, ...))]` zu allen public API Methoden hinzu.
2. Logge wichtige Meilensteine (z.B. "Compensating transaction started").
```

**Akzeptanzkriterien:**
- [ ] `cargo check` mit tracing feature ist erfolgreich.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-DB-002 (Prefix-Sicherheit vor Daten-Migration)
Schritt 2: FIND-DB-001 (FusionWeights Integration)
Schritt 3: FIND-DB-003 (Tracing/Observability)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_namespace_prefix_collision_protection
// Versuche einen Key "__col:other:\x00test" im default namespace zu speichern.
// Prüfe, ob er korrekt geprefixt wird und NICHT den Namespace 'other' korrumpiert.

// TEST-2: test_rrf_with_custom_weights
// Setze weight(vector)=0.1 und weight(text)=0.9.
// Verifiziere, dass Text-Ergebnisse dominieren.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| `hybrid_search` Signatur    | Nein      | Nutzt nun intern Gewichte.     |
| Default-Namespace Prefix    | Ja (Disk) | Automatisierte Key-Migration nötig. |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Hybrid-Search nutzt das vollständige `FusionWeights` Modell.
- [ ] Alle Namespaces sind physisch durch Prefixe isoliert.
- [ ] Tracing deckt alle kritischen Pfade ab (100%).
- [ ] `just triple-test` 3× grün.
