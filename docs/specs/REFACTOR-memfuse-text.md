# REFACTOR-PLAN: memfuse-text
**Datei:** `docs/specs/REFACTOR-memfuse-text.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core, memfuse-store

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Skalierbarkeit     | ❌ SCHLECHT    | O(log N)      |
| DAG-Invarianz      | ⚠️ Dev-Dep-Leak| CLEAN         |
| Scoring-Stabilität | ✅ Gut         | VERIFIED      |
| Tracing            | ❌ FEHLT       | INSTRUMENTED  |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-TXT-003: Posting List Scalability (RMW-Antipattern)
**Typ:** Performance / Design-Fehler
**Datei:** `crates/memfuse-text/src/inverted.rs`
**Zeilen:** 197–209 (Upsert), 241–255 (Delete)
**Problem:** Für jeden Insert/Delete wird die komplette Posting-Liste eines Terms geladen, deserialisiert, manipuliert, reserialisiert und zurückgeschrieben.
**Auswirkung:** Bei 100k+ Dokumenten wird jeder Insert extrem langsam (>100ms pro Term), da die Posting-Listen Megabyte-Größen erreichen.
**Sovereign Core Verstoß:** Effizienz-Axiom verletzt.

**Refaktorisierungsanweisung:**
```
1. Ändere die Speicherung von Posting-Listen im LSM-Store.
2. Statt `key(pl:{term}) -> Vec<(DocId, tf)>` verwende:
   `key(pl:{term}:{doc_id}) -> tf (u32)`.
3. Vorteil: Inserts und Deletes sind nun atomare O(1) Write-Operationen im LSM-Tree.
4. Suche (`search_bm25`): Verwende `storage.scan_prefix(&key_with_term(term))` um alle DocIds für einen Term effizient zu iterieren.
```

**Akzeptanzkriterien:**
- [ ] Benchmark `bench_text_insert_scaling` beweist: Die Insert-Zeit bleibt nahezu konstant, egal ob 10 oder 10.000 Dokumente bereits im Index sind.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-TXT-001: DAG Invariance Cleanup
**Typ:** Architektur
**Datei:** `crates/memfuse-text/Cargo.toml`
**Problem:** `memfuse-store` ist in `dev-dependencies`.
**Lösung:**
```
1. Entferne `memfuse-store` komplett aus `Cargo.toml`.
2. Verschiebe Integrationstests, die echten Storage benötigen, in `crates/memfuse-db/tests`.
3. Nutze ausschließlich `MockStorage` innerhalb von `memfuse-text`.
```

---

#### FIND-TXT-004: Inconsistent Metadata Updates
**Typ:** Datenintegrität
**Datei:** `crates/memfuse-text/src/inverted.rs`
**Zeilen:** 154–190
**Problem:** `total_tokens` und `total_docs` werden als einzelne Keys geladen und gespeichert. Falls `upsert_document` abbricht, können diese Metadaten veraltet sein.

**Refaktorisierungsanweisung:**
```
1. Gruppiere alle Metadaten in ein `TextIndexMetadata` Struct.
2. Speichere dieses Struct unter einem einzigen Key `meta:stats` serialisiert ab.
3. Reduziert I/O und erhöht die Chance auf Konsistenz (da weniger get/put Zyklen).
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Sharded Posting Lists (PL-Key-Refactor) (TXT-003)
Schritt 2: Metadata Struct Konsolidierung (TXT-004)
Schritt 3: Cargo.toml Bereinigung (TXT-001)
```

## NEUE TESTS

```rust
// TEST-1: test_scalability_heavy_term
// 1. Inserte 1000 Dokumente mit dem Wort "the" (auch wenn es stopword ist, für den Test egal).
// 2. Messe Zeit für 1001. Insert.
// 3. Zeit muss < 1ms sein.

// TEST-2: test_prefix_scan_recovery
// 1. Inserte Dokumente.
// 2. Simuliere Suche über `scan_prefix`.
// 3. Verifiziere BM25 Scores gegen Erwartungswerte.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Posting-Listen sind auf Key-Level gesplittet.
- [ ] `memfuse-store` Abhängigkeit entfernt.
- [ ] `just triple-test -p memfuse-text` 3× grün.
