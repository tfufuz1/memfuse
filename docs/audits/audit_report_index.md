# Forensischer Audit-Bericht: memfuse-index

## 1. Executive Summary
- Gesamtbewertung: 🟡 Warning
- Anzahl Findings: 1 Kritisch (Compliance), 2 Mittel, 2 Niedrig
- Gesamteindruck: Hochperformante Implementierung mit exzellenter Nutzung von AVX2/AVX-512. Die Architektur ist durchdacht (Mmap-Support, Transaktions-Isolation). Punktabzug gibt es für die Verletzung des Determinismusgebots bei SIMD-Fallbacks.

## 2. Crate-Steckbrief
- LOC: ~11.800
- Module: [hnsw](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#2096-2140), [diskann](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs#889-923), [distance](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#149-154), [persistence](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#1108-1161), [quantize](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#155-160)
- Schlüsselkomponenten: HNSW-ANN Engine, SIMD Distance Layer, SQ8 Quantization.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ✅ | Panic-Scans bestätigen saubere Error-Handhabung. |
| Determinismus | ❌ | SIMD-Reduktionen (hsum) verändern FP-Endergebnisse gegenüber Scalar. |
| Memory-Safety | ✅ | Unsafe-Blöcke in [distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs) sind gut isoliert und dokumentiert. |
| OOM-Resilience | ✅ | DiskANN erlaubt Out-of-Core Betrieb für große Indizes. |

## 4. Findings

### FIND-IND-001: Determinismus-Bruch via SIMD-Assoziativität
- **Severity:** 🔴 Kritisch (Vorschriften-Verstoß)
- **Kategorie:** Determinismus / Compliance
- **Datei:** [crates/memfuse-index/src/distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs)
- **Beschreibung:** Artikel I §4 der Verfassung fordert strikten numerischen Determinismus zwischen SIMD- und skalaren Pfaden. Aktuell nutzen die SIMD-Implementationen (L211, L371) horizontale Summen ([hsum](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs#514-527)), die die Additionsreihenfolge von [f32](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#229-232) verändern.
- **Impact:** Identische Eingaben können auf verschiedenen CPUs (z.B. AVX2 vs. Scalar-Fallback) geringfügig unterschiedliche Distanzwerte liefern (+/- 1e-7), was in Edge-Cases die HNSW-Traversierung oder RRF-Scores (Signal Fusion) verändern kann.
- **Empfohlene Behebung:** Nutzung einer deterministischen Reduktionsmethode oder Kahan-Summation, falls höchste Präzision gefordert ist.
- **Aufwand:** M

### FIND-IND-002: Globale SQ8-Präzisionsverluste
- **Severity:** 🟡 Mittel
- **Kategorie:** Precision
- **Datei:** [crates/memfuse-index/src/quantize.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs)
- **Beschreibung:** Der [ScalarQuantizer](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs#16-23) berechnet ein globales Minimum/Maximum über alle Dimensionen des gesamten Batches (L37).
- **Impact:** Wenn einzelne Dimensionen von Embedding-Modellen (z.B. Layer-Norm Artefakte) stark unterschiedliche Wertebereiche haben, führt das globale Mapping zu massivem Präzisionsverlust in Dimensionen mit geringer Varianz.
- **Empfohlene Behebung:** Per-Dimension Quantisierung (SQ8-PD) implementieren.
- **Aufwand:** M

### FIND-IND-003: Nicht-portable Byte-Casts in HNSW-Save
- **Severity:** 🟡 Mittel
- **Kategorie:** Portabilität
- **Datei:** [crates/memfuse-index/src/hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs)
- **Zeile(n):** L372, L415
- **Beschreibung:** Nutzt `std::slice::from_raw_parts` um [f32](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#229-232) und `u32` direkt als Bytes zu schreiben.
- **Impact:** Das Dateiformat ist nicht Endian-agnostisch. Ein auf x86_64 gespeicherter Index wäre auf einer Big-Endian Architektur (z.B. PowerPC Edge) korrupt.
- **Empfohlene Behebung:** Explizites Iterieren mit `to_le_bytes()` für alle persistierten Werte.
- **Aufwand:** S

### FIND-IND-004: Ineffiziente Cache-Eviction in DiskANN
- **Severity:** 🟢 Niedrig
- **Kategorie:** Performance
- **Datei:** [crates/memfuse-index/src/diskann.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs)
- **Zeile(n):** L616
- **Beschreibung:** [load_node](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs#541-622) löscht den kompletten Cache (`cache.clear()`), wenn das Budget erreicht ist.
- **Impact:** Führt unter Last zu periodischen Performance-Einbrüchen ("Thundering Herd" Effekt beim Wiederaufbau des Caches).
- **Aufwand:** S

## 5. Test-Gap-Analyse
- **Präzisions-Tests:** Fehlende Tests für den numerischen Delta-Check zwischen SIMD- und skalaren Implementierungen.
- **OOM-Hardening:** Es fehlen Tests, die simulieren, wie DiskANN sich verhält, wenn der Mmap-Bereich die Dateigröße überschreitet (Korrekt abgefangen in L561, aber ungetestet).

## 6. Empfehlungen (priorisiert)
1. **[Kritisch]** Numerischen Determinismus prüfen und dokumentieren, ob Abweichungen im RRF-Kontext tolerabel sind.
2. **[Mittel]** Auf Per-Dimension Quantisierung umstellen, um die Recall-Rate zu erhöhen.
