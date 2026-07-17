# Sprint 2: Data-Integrity & ACID — Tombstone-GC, Snapshot-Isolation, Storage-Lifecycle

## Ziel
Alle Findings heilen, die **ACID-Invarianten**, **Datenintegrität** und **Persistenz-Korrektheit** betreffen. Dieser Sprint berührt die Persistenz- und Orchestrierungsschichten (`memfuse-store`, `memfuse-text`, `memfuse-db`, `memfuse-index`). Er setzt voraus, dass Sprint 1 abgeschlossen ist (insbesondere die Panic-Fixes).

> [!CAUTION]
> FIND-STO-001 (Phantom-Daten) und FIND-DB-003 (fehlende Snapshot-Isolation) betreffen die Datenintegrität direkt. Fehlerhafte Implementierung kann zu stillem Datenverlust führen.

## Betroffene Findings (10)

| ID | Crate | Severity | Kurzname |
|---|---|---|---|
| FIND-STO-001 | `memfuse-store` | 🔴 Kritisch | Phantom-Daten via aggressive Tombstone-GC |
| FIND-STO-002 | `memfuse-store` | 🟡 Mittel | Tier-Backlog in Compaction-Selektion |
| FIND-STO-003 | `memfuse-store` | 🟡 Mittel | Starre CRC-Annahme bei Magic MFSX |
| FIND-STO-004 | `memfuse-store` | 🟢 Niedrig | Fehlendes FSync bei WAL-UUID |
| FIND-TXT-001 | `memfuse-text` | 🔴 Kritisch | Dirty Reads im BM25-Suchpfad |
| FIND-TXT-002 | `memfuse-text` | 🔴 Kritisch | Quadratischer Tombstone-Bottleneck |
| FIND-TXT-003 | `memfuse-text` | 🟡 Mittel | Ineffiziente Posting-List Granularität |
| FIND-TXT-004 | `memfuse-text` | 🟡 Mittel | Fehlendes BM25-Statistik-Caching |
| FIND-DB-002 | `memfuse-db` | 🔴 Kritisch | Storage Leak bei `drop_collection` |
| FIND-DB-003 | `memfuse-db` | 🔴 Kritisch | Fehlende Snapshot-Isolation in Queries |
| FIND-DB-004 | `memfuse-db` | 🟡 Mittel | Ineffizienter HNSW-Repair Mechanismus |
| FIND-DB-005 | `memfuse-db` | 🟡 Mittel | Split-Brain bei 2PC-Kompensation |
| FIND-IND-002 | `memfuse-index` | 🟡 Mittel | Globale SQ8-Präzisionsverluste |

---

## Prompt / Implementierungsplan

> **Kontext**: Du bist der Architekt des Sovereign Core. Sprint 1 ist abgeschlossen — Panic-Safety ist wiederhergestellt. Dein Fokus liegt nun auf ACID-Compliance und Datenintegrität. Du arbeitest an `memfuse-store` (Persistenz), `memfuse-text` (Inverted Index), `memfuse-db` (Orchestrierung) und `memfuse-index` (Quantisierung). Die Invarianten §3 (Ressourcen-Endlichkeit), §4 (Determinismus) und §18 (Persistenzgesetz) sind maßgeblich.

### Phase 1: Tombstone-GC Korrektur (Datenintegrität)

#### Schritt 1.1 — STCS Tombstone-Retention-Rule (FIND-STO-001)
- **Datei**: [crates/memfuse-store/src/compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs), L330
- **Problem**: Tombstones werden gelöscht sobald `seq < min_active_snapshot`, ungeachtet ob ältere Werte in tieferen Tiers existieren.
- **Aktion**:
  1. Lies den aktuellen `compact_sstables()` Pfad vollständig.
  2. Füge eine **Tombstone-Retention-Bedingung** hinzu:
     ```rust
     // Ein Tombstone darf NUR gelöscht werden wenn:
     // (a) es eine Full-Compaction ist (alle SSTables aller Tiers betroffen), ODER
     // (b) das Ziel-Tier das nachweislich unterste ist (kein SSTable mit älteren SeqNos existiert außerhalb)
     let is_full_compaction = selected_tables.len() == all_tables.len();
     let retain_tombstone = !is_full_compaction && entry.is_tombstone();
     ```
  3. Tombstone bleibt erhalten wenn `retain_tombstone == true`.
- **Cross-Check (§22)**: Stelle sicher, dass `lsm.rs` den `compact()` Aufruf korrekt weiterleitet und kein zweiter Tombstone-Filter existiert.
- **Test**: 
  ```
  test_phantom_data_after_partial_compaction:
    1. Schreibe Key "A" mit SeqNo=1
    2. Lösche Key "A" (Tombstone SeqNo=2)
    3. Flush → 2 SSTables in verschiedenen Tiers
    4. Partial-Compaction (nur oberes Tier)
    5. Assert: get("A") == None (nicht der alte Wert!)
  ```

#### Schritt 1.2 — Fair-Selection über alle Tiers (FIND-STO-002)
- **Datei**: [crates/memfuse-store/src/compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs), L214
- **Aktion**: `select_compaction_candidates()` nach dem ersten Fund weiter iterieren lassen. Priorisierung nach Tier-Füllgrad (am vollsten zuerst).
- **Test**: Bestehende Compaction-Tests + neuer Test mit 3 Tiers die alle gleichzeitig die Schwelle überschreiten.

### Phase 2: Snapshot-Isolation (ACID-Compliance)

#### Schritt 2.1 — Text-Engine Snapshot-Guard (FIND-TXT-001)
- **Datei**: [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs), L384
- **Aktion**:
  1. `search_bm25()` Signatur erweitern um `snapshot: Option<&SnapshotGuard>`.
  2. Wenn snapshot vorhanden: `storage.scan_prefix_at(prefix, snapshot.seq_no())` nutzen statt `scan_prefix()`.
  3. Prüfe ob `memfuse-store` `scan_prefix_at(seq_no)` bereits exponiert — falls nicht, muss diese Methode in `lsm.rs` hinzugefügt werden.
- **Annahme**: `LsmStorage` hat eine `get_at_seq()` Methode (bestätigt in Audit). Die `scan_prefix_at()` Variante folgt dem gleichen Pattern.
- **Test**:
  ```
  test_text_search_isolation:
    1. Dokument D1 einfügen + committen
    2. Snapshot S1 nehmen
    3. Dokument D2 einfügen (uncommitted)
    4. search_bm25("term_in_D2", snapshot=S1)
    5. Assert: D2 ist NICHT in den Ergebnissen
  ```

#### Schritt 2.2 — DB-Layer Snapshot-Isolation (FIND-DB-003)
- **Datei**: [crates/memfuse-db/src/collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs)
- **Aktion**:
  1. In `search_with_filter()` vor der Suche einen `SnapshotGuard` vom LSM-Store anfordern.
  2. Diesen Guard an alle Lesepfade durchreichen: `hydrate_from_tuples()`, `storage.get()`.
  3. Auf bestehende `SnapshotRegistry` in `memfuse-core` aufbauen.
- **Cross-Crate-Analyse (§22)**:
  - `memfuse-core::snapshot::SnapshotRegistry` — bereits implementiert
  - `memfuse-store::lsm::get_at_seq()` — bereits implementiert
  - Fehlend: Durchreichung des Guards in der Collection-Orchestrierung
- **Test**:
  ```
  test_collection_search_snapshot_isolation:
    1. Collection erstellen, Doc1 inserieren + committen
    2. Snapshot Guard anfordern
    3. Doc2 inserieren (uncommitted)
    4. Search mit Snapshot → nur Doc1 gefunden
    5. Commit Doc2
    6. Neuer Search ohne Snapshot → Doc1 + Doc2
  ```

### Phase 3: Storage-Lifecycle & Resource-Cleanup

#### Schritt 3.1 — `drop_collection` Storage Cleanup (FIND-DB-002)
- **Datei**: [crates/memfuse-db/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs), L190ff und [crates/memfuse-db/src/collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs)
- **Aktion**:
  1. `Collection::cleanup()` Methode implementieren die `storage.delete_prefix(collection_prefix)` aufruft.
  2. In `drop_collection()` vor dem Entfernen aus der HashMap `cleanup()` aufrufen.
  3. Falls `delete_prefix()` im LSM nicht existiert: Implementierung über Tombstone-Range-Write.
- **Test**: `test_drop_collection_frees_storage` — Nach Drop darf `storage.scan_prefix(old_prefix)` keine Einträge mehr liefern.

#### Schritt 3.2 — WAL Directory FSync (FIND-STO-004)
- **Datei**: [crates/memfuse-store/src/wal.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs), L373
- **Aktion**: Nach Schreiben der `.uuid` Datei den Parent-Directory fsynchen:
  ```rust
  let dir = std::fs::File::open(path.parent().unwrap_or(Path::new(".")))?;
  dir.sync_all()?;
  ```
- **Test**: Schwer direkt zu testen; statische Code-Review als Bestätigung.

#### Schritt 3.3 — SSTable Format-Versionierung (FIND-STO-003)
- **Datei**: [crates/memfuse-store/src/sstable.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs), L589
- **Aktion**: Im Trailer ein `format_version: u16` Feld hinzufügen. CRC-Entscheidung basiert auf Version, nicht nur auf Magic.
- **Test**: Round-Trip-Test mit v1 und v2 Formaten.

### Phase 4: Performance-Fixes

#### Schritt 4.1 — Text-Engine Tombstone-Redesign (FIND-TXT-002)
- **Datei**: [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs), L277
- **Aktion**:
  1. Forward-Index einführen: `pd:{doc_id}:{term}` → Term-Presence.
  2. `resolve_tombstones()` liest zuerst `pd:{doc_id}:*` um die betroffenen Terms zu ermitteln, dann löscht gezielt nur diese Posting-List-Einträge.
  3. Komplexität sinkt von $O(T \times PL)$ auf $O(T \times D_{terms})$ wobei $D_{terms}$ die Anzahl der Terms pro Dokument ist.
- **Migration**: Neue Dokumente schreiben automatisch den Forward-Index mit. Bestehende Daten sind nicht betroffen (Tombstone-Resolution fällt gracefully zurück auf alten Scan).
- **Test**: Benchmark mit 10k Dokumenten und 1k Tombstones — muss in <1s statt bisher potentiell Minuten.

#### Schritt 4.2 — BM25 Statistik-Cache (FIND-TXT-004)
- **Datei**: [crates/memfuse-text/src/inverted.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs), L366
- **Aktion**: `AtomicU64` für `total_docs` und `avg_doc_len_x1000` (fixed-point). Update bei jedem Commit statt bei jeder Query.
- **Test**: `test_bm25_cached_stats_match_storage`.

#### Schritt 4.3 — HNSW-Repair Beschleunigung (FIND-DB-004)
- **Datei**: [crates/memfuse-db/src/collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs), L144ff
- **Aktion**: Statt k=1 Search pro Doc: Iteriere über `hnsw.doc_to_node` Map und prüfe Präsenz in O(1).
- **Test**: Bestehende Repair-Tests + Benchmark mit 100k Docs.

#### Schritt 4.4 — Per-Dimension SQ8 (FIND-IND-002)
- **Datei**: [crates/memfuse-index/src/quantize.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs)
- **Aktion**: `ScalarQuantizer` um `per_dim_min: Vec<f32>` und `per_dim_max: Vec<f32>` erweitern. Quantisierung pro Dimension statt global.
- **Annahme**: Dimension ist fix über die Lebensdauer einer Collection (bestätigt durch `memfuse-core::VectorDimension`).
- **Test**: Recall-Test: SQ8-PD muss bei stark variierenden Dimensionen >5% bessere Recall liefern als globales SQ8.

#### Schritt 4.5 — 2PC Recovery-Log (FIND-DB-005)
- **Datei**: [crates/memfuse-db/src/transaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/transaction.rs), L173
- **Aktion**: Vor dem Commit einen "Commit-Intent" mit Payload in einen dedizierten LSM-Namespace (`__tx_intent:{tx_id}`) schreiben. Nach erfolgreichem Commit löschen. Bei Startup: Alle offenen Intents replaying oder kompensieren.
- **Test**: `test_2pc_recovery_after_crash` — Simuliere Failure nach Intent-Write, vor Commit-Complete. Assert: Recovery findet und kompensiert den Intent.

---

## Verifikationsplan

### Automatisiert (Triple-Gate gemäß Artikel V)

```bash
# Gate I — Kompilierbarkeit
nix develop -c cargo check --all-targets --workspace

# Gate II — Stilgesetz
nix develop -c cargo clippy --all-targets -- -D warnings

# Gate III — Verhalten
nix develop -c cargo test --workspace

# Sprint-spezifische Tests:
nix develop -c cargo test -p memfuse-store -- compaction
nix develop -c cargo test -p memfuse-store -- tombstone
nix develop -c cargo test -p memfuse-text -- search_bm25
nix develop -c cargo test -p memfuse-text -- tombstone
nix develop -c cargo test -p memfuse-db -- drop_collection
nix develop -c cargo test -p memfuse-db -- snapshot
nix develop -c cargo test -p memfuse-db -- repair
nix develop -c cargo test -p memfuse-db -- transaction
nix develop -c cargo test -p memfuse-index -- quantize
```

### Integrationstests
```bash
# Falls vorhanden:
nix develop -c cargo test --test '*' --workspace
```

### Manuelle Verifikation
- `just debt-audit` muss PASSED bleiben.
- `just dag-check` muss PASSED bleiben (keine neuen DAG-Verletzungen).
- Visueller Diff-Review: Keine neuen `unwrap()` oder `panic!` eingeführt.
