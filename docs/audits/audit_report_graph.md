# Forensischer Audit-Bericht: memfuse-graph

## 1. Executive Summary
- Gesamtbewertung: 🟡 Warning
- Anzahl Findings: 1 Kritisch (Architektur), 1 Mittel, 1 Niedrig
- Gesamteindruck: Die CSR-Implementierung ist sauber und nutzt effiziente Speicherlayouts. Die Transaktions-Isolation über Staging-Areas ist vorbildlich. Jedoch fehlt die Anbindung an die Persistenzschicht vollständig, was den Graph-Signal (Signal 3) flüchtig macht.

## 2. Crate-Steckbrief
- LOC: ~710
- Module: [csr](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs#630-648)
- Schlüsselkomponenten: Compressed Sparse Row (CSR) Layout, BFS mit Score-Decay, Staged Transactions.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ✅ | Keine ungeschützten Unwraps im Produktionspfad. |
| Determinismus | ✅ | BFS-Scores sind deterministisch (0.7^hop). |
| Durability | ❌ | Graph-Zustand wird weder in `memfuse-store` noch eigenständig persistiert. |
| Isolation | ✅ | [traverse](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs#222-297) sieht nur committed & compacted Zustände. |

## 4. Findings

### FIND-GRA-001: Volatile-Only Architecture (Durability Deficit)
- **Severity:** 🔴 Kritisch (Architektur)
- **Kategorie:** Durability
- **Datei:** [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs)
- **Beschreibung:** Die Crate bietet keine Mechanismen zum Speichern oder Laden des CSR-Zustands. Während `memfuse-index` einen eigenen Persistence-Layer für HNSW hat, fehlt dieser für den Relation-Graph (Signal 3).
- **Impact:** Nach einem Neustart des Systems sind alle Graph-Relationen verloren, sofern sie nicht manuell aus einer externen Quelle (oder durch Replay aller Transaktionen) neu aufgebaut werden.
- **Empfohlene Behebung:** Integration in den `memfuse-store` WAL oder Implementierung eines `.graph` Binärformats analog zu `memfuse-index`.
- **Aufwand:** H

### FIND-GRA-002: O(N+E) Compaction Bottleneck
- **Severity:** 🟡 Mittel
- **Kategorie:** Performance
- **Datei:** [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs)
- **Zeile(n):** L83-136
- **Beschreibung:** Die [compact()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs#82-137) Methode baut bei jeder Änderung die komplette CSR-Struktur (Offsets, Targets, Weights) neu auf.
- **Impact:** Bei großen Graphen (Millionen von Kanten) führen bereits kleine Transaktionen zu massiven CPU-Bursts und Speicher-Allokationen beim Traversal, da die Komplexität linear mit der Gesamtgröße des Graphen skaliert.
- **Empfohlene Behebung:** Inkrementelle Compaction oder Nutzung eines Adjazenzlisten-Zwischenformats, das nur partiell in CSR überführt wird.
- **Aufwand:** M

### FIND-GRA-003: Hardcoded Traversal Limits
- **Severity:** 🟢 Niedrig
- **Kategorie:** Design
- **Datei:** [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs)
- **Zeile(n):** L20
- **Beschreibung:** `MAX_TRAVERSAL_HOPS` ist fest auf 3 kodiert.
- **Impact:** Schränkt die Flexibilität für tiefere Ontologien oder Knowledge-Graphs ein, ohne dass dies zur Laufzeit konfigurierbar ist.
- **Aufwand:** S

## 5. Test-Gap-Analyse
- **Stress-Tests:** Die aktuellen Tests decken funktionale Korrektheit ab, aber keine Szenarien mit >100k Kanten, bei denen das `O(N+E)` Rebuild-Verhalten kritisch wird.

## 6. Empfehlungen (priorisiert)
1. **[Kritisch]** Persistenz-Layer für CSR-Strukturen entwerfen.
2. **[Mittel]** Inkrementelles Merging der `committed_staged` Edges implementieren.
