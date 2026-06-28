# Forensischer Audit-Bericht: memfuse-db

## 1. Executive Summary
- Gesamtbewertung: 🔴 Kritisch
- Anzahl Findings: 3 Kritisch (Souveränität/Stabilität), 2 Mittel, 1 Niedrig
- Gesamteindruck: Die Orchestrierungsschicht ist hochgradig komplex und implementiert ein mutiges 2-Phase-Commit System für die Konsistenz zwischen LSM und HNSW. Jedoch verletzen fundamentale Implementierungsdetails (Panic-Sicherheit, Ressourcen-Management) die Verfassung des Projekts.

## 2. Crate-Steckbrief
- LOC: ~10.000
- Module: [collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs#127-165), [transaction](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs#275-278), [fusion](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs#189-193), `chunker`, [namespace](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#105-138)
- Schlüsselkomponenten: 4-Signal Fusion Orchestrator, RRF-Ranking, 2PC Transaction Manager.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ❌ | Mehrere `unwrap()` Aufrufe in [lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs) (SandboxBridge). |
| Ressourcen-Isolation | ❌ | [drop_collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#932-951) löscht keine Daten im LSM-Store (Storage Leak). |
| ACID-Isolation | ❌ | Queries nutzen keine Snapshots (Dirty Reads möglich). |
| Souveränität | ✅ | Keine externen Cloud-Abhängigkeiten gefunden. |

## 4. Findings

### FIND-DB-001: Verfassungsverstoß §2 — Panics in SandboxBridge
- **Severity:** 🔴 Kritisch (Vorschriften-Verstoß)
- **Kategorie:** Sicherheit / Stabilität
- **Datei:** [crates/memfuse-db/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs)
- **Zeile(n):** L290, L294, L305
- **Beschreibung:** Artikel I §2 verbietet `unwrap()` im Release-Code. Die `SandboxBridge` Implementierung nutzt `unwrap()` bei der Serialisierung/Deserialisierung von JSON-Ergebnissen.
- **Impact:** Ein korruptes oder unerwartet großes Suchergebnis kann den gesamten Datenbankprozess (Sovereign Core) zum Absturz bringen.
- **Empfohlene Behebung:** Ersetzung durch `?` und Mapping auf `MemFuseError`.
- **Aufwand:** S

### FIND-DB-002: Ressourcen-Isolation — Storage Leak bei [drop_collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#932-951)
- **Severity:** 🔴 Kritisch (Architektur-Fehler)
- **Kategorie:** Ressourcen
- **Datei:** [crates/memfuse-db/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs)
- **Zeile(n):** L190ff
- **Beschreibung:** [drop_collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#932-951) entfernt die Collection aus dem In-Memory Index und dem Management-Hashmap, führt aber keine `delete_range` Operation auf dem LSM-Store aus.
- **Impact:** Gelöschte Collections belegen permanent Speicherplatz im LSM-Store. Bei vielen kurzlebigen Collections droht Disk-OOM (Verstoß gegen Ressourcen-Limes §3).
- **Empfohlene Behebung:** Implementierung einer `storage.delete_prefix()` Logik innerhalb von [drop_collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#932-951).
- **Aufwand:** M

### FIND-DB-003: ACID-Bruch — Fehlende Snapshot-Isolation
- **Severity:** 🔴 Kritisch (Vorschriften-Verstoß)
- **Kategorie:** Korrektheit
- **Datei:** [crates/memfuse-db/src/collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs)
- **Beschreibung:** Die Such- und Filterpfade ([search_with_filter](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#646-710), [hydrate_from_tuples](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#756-779)) nutzen kein Snapshot-System des LSM-Stores.
- **Impact:** Suchergebnisse können inkonsistent sein, wenn während der Hydrierung (L767ff) gleichzeitig Schreibvorgänge stattfinden (Phantome, Non-Repeatable Reads).
- **Empfohlene Behebung:** Alle Lesevorgänge müssen einen [SnapshotGuard](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#102-106) von `memfuse-core` anfordern und nutzen.
- **Aufwand:** H

### FIND-DB-004: Ineffizienter Repair-Mechanismus (HNSW-Audit)
- **Severity:** 🟡 Mittel
- **Kategorie:** Performance
- **Datei:** [crates/memfuse-db/src/collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs)
- **Zeile(n):** L144ff
- **Beschreibung:** [repair()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#139-222) führt für jedes Dokument im Storage eine $k=1$ Suche im HNSW-Index durch, um die Präsenz zu prüfen.
- **Impact:** Bei Millionen von Dokumenten dauert ein Repair-on-Open Stunden.
- **Empfohlene Behebung:** Nutzung der `doc_to_node` Map oder eines Bloom-Filters zur Präsenzprüfung.
- **Aufwand:** M

### FIND-DB-005: Split-Brain Risiko bei 2PC-Kompensation
- **Severity:** 🟡 Mittel
- **Kategorie:** Verlässlichkeit
- **Datei:** [crates/memfuse-db/src/transaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/transaction.rs)
- **Zeile(n):** L173
- **Beschreibung:** Scheitert die Kompensationstransaktion (Schritt 3 des Commits) nach 3 Versuchen, verbleibt das System im inkonsistenten Zustand.
- **Impact:** Datenverlust oder Divergenz zwischen Index und Storage.
- **Empfohlene Behebung:** Persistierung von "Commit-Intents" in einem separaten Recovery-Log.
- **Aufwand:** H

## 5. Empfehlungen (priorisiert)
1. **[Sofort]** Fix der `SandboxBridge` Panics zur Wiederherstellung der §2 Compliance.
2. **[Kritisch]** Snapshot-Isolation in [Collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#53-62) implementieren.
3. **[Kritisch]** Präfix-Cleanup in [drop_collection](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#932-951) hinzufügen.
