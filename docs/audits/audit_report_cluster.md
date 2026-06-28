# Forensischer Audit-Bericht: memfuse-cluster

## 1. Executive Summary
- Gesamtbewertung: 🔴 Kritisch (Vollständiger Funktionsausfall im Cluster-Modus)
- Anzahl Findings: 3 Kritisch (Architektur), 1 Mittel
- Gesamteindruck: Die Integration von `openraft` ist oberflächlich korrekt, vernachlässigt aber die Kern-Invarianten von [memfuse](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs#983-1009). Das System leidet unter "Index-Blindheit" auf Follower-Knoten und verliert bei jedem Neustart den Raft-Log.

## 2. Crate-Steckbrief
- LOC: ~1.200
- Module: [storage](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#936-949), `network`, [node](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs#857-863)
- Schlüsselkomponenten: Raft Log Storage, State Machine (LSM-backed), HTTP Network Factory.

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Persistenz (§18) | ❌ | Raft-Log ist ein reiner In-Memory `BTreeMap`. |
| Index-Konsistenz | ❌ | [apply](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs#338-388) schreibt nur in LSM, ignoriert HNSW/Text-Engines. |
| Determinismus | ✅ | Raft-Protokoll garantiert deterministische Zustandsübergänge. |
| Zero-Panic | ✅ | Keine Panics im Kommunikationspfad. |

## 4. Findings

### FIND-CLU-001: Index-Blindheit auf Follower-Knoten
- **Severity:** 🔴 Kritisch (Architektur-Fehler)
- **Kategorie:** Korrektheit / Verteilte Systeme
- **Datei:** [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs)
- **Zeile(n):** L345-387
- **Beschreibung:** Die [apply](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs#338-388) Methode des Raft-State-Machine schreibt replizierte Dokumente direkt in den [LsmStorage](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#102-119), ohne die Orchestrierungsschicht (`memfuse-db`) zu nutzen.
- **Impact:** Follower-Knoten aktualisieren ihren HNSW-Index und Inverted-Index nicht. Eine Suchanfrage an einen Follower liefert keine Ergebnisse, obwohl die Daten im LSM-Store vorhanden sind. Das System ist als verteilte Vektor-DB funktionsunfähig.
- **Empfohlene Behebung:** [apply](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs#338-388) muss über `memfuse_db::Collection` operieren, um alle Engines (Vector, Text) synchron zu halten.
- **Aufwand:** H

### FIND-CLU-002: Ephemerer Raft-Log (Persistenz-Bruch)
- **Severity:** 🔴 Kritisch (Vorschriften-Verstoß)
- **Kategorie:** Verlässlichkeit
- **Datei:** [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs)
- **Zeile(n):** L106
- **Beschreibung:** Der Raft-Log ([log](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs#163-180)) wird in einer `SyncRwLock<BTreeMap>` gehalten und nicht in den LSM-Store persistiert.
- **Impact:** Bei einem Neustart verliert der Knoten seinen gesamten Log. Er ist vollständig auf Snapshots angewiesen. Bei einem gleichzeitigen Ausfall des Quorums droht totaler Datenverlust von noch nicht kompaktierten Log-Einträgen. Verletzt §18 (Source of Truth).
- **Empfohlene Behebung:** Persistierung des Logs in einem dedizierten LSM-Namespace (`__raft_log:..`).
- **Aufwand:** H

### FIND-CLU-003: Inkonsistente Snapshots (ACID-Bruch)
- **Severity:** 🔴 Kritisch (Architektur-Fehler)
- **Kategorie:** Korrektheit
- **Datei:** [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs)
- **Zeile(n):** L279
- **Beschreibung:** [build_snapshot](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs#272-317) führt einen globalen Scan des LSM-Stores durch, ohne einen konsistenten Snapshot (SeqNo-bezogen) anzufordern.
- **Impact:** Der Raft-Snapshot kann inkonsistente Zustände enthalten, wenn während des Scans Schreibvorgänge stattfinden.
- **Empfohlene Behebung:** Nutzung des `storage.snapshot()` Systems zur Erstellung eines konsistenten Point-in-Time Abbilds.
- **Aufwand:** M

## 5. Empfehlungen (priorisiert)
1. **[Total-Refactor]** Integration der Orchestrierungsschicht in den Raft-Apply-Pfad.
2. **[Dringend]** Persistierung des Raft-Logs im LSM-Store.
3. **[Wichtig]** Snapshot-Isolation sicherstellen.
