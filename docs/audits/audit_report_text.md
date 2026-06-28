# Forensischer Audit-Bericht: memfuse-text

## 1. Executive Summary
- Gesamtbewertung: 🔴 Warning (Kritische Architektur-Mängel)
- Anzahl Findings: 2 Kritisch (Performance/ACID), 2 Mittel
- Gesamteindruck: Die morphologische Inferenz (German Compound Splitting) ist exzellent implementiert. Die Integration in den LSM-Store leidet jedoch unter einem ineffizienten Update-Design und fehlender Isolation im Lesepfad.

## 2. Crate-Steckbrief
- LOC: ~2.500
- Module: `inverted`, [bm25](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#355-440), [tokenizer](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#509-512), `morphology`
- Schlüsselkomponenten: BM25 Scorer, Inverted Index (LSM-backed), German Morph Tokenizer.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ✅ | Keine Panics in der Release-Logik. |
| Determinismus | ✅ | BM25-Scores und Tokenisierung sind deterministisch. |
| ACID-Isolation | ❌ | [search_bm25](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#355-440) führt Präfix-Scans ohne Snapshot/TxId durch. |
| Performance | ❌ | Tombstone-Resolution hat quadratische Komplexität $O(T \times PL)$. |

## 4. Findings

### FIND-TXT-001: Dirty Reads im Suchpfad (ACID-Bruch)
- **Severity:** 🔴 Kritisch (Vorschriften-Verstoß)
- **Kategorie:** Korrektheit / Transaktion
- **Datei:** [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs)
- **Zeile(n):** L384
- **Beschreibung:** Die Methode [search_bm25](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#355-440) führt `storage.scan_prefix()` aus, ohne einen Snapshot oder eine [TxId](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#103-104) zu verwenden.
- **Impact:** Suchanfragen sehen uncommitted Daten oder inkonsistente Zustände während laufender Schreibvorgänge (Phantome). Dies verletzt die Invariante der Isolation.
- **Empfohlene Behebung:** Erweiterung der `TextIndex::search` Signatur um einen optionalen Snapshot-Guard.
- **Aufwand:** M

### FIND-TXT-002: Quadratischer Performance-Bottleneck bei Tombstones
- **Severity:** 🔴 Kritisch (Architektur-Fehler)
- **Kategorie:** Performance
- **Datei:** [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs)
- **Zeile(n):** L277
- **Beschreibung:** Die Methode [resolve_tombstones](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#217-306) scannt für *jeden* zu bereinigenden Tombstone den *gesamten* `pl:` (Posting List) Namensraum des Storage-Engines.
- **Impact:** Bei einer großen Anzahl von Dokumenten und Termen führt dies zu $O(Tombstones \times PostingLines)$ Komplexität. Das System wird bei häufigen Updates unbenutzbar.
- **Empfohlene Behebung:** Einführung eines Forward-Index (Posting-per-Doc), um gezielt aufräumen zu können (`pd:{doc_id}:{term}`).
- **Aufwand:** H

### FIND-TXT-003: Ineffiziente Posting-List Granularität
- **Severity:** 🟡 Mittel
- **Kategorie:** Performance / Storage
- **Datei:** [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs)
- **Beschreibung:** Jeder Term-Doc-Eintrag ist ein eigener Key-Value-Eintrag im LSM-Store (`pl:term:doc_id`).
- **Impact:** Extrem hoher Overhead für die Metadaten des Storage-Engines. Ineffiziente Kompression.
- **Empfohlene Behebung:** Gruppierung von Posting-Listen in Blobs oder die Nutzung von Skip-Lists innerhalb der Werte.
- **Aufwand:** H

### FIND-TXT-004: Fehlendes Caching für globale Statistiken
- **Severity:** 🟡 Mittel
- **Kategorie:** Performance
- **Datei:** [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs)
- **Zeile(n):** L366
- **Beschreibung:** `total_docs` und [avg_doc_len](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs#70-76) werden bei jeder Suchanfrage direkt aus dem Storage geladen und bincode-deserialisiert.
- **Impact:** Unnötige Latenz im Hot-Path der Suche.
- **Empfohlene Behebung:** In-Memory Cache mit atomaren Updates für globale Metriken.
- **Aufwand:** S

## 5. Empfehlungen (priorisiert)
1. **[Kritisch]** Korrektur der Isolation: Suchpfad muss Snapshot-Isolation nutzen.
2. **[Kritisch]** Redesign der [resolve_tombstones](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs#217-306) Logik.
3. **[Mittel]** Implementierung eines effizienteren binären Posting-Listen-Formats.
