# Audit-Bericht: Mmap TOCTOU Dynamic File Truncation & Unlink Risks (`memfuse-index`)

**Datum:** 30. August 2026
**Auditor:** Senior Rust Systems Engineer (Spezialgebiet Memory-Mapped-I/O-Sicherheit)
**Ziel-Crate:** `crates/memfuse-index` (`diskann.rs`, `persistence.rs`, `hnsw.rs`)
**Status:** Audit Abgeschlossen — Empirische Bestätigung durchgeführt

---

## 1. Executive Summary & Risiko-Verdikt

### Risiko-Verdikt: CRITICAL

Der Audit identifiziert eine **kritische strukturelle Sicherheitslücke** in den Memory-Mapped-I/O-Schichten von `DiskAnnIndex` (`crates/memfuse-index/src/diskann.rs`) und `MmapIndex` / `HnswIndex` (`crates/memfuse-index/src/persistence.rs` & `hnsw.rs`).

* **Statische vs. Dynamische Bounds-Checks:** Die in Runde 1 eingeführten sicheren Bounds-Checks via `.get()` schützen ausschließlich gegen den statischen Fall (Datei ist bereits beim Öffnen zu kurz). Sie bieten jedoch **keinerlei Schutz** gegen den dynamischen TOCTOU-Fall (Time-of-Check-to-Time-of-Use), bei dem eine mmap'te Index-Datei während aktiver Nutzung durch externe Prozesse, System-Tools, Nutzer-Aktionen oder Fehlkonfigurationen in-place trunkiert wird (`set_len()`).
* **Betriebssystem-Ebene Absturz (SIGBUS):** Wenn eine Datei trunkiert wird, während ein aktives mmap auf Adressbereiche außerhalb der neuen Dateigröße verweist, löst jede Speicherseite, die vom Betriebssystem nachgeladen wird, ein **SIGBUS-Signal (Signal 7)** aus. Da SIGBUS auf OS-Ebene erzeugt wird, greift kein Rust-Bounds-Checking (`.get()`) und kein reguläres Error-Handling (`Result<T, E>`). Der Prozess wird vom Betriebssystem sofort und unweigerlich beendet (Crash mit Exit-Code 135).
* **Auswirkung auf Produktivumgebungen:** In einer Air-Gapped Agenten-Produktivumgebung führt ein SIGBUS-Crash zum kompletten Ausfall der Prozess-Instanz.
* **Empirische Reproduktionsgarantie:** Der kontrollierte Subprozess-Test `test_mmap_toctou_truncation_causes_sigbus` hat den Prozessabsturz via SIGBUS (Signal 7) empirisch zweifelsfrei nachgewiesen.

---

## 2. Mmap-Lebenszyklus-Analyse (Code-Pfade)

Ein systematisches Code-Tracing der Komponenten in `crates/memfuse-index` zeigt folgende Lebenszyklus-Eigenschaften:

### A. Handles & Storage
1. **`MmapIndex` (`persistence.rs`):**
   - Öffnet Datei via `std::fs::File::open` (Read-Only) und erzeugt `memmap2::Mmap::map(&file)`.
   - Wird von `Arc<Mmap>` gehalten. Der File-Descriptor verbleibt bis zum Drop im Speicher.
   - Genutzt von `HnswIndex` für Offloaded-Layer-Read-Operations.
2. **`DiskAnnIndexInner` (`diskann.rs`):**
   - Öffnet `index_path` via `std::fs::File::open` in `load()` und erzeugt `memmap2::Mmap::map(&file)`.
   - Speichert die Mapping-Struktur in `RwLock<Option<Mmap>>`.
   - Genutzt für dynamische Graph-Traversierungen (`load_node`) und Vektor-Lesezugriffe (`search_blocking`).

### B. Interne Mutations- und Schreibpfade
* **`HnswIndex::save()` & `DiskAnnIndex::write_to_file()`:**
  - Beide interne Speichermethoden schreiben neue Indexstände **nicht in-place**, sondern in eine temporäre Datei (`.hnsw.tmp` bzw. `.idx.tmp`).
  - Nach `flush()` und `sync_all()` wird die temporäre Datei via `std::fs::rename()` / `tokio::fs::rename()` atomar über den Zielpfad geschoben.
  - Anschließend wird das Eltern-Verzeichnis gesynchronisiert (`sync_all()`).

### C. Schwachstelle
Intern schützt die Software sich selbst durch das Atomic-Rename-Muster vor Selbst-Korrumpierung. **Es existiert jedoch kein Schutzmechanismus gegen externe oder parallele Zugriffe**, die die Datei auf dem Dateisystem direkt modifizieren (`std::fs::OpenOptions::new().write(true).open(path)` gefolgt von `set_len(small_size)`).

---

## 3. TOCTOU-Race-Testergebnis (Truncation-Szenario)

* **Testaufbau:** `test_mmap_toctou_truncation_causes_sigbus` in `crates/memfuse-index/tests/mmap_toctou_test.rs`.
* **Ablauf:**
  1. Ein `DiskAnnIndex` (1.000 Vektoren, Dim 128) und ein `HnswIndex` werden erstellt, persisted und via Mmap geladen.
  2. Ein isolierter Child-Prozess hält aktive Mmap-Handles auf beide Dateien.
  3. Die zugrunde liegenden Dateien werden in-place mit `set_len(10)` auf 10 Bytes trunkiert.
  4. Der Child-Prozess führt eine Vektorsuche (`search()`) aus.
* **Ergebnis:**
  ```text
  --- TRUNCATION CHILD STDOUT ---
  Active mmap handles opened for DiskANN & HNSW.
  Truncating DiskANN and HNSW files to 10 bytes in-place...
  Files truncated! Attempting queries against mmap regions...
  Executing DiskANN search_internal...

  Child exit status: ExitStatus(unix_wait_status(7)), Signal: Some(7)
  ```
* **Befund:** Der Prozess stürzt sofort beim ersten Lesezugriff auf das ge-mmap'te Speichersegment mit **SIGBUS (Signal 7)** ab.

---

## 4. TOCTOU-Race-Testergebnis (Deletion-Szenario)

* **Testaufbau:** `test_mmap_toctou_deletion_succeeds_safely` in `crates/memfuse-index/tests/mmap_toctou_test.rs`.
* **Ablauf:**
  1. Ein `DiskAnnIndex` und ein `HnswIndex` werden erstellt und via Mmap gemappt.
  2. Die Dateien werden über das Dateisystem via `std::fs::remove_file()` vollständig gelöscht (`unlink`).
  3. Es wird verifiziert, dass die Dateipfade auf der Festplatte nicht mehr existieren.
  4. Der Child-Prozess führt Vektorsuchen (`search()`) gegen die gemappten Indizes aus.
* **Ergebnis:**
  ```text
  --- DELETION CHILD STDOUT ---
  Active mmap handles opened for DiskANN & HNSW.
  Deleting index files from filesystem...
  Files unlinked successfully.
  Querying mmap'd indexes post-deletion...
  CHILD_DELETION_SUCCESSFUL
  ```
* **Befund:** Der Test verläuft **erfolgreich ohne Fehler oder Absturz (Exit-Code 0)**. Auf POSIX/Linux-Systemen hält das Betriebssystem die Inode und Datenblöcke eines geöffneten File-Descriptors aufrecht, solange offene Handles bestehen. `remove_file` entfernt lediglich den Verzeichniseintrag (`unlink`), während das mmap-Segment voll funktionsfähig und sicher im Speicher verbleibt.

---

## 5. Vorhandene Schutzmechanismen-Bewertung

| Schutzmechanismus | Vorhanden? | Wirksamkeit & Bewertung |
| :--- | :---: | :--- |
| **Sichere Bounds-Checks (`.get()`)** | JA | **Unwirksam gegen dynamische Truncation.** Schützt nur beim ersten Laden einer verkürzten Datei; greift nicht bei nachfolgendem SIGBUS auf OS-Paging-Ebene. |
| **Atomic Rename (`.tmp` -> `rename`)** | JA | **Wirksam für interne Writes.** Schützt aktive Mmap-Reader vor internen Compaction-/Save-Operationen. |
| **Advisory File Locks (`flock` / `fcntl`)** | **NEIN** | Keine Datei-Sperren auf OS-Ebene vorhanden. Externe Schreibprozesse werden nicht blockiert. |
| **Linux File Sealing (`F_SEAL_SHRINK`)** | **NEIN** | Keine Inode-Versiegelung gegen Truncation vorhanden. |
| **Signal-Handler / SIGBUS Catching** | **NEIN** | Kein benutzerdefinierter Signal-Handler zur Umwandlung von SIGBUS in Instanz-Fehler vorhanden. |

---

## 6. Konkrete Absicherungsvorschläge

Um die Systemarchitektur gegen SIGBUS-Abstürze in Produktivumgebungen vollständig zu härten, werden folgende vier Schutzschichten empfohlen:

### 1. File Locking via Advisory Shared/Exclusive Locks (`flock`)
* **Konzept:** Beim Öffnen der Index-Datei für Mmap wird ein shared lock (`flock(fd, LOCK_SH)`) erworben. Schreib- oder Truncation-Prozesse müssen vor der Modifikation ein exclusive lock (`LOCK_EX`) anfordern.
* **Vorteil:** Verhindert koordinierte Schreib- und Truncation-Zugriffe auf Betriebssystem-Ebene.

### 2. Linux File Sealing (`F_ADD_SEALS` & `F_SEAL_SHRINK`)
* **Konzept:** Wenn Indizes auf `memfd_create` oder unterstützten Dateisystemen liegen, kann nach dem Erstellen das Seal `F_SEAL_SHRINK` via `fcntl(fd, F_ADD_SEALS, ...)` gesetzt werden.
* **Vorteil:** Das OS weist jeden Versuch, die Dateigröße via `ftruncate()` / `set_len()` zu verkleinern, direkt mit `EPERM` / `EBADF` zurück.

### 3. Strict Atomic Replacement Protocol Enforcement
* **Konzept:** Verankerung der Invariante in der System-Architektur, dass Index-Dateien niemals in-place überschrieben oder trunkiert werden dürfen. Alle Modifikationen müssen zwingend über das Copy-on-Write / Temp-File & Rename-Muster erfolgen.

### 4. Optional: Custom SIGBUS Signal Handler (`sigaction` + `sigsetjmp`)
* **Konzept:** Registrierung eines plattformspezifischen SIGBUS-Signal-Handlers, der Page-Faults auf ge-mmap'ten Bereichen abfängt und via `siglongjmp` sauber in eine Rust-Panik oder einen kontrollierten Error-State umwandelt.

---

## 7. Anhang: Rohlogs

### Raw Test Execution Log
```text
running 3 tests
test run_subcommand_target ... ok
test test_mmap_toctou_deletion_succeeds_safely ... ok
test test_mmap_toctou_truncation_causes_sigbus ... ok

--- TRUNCATION CHILD STDOUT ---
Active mmap handles opened for DiskANN & HNSW.
Truncating DiskANN and HNSW files to 10 bytes in-place...
Files truncated! Attempting queries against mmap regions...
Executing DiskANN search_internal...

Child exit status: ExitStatus(unix_wait_status(7)), Signal: Some(7)

--- DELETION CHILD STDOUT ---
Active mmap handles opened for DiskANN & HNSW.
Deleting index files from filesystem...
Files unlinked successfully.
Querying mmap'd indexes post-deletion...
CHILD_DELETION_SUCCESSFUL

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
```

---
*Ende des Audit-Berichts.*
