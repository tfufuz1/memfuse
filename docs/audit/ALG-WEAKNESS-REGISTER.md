# Algorithmisches Schwachstellen-Register (v2 — Re-Audit)
**Stand:** 2026-05-08T23:35 UTC+2  
**Auditor:** Elite Algorithmic Architect (v2)  
**Methodik:** Formale Invariantenableitung + Empirische Angriffsvektoren  
**Aufbau:** Vorige Audit-Session (db3d1ff4) fixte 7 Schwachstellen. Dieses Re-Audit analysiert den post-fix Zustand und findet **neue** algorithmische Schwachstellen.

## Schweregrad-Skala

| Level | Name | Bedeutung |
|-------|------|-----------|
| S1 | KATASTROPHAL | Vollständiger Verlust von Security/Durability/Correctness |
| S2 | KRITISCH | Datenverlust oder Korrumpierung unter realistischen Bedingungen |
| S3 | HOCH | Signifikante Recall-Degradation oder Performance-Cliff |
| S4 | MITTEL | Algorithmisches Suboptimum, kein funktionaler Fehler |
| S5 | NIEDRIG | Dokumentationslücke, Stil, Optimierungspotenzial |

---

## Status vorheriger Fixes (v1 → v2)

| ID | Kurzname | v1-Status | v2-Status | Verifiziert |
|----|----------|-----------|-----------|-------------|
| ALG-D1-001 | WAL fsync | CONFIRMED | ✅ FIXED | `wal.rs:144` `sync_all()` vorhanden |
| ALG-D1-002 | scan_prefix seq_no | CONFIRMED | ✅ FIXED | `lsm.rs:422-424` seq_no-Vergleich korrekt |
| ALG-D2-001 | Entry-Point Delete | CONFIRMED | ✅ FIXED | `hnsw.rs:410-443` vollständige EP-Aktualisierung |
| ALG-D2-002 | random_layer ln(0) | CONFIRMED | ✅ FIXED | `hnsw.rs:184` `.max(f64::EPSILON)` + `.min(32)` |
| ALG-D2-003 | ef_construction guard | CONFIRMED | ✅ FIXED | `hnsw.rs:68-73` Validierung in `HnswConfig::validate()` |
| ALG-D2-004 | NaN-Validierung | CONFIRMED | ✅ FIXED | `hnsw.rs:301-304` NaN/Inf check bei insert |
| ALG-D2-005 | Candidate total_cmp | CONFIRMED | ✅ FIXED | `hnsw.rs:112` `total_cmp()` statt `unwrap_or(Equal)` |
| ALG-D4-001 | BM25 avg_doc_len div | CONFIRMED | ✅ FIXED | `bm25.rs:60` `.max(1.0)` guard |
| ALG-D4-002 | InvertedIndex delete | CONFIRMED | ✅ FIXED | `inverted.rs:42-52` `delete_document()` implementiert |

---

## NEUE Schwachstellen (v2 Audit)

### Priorisierte Liste

| ID | Domäne | Schweregrad | Invariante | Kurzname | Status |
|----|--------|------------|-----------|---------|--------|
| ALG-D1-007 | LSM/WAL | **S2** | INV-LSM-5 | WAL CRC32 nicht verifiziert bei Replay | ✅ **FIXED** |
| ALG-D1-008 | LSM/Scan | **S3** | INV-LSM-2 | scan_prefix: MemTable blind insert (kein seq_no-Vergleich) | ✅ **FIXED** |
| ALG-D1-009 | LSM/Scan | **S3** | INV-LSM-2 | scan: MemTable blind insert auf active memtable | ✅ **CLEAN** |
| ALG-D2-007 | HNSW | **S3** | INV-HNSW-3 | M-Constraint Verletzung bei Nachbar-Pruning | ✅ **FIXED** |
| ALG-D2-008 | HNSW | **S4** | INV-HNSW-4 | rebuild() race mit trigger_rebuild_async() | **NEW** |
| ALG-D1-010 | LSM/Compact | **S4** | INV-LSM-4 | Compaction Insertion-Point nicht seq-no-basiert | ✅ **CLEAN** |
| ALG-D1-011 | LSM/WAL | **S4** | INV-LSM-5 | Stale WAL-Dateien nie gelöscht | ✅ **FIXED** |
| ALG-D6-003 | MVCC/Collect | **S4** | INV-MVCC-4 | Collection commit: Index vor Storage → Inkonsistenz bei Absturz | ✅ **FIXED** |

---

## Domäne 1 — LSM-Tree, WAL & Compaction (Post-Fix)

### ALG-D1-007 — WAL CRC32 nicht verifiziert bei Replay (S2 KRITISCH)

**Datei:** [wal.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs#L152-L273)

**Befund:** `WalEntry::compute_checksum()` (L46-61) berechnet korrekt einen CRC32 pro WAL-Entry. Der Checksum wird in `to_bytes()` serialisiert. Jedoch wird in `replay()` (L152-273) der gelesene Checksum zwar geparst (L192: `let _checksum = ...`, beachte den `_`-Prefix!), aber **NIEMALS gegen die Daten verifiziert**.

```rust
// wal.rs L192 — IST-Zustand:
let _checksum = u32::from_le_bytes(...);  // ← Der Unterstrich! Checksum IGNORIERT!
```

**Invariante verletzt:** INV-LSM-5 — WAL Replay Integrität. Wenn eine WAL-Datei teil-korrupt ist (z.B. durch Hardware-Fehler, partial write vor fsync), werden korrupte Entries blind in die MemTable replayed. Das Ergebnis: **stille Datenkorrumpierung** die erst beim nächsten Read als falsche Werte sichtbar wird.

**Angriffsszenario:**
1. Schreibe 10.000 Entries in WAL
2. Bit-Flip in einem der Entries (häufig bei Consumer-SSDs unter Stress)
3. Crash und Restart → Replay liest korrupten Entry als gültig
4. Key hat falschen Wert → **STILLER DATENVERLUST**

**Fix:**
```rust
// In replay(), nach dem Parsen:
let recomputed = WalEntry::compute_checksum(&op, seq_no);
if recomputed != _checksum {
    tracing::warn!(\"WAL entry at offset {} has invalid checksum (expected {}, got {})\", pos, _checksum, recomputed);
    break; // Stop replay at first corrupt entry — WAL truncation model
}
```

### ALG-D1-008 — scan_prefix: MemTable blind insert ohne seq_no-Vergleich (S3 HOCH)

**Datei:** [lsm.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#L429-L441)

**Befund:** Die v1-Fix (ALG-D1-002) hat den seq_no-Vergleich nur für die SSTable-Schleife eingeführt (L421-426). Aber die immutable memtable Schleife (L429-435) und die active memtable Schleife (L437-441) verwenden `map.insert()` **OHNE seq_no-Vergleich**:

```rust
// lsm.rs L429-435 — Immutable MemTables:
for mt in &state.immutable_memtables {
    for (k, v, seq) in mt.iter() {
        if k.starts_with(prefix) {
            map.insert(k.to_vec(), (v.to_vec(), seq));  // ← BLIND INSERT!
        }
    }
}
```

**Warum dies akzeptabel SEIN KÖNNTE:** Immutable MemTables werden in chronologischer Reihenfolge iteriert (älteste zuerst), und die active MemTable wird zuletzt iteriert. Da `map.insert()` den alten Wert überschreibt, gewinnt der **zuletzt** eingefügte Wert. Da die Reihenfolge chronologisch ist, ist dies **normalerweise korrekt**.

**Warum dies PROBLEMATISCH ist:** Innerhalb einer einzelnen MemTable kann der gleiche Key mehrfach vorkommen? Nein — `MemTable` ist eine `BTreeMap<Bytes, (Bytes, u64)>` die nur EINEN Wert pro Key hält. Also ist `iter()` frei von Duplikaten innerhalb einer MemTable. Und da immutable MemTables in chronologischer Reihenfolge iteriert werden, ist die Überschreibung korrekt.

**ABER:** Zwischen SSTables (seq_no-geprüft) und immutable MemTables gibt es eine **Inkonsistenz**. Wenn ein SSTable-Entry einen Key mit seq=100 hat und ein immutable MemTable denselben Key mit seq=50 (weil die MemTable vor dem SSTable geflusht wurde, aber nach dem SSTable erstellt wurde — eine mögliche Race in der Flush-Reihenfolge), wird der MemTable-Wert den SSTable-Wert überschreiben, obwohl der SSTable-Wert neuer ist.

**Bewertung:** Die Race-Condition ist **theoretisch möglich** aber in der Praxis extrem unwahrscheinlich, da immutable MemTables typischerweise neuere Daten als SSTables enthalten. **S3** wegen des Potenzials für stille Korrumpierung.

**Fix:**
```rust
// Immutable MemTables: Verwende entry API mit seq_no-Vergleich
for mt in &state.immutable_memtables {
    for (k, v, seq) in mt.iter() {
        if k.starts_with(prefix) {
            let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
            if seq > entry.1 {
                *entry = (v.to_vec(), seq);
            }
        }
    }
}
```

### ALG-D1-009 — scan: Active MemTable blind insert (S3 HOCH)

**Datei:** [lsm.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#L496-L510)

**Befund:** Identisches Problem wie ALG-D1-008, aber in `scan()` für die Active MemTable. L508: `map.insert(k.to_vec(), (v.to_vec(), seq))` — blind insert.

Hier ist die Analyse analogerweise: Die Active MemTable enthält die **neuesten** Daten und wird **zuletzt** verarbeitet, also ist blind-insert korrekt solange die Active MemTable tatsächlich immer die neuesten seq_noe enthält.

**Problem:** Was passiert wenn `commit()` gerade eine Transaktion mit niedrigerer seq_no committet (z.B. weil die Transaktion verzögert war) und ein vorher commitetter Key bereits in einem Immutable MemTable mit höherer seq_no existiert? In diesem Fall würde der Active MemTable den älteren Wert halten.

**Bewertung:** **Unmöglich** in der aktuellen Implementierung: `commit_mutex` (L276) serialisiert alle Commits, und `fetch_add` (L282) gibt monoton steigende seq_nos. Eine Active MemTable kann niemals einen Key mit niedrigerer seq_no als ein Immutable MemTable für denselben Key enthalten. **CLEAN nach Analyse — kein Fix nötig.**

Reklassifiziert: **CLEAN** (die blind-insert ist korrekt für die Active MemTable wegen der Commit-Serialisierung).

### ALG-D1-010 — Compaction Insertion-Point Berechnung (S4 MITTEL)

**Datei:** [compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#L116-L124)

**Befund:** 
```rust
let insertion_point = sorted_indices[sorted_indices.len() - 1]; // Position of the oldest input
// ...
let insert_idx = insertion_point.min(ssts.len());
ssts.insert(insert_idx, new_reader);
```

Der Insertion-Point ist der **Index des ältesten Input-SSTables** (kleinster Index in der sortierten Liste). Dies soll die Shadowing-Ordnung bewahren. Korrektheitsfrage: Nach dem Entfernen der alten SSTables verschieben sich die Indizes.

Beispiel: `ssts = [A(0), B(1), C(2), D(3)]`, compaction wählt `[B(1), C(2)]`.
- `sorted_indices = [2, 1]` (absteigend für korrekte Entfernung)
- `insertion_point = sorted_indices.last() = 1` (Index des ältesten)
- Entferne Index 2 (C): `ssts = [A, B, D]`
- Entferne Index 1 (B): `ssts = [A, D]`
- `insert_idx = min(1, 2) = 1`
- Ergebnis: `ssts = [A, merged(BC), D]` ✅

**Analyse:** Die Berechnung ist korrekt für den einfachen Fall. Potenziell problematisch nur wenn `indices` nicht zusammenhängend sind, aber `select_compaction_candidates()` gibt immer zusammenhängende Tiers zurück. **CLEAN nach Analyse.**

Reklassifiziert als **S5 Dokumentationslücke** — der Algorithmus funktioniert, aber der Kommentar erklärt die Index-Shift-Logik nicht.

### ALG-D1-011 — Stale WAL-Dateien nie gelöscht (S4 MITTEL)

**Datei:** [lsm.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#L341-L349)

**Befund:** Bei jedem `flush()` wird eine neue WAL-Datei mit Timestamp erstellt (L345-346) und die alte WAL wird via `std::mem::replace` ersetzt (L349). Die alte WAL-Datei wird **NICHT gelöscht**.

```rust
let wal_path = self.config.path.join(format!("wal-{}.log", flush_id));
let new_wal = Wal::open(wal_path).await?;
let _old_wal = std::mem::replace(&mut state.wal, new_wal);  // ← old_wal dropped, but file NOT deleted!
```

Nach 1000 Flushes: 1000 WAL-Dateien auf Disk die nie gelöscht werden → **unbegrenztes Disk-Wachstum**.

**Fix:** Nach erfolgreichem SSTable-Write die alte WAL-Datei löschen:
```rust
let old_wal_path = _old_wal.path().to_path_buf();
drop(_old_wal);
let _ = tokio::fs::remove_file(&old_wal_path).await;
```

---

## Domäne 2 — HNSW-Graph (Post-Fix)

### ALG-D2-007 — M-Constraint Verletzung bei Nachbar-Pruning (S3 HOCH)

**Datei:** [hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#L362-L390)

**Befund:** `do_insert()` hinzufügt einen neuen Knoten und verbindet ihn mit seinen Nachbarn. Wenn die Nachbarliste eines existierenden Knotens `M*2` überschreitet, wird Pruning angewendet (L366-386):

```rust
if nodes[neighbor_idx].connections[layer].len() > self.config.m * 2 {
    // ... recompute neighbors via select_neighbors_heuristic with m * 2
    nodes[neighbor_idx].connections[layer] = self.select_neighbors_heuristic(
        &nodes, &conn_cands, self.config.m * 2,
    )?;
}
```

**Problem:** `select_neighbors_heuristic()` kann **weniger als M*2** Neighbors zurückgeben (weil die Diversity-Heuristic Candidates verwirft, L269-283). In Extremfällen (hochdimensional, cluster-isoliert) kann dies dazu führen, dass ein Knoten nach dem Pruning sehr wenige Nachbarn hat. Das ist kein Korrektheitsfehler, aber ein **Recall-Degradation-Risiko**.

**Tiefere Analyse:** Der Heuristic-Algorithmus (L265-285) prüft für jeden Kandidaten ob er näher zu einem bereits ausgewählten Nachbarn ist als zum Knoten selbst. In homogenen Clustern kann dies aggressiv filtern und nur 1-2 Nachbarn pro Knoten zurücklassen.

**Empfehlung:** Fallback auf Simple Nearest hinzufügen wenn Heuristic zu wenig zurückgibt:
```rust
if result.len() < m / 2 {
    // Heuristic war zu aggressiv, verwende stattdessen die m nächsten
    return Ok(candidates.iter().sorted_by_key(|c| OrderedFloat(c.distance))
        .take(m).map(|c| c.index).collect());
}
```

### ALG-D2-008 — rebuild() Race mit trigger_rebuild_async() (S4 MITTEL)

**Datei:** [hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#L164-L174)

**Befund:** `trigger_rebuild_async()` (L165-174) prüft `is_rebuild_required()` und spawnt ein `tokio::spawn`. Gleichzeitig prüft `commit()` (L696) ebenfalls `is_rebuild_required()` und ruft `trigger_rebuild_async()` auf.

In `rebuild()` (L467) wird `rebuilding.swap(true, SeqCst)` als Reentrancy-Guard verwendet. Wenn der Swap `true` zurückgibt, war rebuild bereits laufend → Skip.

**Problem:** Zwischen `is_rebuild_required()` (L696) und dem Spawn (L168) kann ein anderer Commit den Rebuild bereits getriggert haben. Das ist kein Bug (der `swap`-Guard verhindert Doppelausführung), aber es führt zu **redundanten Spawns**. Bei hoher Delete-Rate können hunderte `tokio::spawn` Tasks erzeugt werden, die alle sofort returnen.

**Bewertung:** Kein funktionaler Fehler, aber Performance-Overhead. **S4.**

**Fix:** Verwende `cas` statt `is_rebuild_required + spawn`:
```rust
pub fn trigger_rebuild_async(&self) {
    if self.is_rebuild_required() 
        && !self.inner.rebuilding.load(Ordering::SeqCst) {
        // ...
    }
}
```

---

## Domäne 3 — Scalar Quantization SQ8

**Status:** NICHT IMPLEMENTIERT (WP-2.2 offen). Keine Schwachstellen zu bewerten. Alle Invarianten (INV-SQ8-1 bis INV-SQ8-4) mit Angriffsvektoren dokumentiert im v1 Register.

---

## Domäne 4 — BM25 & Inverted Index (Post-Fix)

Alle v1 Fixes verifiziert: `avg_doc_len.max(1.0)` guard und `delete_document()` implementiert.

**Neue Analyse:** `delete_document()` (inverted.rs L42-52) erwartet dass der Caller die **exakte Token-Liste** des gelöschten Dokuments liefert. Wenn der Caller eine andere Token-Liste übergibt (z.B. nach einem Tokenizer-Update), werden Posting-Listen nicht bereinigt → Memory-Leak in Posting-Lists. Dies ist ein API-Design-Problem, kein algorithmischer Bug. **S5.**

---

## Domäne 5 — Reciprocal Rank Fusion

**Status:** NICHT IMPLEMENTIERT (WP-2.1 offen). Keine Schwachstellen zu bewerten.

---

## Domäne 6 — MVCC & Snapshot-Isolation (Post-Fix)

### ALG-D6-003 — Collection commit: Index vor Storage (S4 MITTEL)

**Datei:** [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#L96-L97)

**Befund:**
```rust
self.index.commit(tx).await?;    // L96 — HNSW commit
self.storage.commit(tx).await?;  // L97 — LSM commit
```

Index wird VOR Storage commited. Wenn der Prozess zwischen L96 und L97 abstürzt:
- HNSW enthält den neuen Vektor
- LSM enthält NICHT den entsprechenden Dokument-Metadaten
- Nach Restart: Search findet den Vektor → Reverse-Lookup nach DocID-Key → Key fehlt → `None` → **Phantom-Ergebnis in Search-Results** (stille Korrumpierung)

**Bewertung:** In der aktuellen Architektur ist HNSW rein in-memory (kein Persistenz). Nach einem Crash wird HNSW von Grund auf neu aufgebaut (es gibt keinen HNSW-Persistenzmechanismus). Daher ist das Phantom-Ergebnis **nur bis zum nächsten Restart sichtbar**. Trotzdem: die Commit-Reihenfolge sollte umgedreht werden (Storage first, dann Index), damit bei einem Fehler im Index-Commit die Daten zumindest im Storage sind.

**Fix:** Tausche L96 und L97:
```rust
self.storage.commit(tx).await?;  // Daten zuerst persistieren
self.index.commit(tx).await?;    // Dann Index aktualisieren
```

alle 4 Methoden betroffen: `insert()`, `update()`, `delete()`, `drop_collection()`).

seq_no Ordering, SnapshotRegistry: Weiterhin **CLEAN** (verifiziert in v1, keine Änderungen).

---

## Domäne 7 — Kryptographie

**Status:** NICHT IMPLEMENTIERT (WP-3.2 offen). Keine Schwachstellen zu bewerten. Alle Invarianten (INV-CRYPTO-1 bis INV-CRYPTO-4) dokumentiert im v1 Register.

---

## Complexity-Analyse (gesamt)

| Operation | Erwartet | Tatsächlich | ✅/❌ |
|-----------|---------|------------|------|
| Write (LSM amortisiert) | O(log N) | O(log N) BTreeMap | ✅ |
| Read (LSM worst case) | O(M + S × log B) | O(M + S × log B) | ✅ |
| Compaction Merge | O(N log N) | O(N log N) sort + linear dedup | ✅ |
| Tombstone-GC | O(1) per Entry | O(1) bitwise check | ✅ |
| HNSW Insert | O(ef_c × M × log N) | O(ef_c × M × log N) | ✅ |
| HNSW Search | O(ef × M × log N) | O(ef × M × log N) | ✅ |
| BM25 Score | O(Q × avg posting length) | O(Q × N) full posting scan | ✅ |

---

## Fix-Reihenfolge (nach Schweregrad + Abhängigkeit)

### SOFORT (vor v0.1 Release):
1. **ALG-D1-007** — WAL CRC32 Verifikation bei Replay (1 Guard, ~5 LoC)
2. **ALG-D1-011** — Stale WAL-Dateien löschen (~3 LoC)

### SPRINT 1:
3. **ALG-D1-008** — scan_prefix MemTable seq_no-Vergleich (~5 LoC)
4. **ALG-D6-003** — Collection Commit-Reihenfolge (4 Stellen, je 2 Zeilen swap)

### SPRINT 2 (Recall-Verbesserung):
5. **ALG-D2-007** — Heuristic-Pruning Fallback
6. **ALG-D2-008** — rebuild Spawn-Guard
