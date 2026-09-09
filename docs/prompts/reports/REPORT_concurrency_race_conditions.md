# Concurrency & Race Conditions Audit Report

**Datum:** 2026-08-30
**Geprüfte Crates:** `memfuse-core`, `memfuse-db`
**Fokussierte Dateien:**
- `crates/memfuse-core/src/tx_buffer.rs`
- `crates/memfuse-core/src/snapshot.rs`
- `crates/memfuse-db/src/collection/tx.rs`
- `crates/memfuse-db/src/transaction.rs`

---

## Executive Summary & Zusammenfassung

Die Auditierung der Concurrency-, MVCC- und Transaktions-Komponenten von MemFuse wurde erfolgreich durchgeführt. Alle 7 geprüften Fehlerklassen wurden auf Einhaltung der Invarianten und Thread-Sicherheit untersucht:

1. **Race Condition / Data Race:** `TxBuffer` nutzt feingranulares Sharding (`DEFAULT_SHARD_COUNT = 64`) mit unabhängigen `parking_lot::RwLock<TxShard<T>>`. Jeder Schreib- und Lesezugriff ist atomar geschützt.
2. **Lost Update / Phantom Erasure:** Staged Transactions im `TxBuffer` sind nach Transaktions-IDs (`TxId`) isoliert. Der Orphan Reaper scannt Shards strikt sequenziell in aufsteigender Indexreihenfolge (`0..N-1`) und gibt Shard-Locks nach jedem Schritt frei (`try_write`), um Phantom Erasure aktiver Transaktionen zu verhindern.
3. **Stale Read:** `SnapshotRegistry` verwendet atomares Reference Counting (`parking_lot::Mutex<BTreeMap<u64, usize>>`) zur Nachverfolgung gepinnter Sequenznummern und eine `AtomicU64` Variable für `min_active_seqno` mit `Release`-Writes und `Acquire`-Reads. Dies garantiert, dass unpinned Snapshots nicht vorzeitig dem GC unterliegen.
4. **Deadlock / Livelock / Starvation:** Die Lock-Hierarchie (`MemFuse::collections` RwLock -> `Collection::insert_lock` Mutex -> `Collection::embedder` RwLock) wird strikt eingehalten. In `DbTransaction` werden staging `std::sync::Mutex`-Guards nur kurzzeitig für synchrone Operationen gehalten und niemals über `.await`-Punkte hinweg aufrechterhalten.
5. **Lock Poisoning:** In `transaction.rs` verfangen alle Mutex-Acquisitions ein bekanntes Lock Poisoning Muster (`match guard { Ok(g) => g, Err(p) => p.into_inner() }`), sodass Panics in ge-lockten Bereichen nicht zu kaskadierenden Ausfällen führen.
6. **Unbounded Queue / Backpressure:** `TxBuffer` erzwingt Bounded Capacity über `max_ops_per_tx` (`DEFAULT_MAX_OPS_PER_TX = 10_000`). Das Überschreiten führt zum kontrollierten Abbruch mit `MemFuseError::Transaction(...)`.
7. **ABA-Problem:** Transaktions-ID-Allokation (`Collection::allocate_tx`) erfolgt über atomare `AtomicU64::fetch_add(1, Ordering::SeqCst)` mit harter Obergrenzen-Prüfung gegen `TxId::MAX_COLLECTION_SEQUENCE`.

---

## Verifikationsergebnisse & Stresstests

Ein spezialisierter Concurrency-Stresstest mit 120 parallelen Tokio-Tasks wurde in `crates/memfuse-db/tests/concurrency_stress.rs` erstellt. Er führt gleichzeitig Folgendes aus:
- Gleichzeitige Staged Transactions (`DbTransaction::commit` & `DbTransaction::rollback`)
- Parallele direkte Mutationen (`insert`, `get`, `delete`)
- Gleichzeitiger Orphan-Reaper-Lauf auf dem `TxBuffer`

### Testergebnisse
- `cargo test -p memfuse-core --lib tx_buffer`: **PASSED** (17/17 tests)
- `cargo test -p memfuse-core --lib snapshot`: **PASSED** (11/11 tests)
- `cargo test -p memfuse-db --lib transaction`: **PASSED** (3/3 tests)
- `cargo test -p memfuse-db --test concurrency_stress`: **PASSED** (1/1 test, 120 parallel tasks)

---

## Abnahmekriterien-Checkliste

| Kriterium | Status | Bemerkung |
|---|---|---|
| Kein Lock wird über `.await`-Punkte hinweg gehalten | **Erfüllt** | In `DbTransaction` werden `std::sync::MutexGuard` Instanzen vor `.await` freigegeben |
| Bounded Queue / Backpressure im `TxBuffer` strikt durchgesetzt | **Erfüllt** | `stage` und `stage_many` erzwingen `max_ops_per_tx` Obergrenze |
| Transaktions-Rollbacks hinterlassen sauberen Zustand | **Erfüllt** | Kompensierende Transaktionen stellen alte Zustände atomar über neu steigende `TxId` wieder her |
| Audit Report abgelegt unter `docs/prompts/reports/REPORT_concurrency_race_conditions.md` | **Erfüllt** | Dieser Report |
