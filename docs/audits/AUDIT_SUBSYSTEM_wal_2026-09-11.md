# WAL Subsystem Tiefen-Audit Report (`wal.rs`)

**Datum:** 2026-09-11
**Crate:** `memfuse-store` (Layer 1 / Layer 2 Storage Engine)
**Zieldatei:** `crates/memfuse-store/src/wal.rs` (XL-Datei, 3.598 Zeilen)
**Auditor-Session:** `$SESSION_HASH` (Task ID: `JULES-20260911-WALAUD`)

---

## Executive Summary & Scope

Im Rahmen dieses Subsystem-Audits wurde die Write-Ahead Log Implementation (`crates/memfuse-store/src/wal.rs`) vollständig analysiert. Der Fokus lag auf Crash-Safety, HMAC-Kettenintegrität, Dateisystem-Synchronisation (`fsync`), Replay-Sicherheit, TOCTOU-Schutz und Backup/Recovery-Pfaden.

### Inventar-Realitätsabgleich (Stand 2026-09-10 vs. Aktuell)
- **Prompter-Inventar (2026-09-10):** `checkpoint.rs`, `compaction.rs`, `lib.rs`, `lsm.rs`, `memtable.rs`, `mmap.rs`, `sstable.rs`, `tenant_codec.rs`, `util.rs`, `wal.rs`
- **Tatsächliches Repo-Inventar:** `checkpoint.rs`, `compaction.rs`, `lib.rs`, `lsm.rs`, `manifest.rs`, `memtable.rs`, `mmap.rs`, `sstable.rs`, `tenant_codec.rs`, `util.rs`, `wal.rs`
- **Befund:** `Inventar-Drift: Datei crates/memfuse-store/src/manifest.rs im Prompter-Inventar vom 2026-09-10 nicht erfasst`.

---

## Phase W-2: Verifikation der WAL-Invarianten

| Invariante | Prüfung / Referenz in `wal.rs` | Status | Detailbefund |
|---|---|---|---|
| **C-2: `sync_all()` nach `set_len()` in `truncate()`** | `wal.rs:1705-1714` | 🟢 VERIFIZIERT / OK | `Wal::truncate` führt `file.set_len(offset)` gefolgt von einem unmittelbaren `file.sync_all()` aus, bevor der In-Memory-Seek-Cursor angepasst wird. AI-TAG `audit-C-2` ist als `RESOLVED` markiert. |
| **C-3: Atomare Doppelprüfung in `append_batch()`** | `wal.rs:1048-1090` | 🟢 VERIFIZIERT / OK | `append_batch` erwirbt den exklusiven Mutex-Lock `self.file.lock().await` vor der Evaluierung von `header_written` und `size`. Beide Bedingungen werden im kritischen Abschnitt geprüft, wodurch TOCTOU-Header-Duplikation verhindert wird. AI-TAG `audit-C-3` ist als `ANALYZED-SAFE` dokumentiert. |
| **APM-1: Atomare Datei-Erstellung** | `wal.rs:818-935` | 🟢 VERIFIZIERT / OK | Integritätsschlüssel (`.integrity.key`) und WAL UUID (`.uuid`) werden via Dateisystem-Locks / Atomic Write Patterns mit korrekten POSIX-ACLs (`0600`) erstellt und synchronisiert. |
| **APM-38: Sequenz-/TxId-Bindung (Replay-Schutz)** | `wal.rs:117-148` & `wal_crypto.rs:219-245` | 🟢 VERIFIZIERT / OK | `WalEntry::compute_checksum_v3` bindet `prev_hmac`, `seq_no`, `tx_id` (little-endian) sowie Längen-Präfixe von Key und Value in den HMAC-SHA256 ein. Cross-Referenz mit `WalCryptoVerifier::verify_and_update_v3` bestätigt exakte Byte-für-Byte-Gleichheit. |
| **H-6: Backup-Restore-Pfad** | `wal.rs:490`, `635-675`, `1603` | 🟢 VERIFIZIERT / OK | Priorisiert vor dem WAL-Öffnen prüft `recover_from_bak_if_present` das Vorhandensein von `.v1.bak` / `.v2.bak` Dateien (z.B. nach abgebrochener V3-Migration) und stellt diese wieder her. AI-TAG `audit-H-6` ist als `RESOLVED` markiert. |
| **V3-HMAC-Byte-Reihenfolge** | `wal.rs:117-148` & `wal_crypto.rs:219-245` | 🟢 VERIFIZIERT / OK | Die Byte-Reihenfolge für V3 HMAC (`prev_hmac` -> `seq_no` -> `tx_id` -> `op_type` -> `key_len` -> `key` -> `val_len` -> `val`) ist zwischen `memfuse-store` und `memfuse-crypto` identisch implementiert. |

---

## Crash-Recovery & Simulation

- Die Modul-Tests in `wal.rs` decken Replay-Truncation, Tail-Corruption, Middle-CRC-Corruption, Key-Migration, HMAC-Ketten-Integrität und synchrone Truncation-Flushes ab.
- Sämtliche WAL-spezifischen Unit- und Integrationstests wurden mit `cargo test -p memfuse-store wal` ohne Fehler ausgeführt.

---

## Fazit & Audit-Ergebnis

Die Write-Ahead Log Komponente `crates/memfuse-store/src/wal.rs` befindet sich in einem sehr robusten und gehärteten Zustand. Alle bekannten historischen Risiken (C-2, C-3, H-6, APM-1, APM-38) wurden erfolgreich gehärtet, verifiziert und dokumentiert. In dieser Sitzung wurden keine neuen funktionalen Codeänderungen vorgenommen (strikte Auditor-Rollensperre).
