# Forensischer Audit-Bericht: memfuse-store

## 1. Executive Summary
- Gesamtbewertung: 🔴 Danger
- Anzahl Findings: 1 Kritisch, 2 Mittel, 1 Niedrig
- Gesamteindruck: `memfuse-store` ist das Herzstück der Persistenz. Die Implementierung von WAL und MemTable ist robust und nutzt starke kryptographische Garantien. Jedoch wurde ein fundamentaler Fehler in der Compaction-Logik gefunden, der zu Datenkorruption (Wiederauftauchen gelöschter Daten) führen kann.

## 2. Crate-Steckbrief
- LOC: ~5.014
- Module: [wal](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs#121-131), `memtable`, [sstable](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#235-351), [compaction](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#178-234), [lsm](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#1108-1161), [mmap](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs#1692-1729), [checkpoint](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#664-668)
- Schlüsselkomponenten: Size-Tiered LSM-Tree, HMAC-chained WAL, Block-Level encrypted SSTables.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ✅ | Keine Panics im Produktionscode identifiziert. |
| WAL-First | ✅ | Commit-Protokoll garantiert WAL-Append vor MemTable-Update. |
| MVCC-Isolation | ✅ | SnapshotRegistry und [get_at_seq](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#444-505) korrekt implementiert. |
| Korrektheit (GC) | ❌ | Tombstones werden in Teil-Compactions fälschlicherweise gelöscht. |

## 4. Findings

### FIND-STO-001: Phantom-Data via Aggressive Tombstone GC
- **Severity:** 🔴 Kritisch
- **Kategorie:** Data-Integrity
- **Datei:** [crates/memfuse-store/src/compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs)
- **Zeile(n):** L330
- **Beschreibung:** Der LSM-Tree nutzt eine Size-Tiered Compaction Strategy (STCS), bei der oft nur eine Teilmenge der SSTables gemergt wird. Der Code löscht Tombstones, sobald ihre Sequence Number kleiner als die kleinste aktive Snapshot-ID ist. In einem hierarchischen LSM-Tree darf ein Tombstone jedoch nur gelöscht werden, wenn sichergestellt ist, dass kein älterer Wert in einem *niedrigeren* (größeren) Tier existiert.
- **Impact:** Gelöschte Daten können nach einer Compaction plötzlich wieder auftauchen (Phantom-Daten), wenn der Tombstone gelöscht wird, bevor er die unterste Ebene des Baums erreicht hat.
- **Empfohlene Behebung:** Tombstones in STCS nur löschen, wenn (a) die Compaction alle SSTables umfasst (Full compaction) oder (b) das Ziel-Tier das nachweislich unterste ist.
- **Aufwand:** M

### FIND-STO-002: Tier-Backlog in Compaction Selection
- **Severity:** 🟡 Mittel
- **Kategorie:** Performance/Stability
- **Datei:** [crates/memfuse-store/src/compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs)
- **Zeile(n):** L214
- **Beschreibung:** [select_compaction_candidates](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#178-234) bricht nach dem ersten gefundenen Tier ab, das die Mindestanzahl an SSTables erreicht.
- **Impact:** Unter Last können sich in anderen Tiers SSTables ansammeln, ohne dass diese bereinigt werden, was zu hohen Read-Amplitudes führt.
- **Aufwand:** S

### FIND-STO-003: Starre CRC-Annahme bei Magic MFSX
- **Severity:** 🟡 Mittel
- **Kategorie:** Robustness
- **Datei:** [crates/memfuse-store/src/sstable.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs)
- **Zeile(n):** L589
- **Beschreibung:** Die Entscheidung, ob CRC32-Prüfsummen in den Datenblöcken vorhanden sind, wird allein am Magic-Header `MFSX` festgemacht.
- **Impact:** Falls zukünftige Formatänderungen das Magic beibehalten, aber CRC-Positionen verschieben, führt dies zu unvorhersehbaren Fehlern.
- **Empfohlene Behebung:** Versionierung im Trailer einführen.
- **Aufwand:** S

### FIND-STO-004: Fehlendes FSync bei WAL-UUID-Persistenz
- **Severity:** 🟢 Niedrig
- **Kategorie:** Durability
- **Datei:** [crates/memfuse-store/src/wal.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs)
- **Zeile(n):** L373
- **Beschreibung:** Die `.uuid`-Datei des WAL wird geschrieben, aber das Verzeichnis wird nicht ge-fsynced.
- **Impact:** Bei einem Crash unmittelbar nach Erstellung könnte die UUID verloren gehen, was den Zugriff auf den verschlüsselten WAL verhindert.
- **Aufwand:** S

## 5. Test-Gap-Analyse

| Funktion/Modul | Testabdeckung | Fehlende Szenarien |
|---|---|---|
| LSM Rollback | ✅ High | Rollback bei Disk-Full während WAL-Append |
| Compaction GC | 🔴 Kritisch | Wiederauftauchen von Keys nach Teil-Compaction |
| MMap Reader | 🔴 Zero | Aktuelle Implementierung ist ein Skeleton (WP-4.1) |

## 6. Empfehlungen (priorisiert)
1. **[Kritisch]** STCS-Tombstone-Logik fixen (Tombstone-Retention-Rule).
2. **[Mittel]** Compaction-Selektion optimieren (Fair-Selection über alle Tiers).
3. **[Niedrig]** WAL-UUID-Persistenz absichern (Directory FSync).
