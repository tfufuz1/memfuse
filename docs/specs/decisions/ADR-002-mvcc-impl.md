# ADR-002 — MVCC Implementation in memfuse-core

## Status
- Typ: ARCH
- Status: ACCEPTED
- Agent: AGENT:01
- Datum: 2026-05-11

## Kontext
MemFuse benötigt eine Multi-Version Concurrency Control (MVCC), um konsistente Read-Snapshots zu ermöglichen, während gleichzeitig Schreibvorgänge und Hintergrund-Garbage-Collection (Compaction) stattfinden.

## Entscheidung
Wir implementieren eine zentrale `SnapshotRegistry` in `memfuse-core`.

### 1. Sequence Numbers
Jeder Schreibvorgang erhält eine monoton steigende Sequence Number (u64).
Tombstones werden durch das Setzen des höchstwertigen Bits (Bit 63, `TOMBSTONE_BIT`) in der Sequence Number markiert.

### 2. Snapshot Registry
Die `SnapshotRegistry` verwaltet eine Map (`BTreeMap`) von aktiven Sequence Numbers, die von Read-Snapshots gehalten werden.
- `register(seq_no)`: Erhöht den Reference-Count für eine Sequence Number und gibt einen `SnapshotGuard` (RAII) zurück.
- `min_active_seqno()`: Gibt die kleinste aktuell registrierte Sequence Number zurück.

### 3. LSM-Integration (Garbage Collection)
Die LSM-Compaction-Engine fragt `min_active_seqno()` ab.
- Alle Einträge (inkl. Tombstones) mit einer Sequence Number **kleiner** als `min_active_seqno()` können sicher zusammengefasst oder gelöscht werden, da kein aktiver Read-Snapshot mehr darauf zugreift.
- Wenn keine Snapshots aktiv sind, liefert die Registry `u64::MAX`, was die vollständige Bereinigung ermöglicht.

### 4. Lock-Strategie
Die Registry verwendet einen `parking_lot::Mutex` für die Map-Operationen. Da Snapshots typischerweise am Anfang eines Requests erstellt und am Ende freigegeben werden, ist die Contention gering. Die `min_active_seqno` wird in einem `AtomicU64` gecached, um lock-freie Abfragen während der Compaction zu ermöglichen.

## Konsequenzen
- **Positiv:** Konsistente Reads ohne Blockierung von Writern.
- **Positiv:** Effiziente Garbage Collection durch präzises Tracking der ältesten benötigten Version.
- **Negativ:** Lang laufende Snapshots können die Garbage Collection blockieren und zu Speicherplatz-Wachstum führen (Orphan-Snapshot Problem).

// ANCHOR:DONE:ADR-002 — MVCC Implementierung dokumentiert
// AGENT:01 DATE:2026-05-11 STATUS:DONE
