# Technischer Audit-Report: `memfuse-core` & `memfuse-store`

**System Version:** MemFuse Core Kernel 0.1.0 (MSRV 1.89)
**Datum:** 28. August 2026
**Auditor:** Staff-Level Rust Systems Engineer & Datenbank-Architekt
**Fokus-Crates:** `crates/memfuse-core`, `crates/memfuse-store`

---

## Executive Summary

Im Rahmen des tiefgehenden System-Audits der Persistenz- und Kernschichten (`memfuse-core` und `memfuse-store`) wurden die Kernkomponenten MVCC-Snapshot-Isolation, LSM-Tree State Machine, WAL-Integrität sowie nebenläufige Lock-Hierarchien auf Architekturebene geprüft.

Die Codebasis weist eine herausragende Sicherheitskultur auf (strikte `#![forbid(unsafe_code)]`-Compliance, unaligned SIMD-Sicherungen, HMAC-Chaining). Dennoch wurden 8 kritische bis mittlere Architektur- und Nebenläufigkeitsrisiken identifiziert, die in Grenzfällen zu Dateninkonsistenzen, Memory-Budget-Lecks, Stale Reads oder Deadlocks führen können.

---

## Technical Audit Findings

---

### 1. MVCC & Snapshot-Isolation (TxId-Disziplin)

#### **[KRITISCH] - Invalidation der Sequence-Monotonie durch ungefiltertes `TOMBSTONE_BIT` in `rollback_to_tx`**
* **Pfad/Modul:** `crates/memfuse-store/src/lsm.rs` (`LsmStorage::rollback_to_tx`)
* **Mechanismus:**
  Bei einem Rollback auf eine bestimmte Transaktion (`rollback_to_tx(target_tx)`) iteriert die Methode über verbleibende SSTables und WAL-Einträge, um den höchsten existierenden Sequenzwert (`max_seq`) zu ermitteln:
  ```rust
  for sst in sstables_lock.iter() {
      max_seq = max_seq.max(sst.metadata().max_seq);
  }
  // ... WAL replay ...
  self.next_seq_no.store(max_seq + 1, Ordering::SeqCst);
  ```
  Sollte die höchste verbleibende Transaktion in einer SSTable oder einem WAL-Eintrag ein Löschbefehl sein, ist in dessen Sequenznummer das `TOMBSTONE_BIT` (`1 << 63`) gesetzt. Da `max_seq` ohne Bitmaskierung (`& !TOMBSTONE_BIT`) berechnet wird, übernimmt `max_seq` den Wert `(seq | (1 << 63))`.
  Dies führt dazu, dass `self.next_seq_no` auf einen astronomisch hohen Wert (`> 9_223_372_036_854_775_808`) gesetzt wird. Jede daraufhin eingefügte Transaktion bekommt fortan automatisch das `TOMBSTONE_BIT` eingeprägt. Alle nach dem Rollback durchgeführten Einfügeoperationen (`put`) werden dadurch im System als Tombstones behandelt – ein vollständiger und irreversibler Datenverlust für nachfolgende Schreibvorgänge.
* **Verletzte Invariante:** Sequence-Number Monotonie (ADR-016 / `domain.rs`): `next_seq_no` repräsentiert eine fortlaufende 63-Bit Sequenznummer. Bit 63 darf ausschließlich bei der Speicherung in MemTable/SSTable als Tombstone-Flag verwendet werden und niemals in den Zähler `next_seq_no` fließen.
* **Lösungsvorschlag:**
  Maskiere `max_seq` strikt mit `!TOMBSTONE_BIT` vor dem Vergleich und der Speicherung:
  ```rust
  <<<<<<< SEARCH
          let mut max_seq = 0;
          for sst in sstables_lock.iter() {
              max_seq = max_seq.max(sst.metadata().max_seq);
          }
  =======
          let mut max_seq = 0;
          for sst in sstables_lock.iter() {
              let sst_seq = sst.metadata().max_seq & !TOMBSTONE_BIT;
              max_seq = max_seq.max(sst_seq);
          }
  >>>>>>> REPLACE
  ```
  Ebenso muss das WAL-Replay während des Rollbacks maskiert ausgewertet werden:
  ```rust
  <<<<<<< SEARCH
          for (seq, entry, _offset) in entries {
              if seq > max_seq {
                  max_seq = seq;
              }
  =======
          for (seq, entry, _offset) in entries {
              let clean_seq = seq & !TOMBSTONE_BIT;
              if clean_seq > max_seq {
                  max_seq = clean_seq;
              }
  >>>>>>> REPLACE
  ```

---

#### **[HOCH] - Read-Visibility Window in `LsmStorage::flush()` ermöglicht Stale Reads bei parallelem `get_at_seq`**
* **Pfad/Modul:** `crates/memfuse-store/src/lsm.rs` (`LsmStorage::flush`)
* **Mechanismus:**
  1. Während `flush()` wird die aktive MemTable in `immutable_memtables` geschoben und auf Festplatte geschrieben.
  2. Nach der SSTable-Erstellung holt `flush()` die Schreibsperre auf `sstables` und fügt die neue SSTable ein (`sstables.push(Arc::new(reader))`).
  3. Erst **nach** dem Einfügen in `sstables` wird `last_committed_tx` via Atomic-Compare-Exchange aktualisiert:
     ```rust
     sstables.push(Arc::new(reader));

     if sst_max_tx < TxId::INTERNAL_BASE {
         let mut current = self.last_committed_tx.load(Ordering::Acquire);
         while sst_max_tx > current {
             // ... Compare Exchange ...
         }
     }
     drop(sstables);
     drop(state);
     ```
  4. Trifft ein paralleler Reader in `get_at_seq()` exakt im Fenster zwischen `sstables.push()` und der `last_committed_tx`-Aktualisierung ein, liest er den alten Stand von `last_committed_tx`.
  5. Der Reader prüft die SSTable, scheitert jedoch am Filter `tx <= snapshot_tx` (`snapshot_tx` ist noch veraltet), obwohl die Daten aus der geflushten MemTable nicht mehr in `immutable_memtables` vorhanden sind. Der Reader erhält fälschlicherweise `Ok(None)` (Stale Read / Data Invisibility).
* **Verletzte Invariante:** MVCC Snapshot-Isolation (ADR-024): Wenn eine Transaktion committed ist und deren Daten in SSTables überführt werden, müssen diese nahtlos für alle nachfolgenden Reads sichtbar bleiben.
* **Lösungsvorschlag:**
  Aktualisiere `last_committed_tx` **vor** dem Freigeben der Schreibsperre bzw. vor dem Entfernen aus `immutable_memtables`:
  ```rust
  <<<<<<< SEARCH
          let sst_max_tx = reader.metadata().max_tx_id;
          sstables.push(Arc::new(reader));

          if sst_max_tx < TxId::INTERNAL_BASE {
              let mut current = self.last_committed_tx.load(Ordering::Acquire);
              while sst_max_tx > current {
                  match self.last_committed_tx.compare_exchange_weak(
                      current,
                      sst_max_tx,
                      Ordering::SeqCst,
                      Ordering::Relaxed,
                  ) {
                      Ok(_) => break,
                      Err(actual) => current = actual,
                  }
              }
          }

          drop(sstables);
          drop(state);
  =======
          let sst_max_tx = reader.metadata().max_tx_id;
          if sst_max_tx < TxId::INTERNAL_BASE {
              let mut current = self.last_committed_tx.load(Ordering::Acquire);
              while sst_max_tx > current {
                  match self.last_committed_tx.compare_exchange_weak(
                      current,
                      sst_max_tx,
                      Ordering::SeqCst,
                      Ordering::Relaxed,
                  ) {
                      Ok(_) => break,
                      Err(actual) => current = actual,
                  }
              }
          }

          sstables.push(Arc::new(reader));
          drop(sstables);
          drop(state);
  >>>>>>> REPLACE
  ```

---

#### **[MITTEL] - Maskierung unvollständiger Lifecycle-Pins in `SnapshotRegistry::release`**
* **Pfad/Modul:** `crates/memfuse-core/src/snapshot.rs` (`SnapshotRegistry::release`)
* **Mechanismus:**
  `SnapshotRegistry::release` verarbeitet Entpinnungen von Sequenznummern wie folgt:
  ```rust
  pub(crate) fn release(&self, seq_no: u64) {
      let seq_no = seq_no & !TOMBSTONE_BIT;
      let mut active = self.active.lock();
      if let Some(count) = active.get_mut(&seq_no) {
          *count -= 1;
          if *count == 0 {
              active.remove(&seq_no);
          }
      } else {
          // Unpinning or releasing an un-tracked sequence number is a no-op (§2 Zero-Panic).
      }
      self.update_min(&active);
  }
  ```
  Wenn ein Caller versehentlich `unpin` mit einer abweichenden `seq_no` aufruft (oder bei unkorrektem Handling von `TOMBSTONE_BIT`), schlägt der Look-up laut Silent-Fail-Dokumentation fehl. Der ursprünglich registrierte Pin bleibt jedoch mit `count >= 1` in der `BTreeMap` bestehen. Dadurch bleibt `min_active_seqno()` dauerhaft auf dem veralteten niedrigen Wert blockiert. In der Folge kann die `CompactionEngine` keine alten Tombstones mehr löschen (Garbage Collection wird schleichend deaktiviert, Disk-Usage wächst unbegrenzt).
* **Verletzte Invariante:** Zero-Panic & Snapshot Lifecycle Safety (ADR-025): Fehlgeschlagene Deregistrierungen dürfen nicht stumm bleiben, sondern müssen diagnostizierbar geloggt werden.
* **Lösungsvorschlag:**
  Füge im `else`-Zweig ein `tracing::warn!` ein, um Lifecycle-Lecks sofort im Monitoring sichtbar zu machen:
  ```rust
  <<<<<<< SEARCH
          } else {
              // Unpinning or releasing an un-tracked sequence number is a no-op (§2 Zero-Panic).
          }
  =======
          } else {
              tracing::warn!(
                  seq_no = seq_no,
                  "SnapshotRegistry::release aufgerufen für nicht getrackte seq_no. Mögliches Pin-Leck!"
              );
          }
  >>>>>>> REPLACE
  ```

---

### 2. LSM-Tree State-Machine & Concurrency (`memfuse-store`)

#### **[KRITISCH] - Verwaiste SSTable-Dateien bei abgebrochener Compaction führen zu korrumpiertem Recovery-Zustand**
* **Pfad/Modul:** `crates/memfuse-store/src/compaction.rs` (`CompactionEngine::maybe_compact` / `merge_sstables`)
* **Mechanismus:**
  1. `CompactionEngine::maybe_compact` generiert mit `Self::generate_sst_path(data_path)` einen neuen Dateipfad (z. B. `sst-compact-1700000000-0001.sst`).
  2. Anschließend wird `merge_sstables` ausgeführt, das via `SstableBuilder` Daten in die Datei schreibt.
  3. Wird der Vorgang während `merge_sstables` abgebrochen (z.B. durch Task-Cancellation via `CancellationToken` oder I/O-Fehler wie `DiskFull`), wird die halbgeschriebene `.sst`-Datei nicht gelöscht.
  4. Startet der Prozess neu, durchsucht `LsmStorage::new()` das Verzeichnis nach allen Dateien mit der Endung `.sst`:
     ```rust
     if entry.path().extension().is_some_and(|ext| ext == "sst") {
         sst_files.push(entry.path());
     }
     ```
  5. Die unvollständige, verwaiste Compaction-SSTable wird geöffnet und in die aktive SSTable-Liste aufgenommen. Dies führt zu ungültigen Index-Slices, duplizierten Keys oder Abstürzen beim Replay.
* **Verletzte Invariante:** Atomic State Transition Invariante (ADR-003): SSTables dürfen erst dann für das System oder für Recovery-Prozesse sichtbar sein, wenn sie vollständig geschrieben, geflusht und atomar umbenannt wurden.
* **Lösungsvorschlag:**
  Verwende eine temporäre Dateiendung (`.sst.tmp`) während des Aufbaus und benenne die Datei erst nach erfolgreichem `builder.finish()` atomar um. Implementiere zudem einen Cleanup-Guard:
  ```rust
  <<<<<<< SEARCH
          let output_path = Self::generate_sst_path(data_path)?;
          self.merge_sstables(
              &input_ssts,
              &output_path,
              min_snapshot_seq,
              is_full_compaction,
          )
          .await?;
  =======
          let final_output_path = Self::generate_sst_path(data_path)?;
          let tmp_output_path = final_output_path.with_extension("sst.tmp");

          let merge_res = self.merge_sstables(
              &input_ssts,
              &tmp_output_path,
              min_snapshot_seq,
              is_full_compaction,
          ).await;

          if let Err(e) = merge_res {
              let _ = tokio::fs::remove_file(&tmp_output_path).await;
              return Err(e);
          }

          tokio::fs::rename(&tmp_output_path, &final_output_path)
              .await
              .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Failed to rename SSTable: {}", e)))?;
          let output_path = final_output_path;
  >>>>>>> REPLACE
  ```

---

#### **[HOCH] - Unvollständiges WAL-Cleanup bei fehlgeschlagenen Dateisystem-Löschungen in `rollback_to_tx`**
* **Pfad/Modul:** `crates/memfuse-store/src/lsm.rs` (`LsmStorage::rollback_to_tx`)
* **Mechanismus:**
  Bei `rollback_to_tx` werden überschüssige SSTables aus dem In-Memory-Zustand entfernt und zur Löschung auf der Disk vorgemerkt:
  ```rust
  for path in sst_to_remove {
      if let Err(e) = tokio::fs::remove_file(&path).await {
          tracing::error!(path = ?path, "Orphaned SSTable konnte nicht entfernt werden: {e}.");
      }
  }
  ```
  Schlägt `tokio::fs::remove_file` fehl (z. B. wegen temporärer File-Locks unter Windows oder Berechtigungsproblemen), wird der Fehler zwar geloggt, aber ignoriert. Wenn die Datenbank zu einem späteren Zeitpunkt neu gestartet wird, liest `LsmStorage::new()` alle `.sst`-Dateien im Ordner neu ein. Die verwaiste SSTable, die Daten **nach** der Rollback-Ziel-Transaktion enthält, wird wieder geladen. Dadurch tauchen bereits zurückgerollte Transaktionen nach einem Neustart als "Phantom-Daten" wieder auf.
* **Verletzte Invariante:** Durable Rollback Contract (ADR-023): Ein physisch durchgeführter Rollback auf `target_tx` muss auch nach einem Neustart des Prozesses strikt garantiert bleiben.
* **Lösungsvorschlag:**
  Benenne SSTables vor dem Löschen atomar in ein Trash-Verzeichnis um oder verwende Marker-Header, damit verwaiste Dateien beim Startup ignoriert werden. Schlägt das Löschen fehl, sollte eine explizite Rollback-Tombstone-Manifestdatei aktualisiert werden.

---

### 3. Integrität & Fehlerbehandlung (`MemFuseError`)

#### **[HOCH] - Silent Truncation von korrumpierten WAL-V2-Batch-Chunks am Dateiende**
* **Pfad/Modul:** `crates/memfuse-store/src/wal.rs` (`Wal::replay_with_size`)
* **Mechanismus:**
  Beim Replay von WAL-V2 Batch-Verschlüsselungsblöcken wird folgender Entschlüsselungspfad durchlaufen:
  ```rust
  let decrypted_data = match km.decrypt_auto_nonce(&entry_data_raw[12..], &nonce) {
      Ok(data) => data,
      Err(e) => {
          if pos >= file_size {
              tracing::warn!(
                  "WAL truncation at tail (offset {}), decryption failed: {}",
                  chunk_start_pos,
                  e
              );
              break;
          } else {
              return Err(MemFuseError::wal_corruption(
                  chunk_start_pos,
                  format!("Decryption failed: {}", e),
              ));
          }
      }
  };
  ```
  Wenn durch einen abrupten Stromausfall ein Batch-Chunk unvollständig auf die Disk geschrieben wurde (z.B. fehlen die letzten 16 Bytes des AES-GCM-Tag), schlägt `decrypt_auto_nonce` fehl. Da der Teil-Write am Ende der Datei liegt, evaluiert `pos >= file_size` zu `true`.
  Der Code behandelt dies als "normale" Tail-Truncation, bricht die Replay-Schleife mit `break` ab und gibt `Ok(entries)` zurück. Der Aufrufer erhält ein erfolgreiches Replay-Ergebnis, obwohl eine von der Anwendung als comittet bestätigte Transaktion lautlos verworfen wurde. Es erfolgt keine Eskalation an den Caller oder das Monitoring.
* **Verletzte Invariante:** WAL Strict Integrity Guarantee (ADR-002 / `SECURITY.md`): Partial-Writes von committeten Batches dürfen nicht unbemerkt als valides Log-Ende akzeptiert werden, wenn dadurch Datenverlust entsteht.
* **Lösungsvorschlag:**
  Prüfe vor dem Akzeptieren einer Tail-Truncation, ob der angefangene Block ein korrektes Längenpräfix besaß. Wenn ein angefangener Chunk nicht vollständig entschlüsselt werden kann, muss zumindest ein Audit-Log mit hoher Priorität (`tracing::error!`) erzeugt und der Truncation-Zustand im `Wal`-Objekt vermerkt werden.

---

#### **[MITTEL] - Inkompatibles Memory-Accounting bei Rollback-Operationen im `ResourceTracker`**
* **Pfad/Modul:** `crates/memfuse-store/src/lsm.rs` (`commit`, `rollback`, `rollback_to_tx`) & `crates/memfuse-core/src/types/budget.rs` (`ResourceTracker`)
* **Mechanismus:**
  1. Bei jedem Commit konsumiert `LsmStorage::commit()` Speicher im globalen `ResourceTracker`:
     ```rust
     self.budget.consume_memory(entry_size as u64)?;
     ```
  2. Bei einem MemTable-Flush gibt `LsmStorage::flush()` den freigewordenen Speicher wieder frei:
     ```rust
     self.budget.release_memory(bytes_freed);
     ```
  3. Bei Aufruf von `rollback(tx_id)` oder `rollback_to_tx(target_tx)` werden jedoch MemTable-Einträge gelöscht, **ohne** `self.budget.release_memory(...)` aufzurufen.
  4. Nach mehreren Rollbacks weicht der im `ResourceTracker` registrierte Speicherverbrauch (`memory_used`) drastisch vom tatsächlichen In-Memory-Bedarf der MemTables ab. Dies führt dazu, dass spätere `put()`-Operationen fälschlicherweise mit `MemFuseError::Storage("Memory budget exceeded (95%)")` abgelehnt werden (künstliche Denial-of-Service-Situation).
* **Verletzte Invariante:** Accurate Resource Accounting (ADR-016 / `budget.rs`): `ResourceTracker::memory_used()` muss die reale Speichernutzung der aktiven In-Memory-Strukturen exakt abbilden.
* **Lösungsvorschlag:**
  Erweitere `MemTable::rollback` um die Rückgabe der exakt freigegebenen Byte-Anzahl und rufe im LSM-Tree `self.budget.release_memory(freed_bytes)` auf:
  ```rust
  <<<<<<< SEARCH
      async fn rollback(&self, tx_id: TxId) -> Result<()> {
          self.tx_buffer.discard(tx_id);
          Ok(())
      }
  =======
      async fn rollback(&self, tx_id: TxId) -> Result<()> {
          self.tx_buffer.discard(tx_id);
          let state = self.state.read().await;
          let freed = state.memtable.rollback_with_bytes(tx_id.inner());
          if freed > 0 {
              self.budget.release_memory(freed as u64);
          }
          Ok(())
      }
  >>>>>>> REPLACE
  ```

---

### 4. Cross-Cutting Risks

#### **[HOCH] - Potential Deadlock in `LsmStorage::rollback_to_tx` durch inverse Lock-Akquisition**
* **Pfad/Modul:** `crates/memfuse-store/src/lsm.rs` (`LsmStorage::rollback_to_tx`)
* **Mechanismus:**
  In `LsmStorage` existieren zwei primäre RwLocks für den Zustand: `self.state` (überwacht MemTable & WAL) und `self.sstables` (überwacht die Liste der SSTable-Reader).

  Die reguläre Lock-Hierarchie im System lautet:
  - **Read-Path / Standard:** Erst `self.state.read()`, danach `self.sstables.read()`.
  - **Flush-Path:** Erst `self.state.write()`, gibt `state` frei, holt danach `self.sstables.write()`.

  In `LsmStorage::rollback_to_tx` wird folgende Sequenz ausgeführt:
  ```rust
  let _commit_lock = self.commit_mutex.lock().await;
  let mut state = self.state.write().await; // Lock A (state)
  // ... WAL Truncation ...
  let mut sstables_lock = self.sstables.write().await; // Lock B (sstables) während Lock A gehalten wird!
  ```
  Wenn zeitgleich ein paralleler Background-Task (z. B. ein benutzerdefinierter Reader oder Scan) `self.sstables.read()` (Lock B) hält und im nächsten Schritt `self.state.read()` (Lock A) anfordert, entsteht ein klassischer AB-BA Deadlock unter hoher Last.
* **Verletzte Invariante:** Deadlock-Free Lock Hierarchy (ADR-003): Es muss eine strikte, einheitliche Akquisitionsreihenfolge für alle systemweiten Locks eingehalten werden. Lock A darf niemals gehalten werden, während Lock B angefordert wird, wenn an anderer Stelle Lock B vor Lock A akquiriert werden kann.
* **Lösungsvorschlag:**
  Löse die Schreibsperre auf `self.state` temporär auf, bevor die Schreibsperre auf `self.sstables` akquiriert wird, da beide durch den bereits gehaltenen `commit_mutex` vor konkurrierenden Commits geschützt sind:
  ```rust
  <<<<<<< SEARCH
          let _commit_lock = self.commit_mutex.lock().await;
          let mut state = self.state.write().await;

          // 1. Truncate WAL to the position after target_tx
          let (target_offset, target_hmac) = state.wal.find_tx_offset(target_tx).await?;
          state.wal.truncate(target_offset, target_hmac).await?;

          // 2. Clear current memtable (it might have data > target_tx)
          state.memtable = Arc::new(MemTable::new());
          state.immutable_memtables.clear();

          // 3. Handle SSTables
          let mut sstables_lock = self.sstables.write().await;
  =======
          let _commit_lock = self.commit_mutex.lock().await;

          // Phase 1: State Mutation
          {
              let mut state = self.state.write().await;
              let (target_offset, target_hmac) = state.wal.find_tx_offset(target_tx).await?;
              state.wal.truncate(target_offset, target_hmac).await?;
              state.memtable = Arc::new(MemTable::new());
              state.immutable_memtables.clear();
          }

          // Phase 2: SSTable Mutation (ohne gehaltenes state.write())
          let mut sstables_lock = self.sstables.write().await;
  >>>>>>> REPLACE
  ```

---

#### **[MITTEL] - Fehlende Bereichsvalidierung für `TxId`-Grenzwerte in `LsmStorage::commit`**
* **Pfad/Modul:** `crates/memfuse-core/src/types/domain.rs` & `crates/memfuse-store/src/lsm.rs` (`commit`)
* **Mechanismus:**
  `TxId::INTERNAL_BASE` (`u64::MAX - 1_000_000`) trennt benutzerdefinierte Collection-Transaktionen (`[1, ~10^12]`) von internen System-Transaktionen.
  In `LsmStorage::commit()` wird geprüft:
  ```rust
  if tx_id.inner() < TxId::INTERNAL_BASE {
      let mut current = self.last_committed_tx.load(Ordering::Acquire);
      while tx_id.inner() > current {
          // Update last_committed_tx
      }
  }
  ```
  Wenn eine fehlerhafte Upstream-Komponente (z. B. ein benutzerdefinierter Treiber) eine ungeprüfte `TxId` mit einem Wert nahe `TxId::INTERNAL_BASE` übergibt (oder versehentlich Wall-Clock-Timestamp-Nanosekunden `~1.7e18`), akzeptiert `commit()` diese Transaktion klaglos und hebt `last_committed_tx` auf den enormen Wert an.
  Alle darauffolgenden regulären Transaktionen (`tx_id` 100, 101, etc.) liegen nun numerisch weit unter `last_committed_tx`. Da die Sichtbarkeitsprüfungen in `get_at_seq()` und `scan_prefix_at()` den Filter `tx <= snapshot_tx` anwenden, werden nachfolgende reguläre Commits für Point-in-Time-Reads unauffindbar (System-Blackout).
* **Verletzte Invariante:** TxId Domain Separation Guarantee (AGT-GRAPH-001 / ADR-016): Transaktions-IDs von Benutzern müssen monoton sequentiell vergeben werden. Ausreißer im Zwischenbereich zwischen Collection-Sequenz und `INTERNAL_BASE` dürfen nicht als gültige Transaktions-IDs akzeptiert werden.
* **Lösungsvorschlag:**
  Füge in `LsmStorage::commit()` eine strikte Validierung der `TxId`-Range ein:
  ```rust
  <<<<<<< SEARCH
          if tx_id.inner() < TxId::INTERNAL_BASE {
              let mut current = self.last_committed_tx.load(Ordering::Acquire);
  =======
          // Validierung der Transaktions-ID Range
          if tx_id.inner() > 1_000_000_000_000 && tx_id.inner() < TxId::INTERNAL_BASE {
              return Err(MemFuseError::InvalidInput(format!(
                  "Invalid TxId {}: TxId falls into reserved unallocated range between collection sequence and INTERNAL_BASE",
                  tx_id
              )));
          }

          if tx_id.inner() < TxId::INTERNAL_BASE {
              let mut current = self.last_committed_tx.load(Ordering::Acquire);
  >>>>>>> REPLACE
  ```

---

## Zusammenfassung & Handlungsempfehlungen

1. **Sofortige Behebung von Finding 1 & 4 (Kritisch):**
   - In `lsm.rs::rollback_to_tx` muss das `TOMBSTONE_BIT` bei der Ermittlung von `max_seq` maskiert werden (`& !TOMBSTONE_BIT`), um irreversible System-Blockaden zu verhindern.
   - In `compaction.rs` müssen temporäre Compaction-Dateien mit `.sst.tmp` erzeugt und erst nach FSync atomar umbenannt werden, damit fehlgeschlagene Compactions keine korrupten SSTables hinterlassen.

2. **Absicherung der Lock-Hierarchien & Sichtbarkeit (Finding 2 & 7):**
   - Entkopplung der Locks in `rollback_to_tx`, um AB-BA Deadlocks mit parallelen Readers/Flushes auszuschließen.
   - Aktualisierung von `last_committed_tx` in `flush()` vor dem Entlocken der SSTable-Liste.

3. **Integrität & Accounting (Finding 5, 6 & 8):**
   - Exakte Speicherrückgabe im `ResourceTracker` bei Rollbacks verbuchen.
   - Strikte Validierung von `TxId`-Ranges in `LsmStorage::commit()`.
