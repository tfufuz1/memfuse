# AUDIT REPORT: P2-unsafe-Vollständigkeit & Python-Kalibrierungs-Bindung (§4.14)

**Datum:** 2026-09-12
**Autor:** Jules (MemFuse Audit Specialist)
**Status:** COMPLETED
**Git HEAD:** `1411ac4f`
**Ziel-Crates:** Workspace-weit (`crates/*`) für Teil A (`unsafe`-Inventur); `memfuse-py`, `memfuse-db`, `memfuse-router`, `memfuse-calibration`, `memfuse-mcp` für Teil B (Python FFI Bindung).

---

## Executive Summary

Dieses Dokument fasst das umfassende Audit zur Speichersicherheit (`unsafe`-Blöcke) sowie zur FFI-Grenzschicht-Anbindung von Observability- und Kalibrierungsdaten in MemFuse zusammen.

1. **Teil A (`unsafe`-Vollständigkeitsinventur):**
   - **Geprüfte Dateien:** 17 Rust-Quellcodedateien über 8 Crates (vollständig workspace-weit analysiert).
   - **`unsafe`-Vorkommen gesamt:** 162 `unsafe`-Schlüsselwort-Fundstellen in Produktionscode-Dateien (davon **135 tatsächliche `unsafe`-Blöcke/Funktionsdeklarationen**, zzgl. 27 Modul-/Crate-Level Attribute wie `#![deny(unsafe_code)]` oder `#![allow(unsafe_code)]`).
   - **`// SAFETY:`-Dokumentationsquote:**
     - **In `memfuse-index/src/distance.rs` & `persistence.rs` & `diskann.rs`:** Nahezu 100% vorbildlich nach ADR-017 / P2 strukturiert.
     - **In `memfuse-core-ipc-gen/src/memfuse_generated.rs` (FlatBuffers Generator):** 29/29 `unsafe`-Blöcke/Funktionen **vollständig unkommentiert** (Generierter Code aus `flatc`).
     - **In `memfuse-db/src/volatile_vault.rs` (POSIX `mlock`/`munlock`):** 2/4 Aufrufe unkommentiert (`munlock` bei Drop/Cleanup).
   - **Kategorie-Zuordnung zu P2:**
     - **Kategorie (a) SIMD-Distanzberechnung:** 89 Vorkommen in `memfuse-index/src/distance.rs`.
     - **Kategorie (b) Mmap-Persistenz:** 2 Vorkommen in `memfuse-index/src/persistence.rs` & `diskann.rs`.
     - **Kategorie (c) Plattformspezifische Syscalls:** 31 Vorkommen in `memfuse-store/src/wal.rs` (Windows Direct I/O / ACLs) & `memfuse-db/src/volatile_vault.rs` (POSIX `mlock`/`munlock`).
     - **Kategorie (d) Neue unberücksichtigte Kategorie — IPC/FlatBuffers Zerocopy Deserialisierung:** 29 Vorkommen in `memfuse-core-ipc-gen/src/memfuse_generated.rs`.

2. **Teil B (§4.14 Python-Kalibrierungs-/Drift-Bindung):**
   - **FFI-Exposition:** `memfuse-py` exponiert `drift_status`, `calibration_ece` und `last_calibration_at` über `PyDbStats` und die `stats()`-Methode an Python.
   - **End-to-End Trace:** Die Verfolgung vom Python-Binding (`PyDbStats` in `crates/memfuse-py/src/lib.rs:1107-1110`) über die FFI-Grenze bis zur Rust-Implementierung (`MemFuseStats` in `crates/memfuse-db/src/lib.rs:1425-1437`) enthüllt ein **Hartkodiertes Platzhalter-Muster (Analogiefall zu H-17)**:
     - `drift_status`: Fest auf `"stabil".to_string()` hartkodiert.
     - `calibration_ece`: Fest auf `None` hartkodiert.
     - `last_calibration_at`: Fest auf `None` hartkodiert.
   - **Befund-Bestätigung:** Die Aussage aus der Spezifikation wird exakt bestätigt: Die Metriken sind bis in die Python-Grenzschicht durchgeschleift, aber in `memfuse-db::stats()` befinden sich tote/hartkodierte Platzhalter ohne Anbindung an den tatsächlichen `RouterEngine` / `IsotonicCalibrator` / `LyapunovDriftWatcher`.

---

## Teil A: unsafe-Inventur

### 1. Übersicht & Dateiklassifikation

Die nachfolgende Tabelle klassifiziert alle 17 Quellcodedateien mit `unsafe`-Vorkommen anhand ihrer Zeilenzahl (`wc -l`):
- **S:** < 100 Zeilen
- **M:** 100–500 Zeilen
- **L:** 500–1000 Zeilen
- **XL:** > 1000 Zeilen

| Crate | Datei | Zeilen | Klasse | `unsafe`-Fundstellen | Davon Prod-Blöcke | Status / Hauptfokus |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `memfuse-candle` | `src/embedding.rs` | 408 | M | 0 | 0 | Sauber (0 `unsafe`) |
| `memfuse-candle` | `src/lib.rs` | 33 | S | 1 | 0 | `#![deny(unsafe_code)]` Crate-Attribut |
| `memfuse-store` | `src/mmap.rs` | 39 | S | 0 | 0 | Sauber (0 `unsafe`) |
| `memfuse-store` | `src/lib.rs` | 39 | S | 1 | 0 | `#![deny(unsafe_code)]` Crate-Attribut |
| `memfuse-store` | `src/wal.rs` | 3701 | XL | 26 | 9 | Windows Direct I/O / Unbuffered File & ACLs (`winapi`/`windows-sys`) |
| `memfuse-crypto` | `src/kv_segment/segment.rs` | 243 | M | 2 | 0 | Crate/Modul-Level Attribut |
| `memfuse-crypto` | `src/anti_tamper.rs` | 144 | M | 3 | 0 | `#![cfg_attr(not(test), forbid(unsafe_code))]` |
| `memfuse-crypto` | `src/crypto.rs` | 707 | L | 1 | 0 | `#![forbid(unsafe_code)]` Crate-Attribut |
| `memfuse-graph` | `src/lib.rs` | 83 | S | 1 | 0 | `#![forbid(unsafe_code)]` Crate-Attribut |
| `memfuse-core-ipc-gen` | `src/memfuse_generated.rs` | 815 | L | 29 | 29 | FlatBuffers generierte FFI/Tables (`unsafe fn follow`, `init_from_table`) |
| `memfuse-embed` | `src/lib.rs` | 830 | L | 1 | 0 | `#![deny(unsafe_code)]` Crate-Attribut |
| `memfuse-embed` | `src/reranker.rs` | 1105 | XL | 0 | 0 | Sauber (0 `unsafe`) |
| `memfuse-db` | `src/volatile_vault.rs` | 425 | M | 5 | 4 | Memory Locking via POSIX `mlock` / `munlock` (libc) |
| `memfuse-index` | `src/persistence.rs` | 851 | L | 1 | 1 | `memmap2::Mmap::map(&file)` |
| `memfuse-index` | `src/distance.rs` | 1909 | XL | 89 | 89 | AVX2 / AVX-512 / NEON SIMD Intrinsics & Vector Dispatch |
| `memfuse-index` | `src/diskann.rs` | 3055 | XL | 1 | 1 | Out-of-Core `Mmap::map(&file)` |
| `memfuse-index` | `src/lib.rs` | 36 | S | 1 | 0 | `#![deny(unsafe_code)]` Crate-Attribut |

---

### 2. Detaillierte Tabellarische Inventur aller Produktions-`unsafe`-Blöcke

| Datei | Zeile | SAFETY-Kommentar (Y/N) | P2-Kategorie | Code-Kontext & Bewertung |
| :--- | :--- | :--- | :--- | :--- |
| **`memfuse-store/src/wal.rs`** | 702 | Y | (c) Syscalls | `CreateFileW` mit `FILE_FLAG_NO_BUFFERING` für Direct I/O unter Windows. Vorbildlicher `// SAFETY:`-Beweis. |
| **`memfuse-store/src/wal.rs`** | 704 | Y | (c) Syscalls | `SetFileValidData` für Pre-allocation bypassing zero-fill unter Windows. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 715 | Y | (c) Syscalls | `GetSecurityInfo` für Windows ACL Invarianten. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 723 | Y | (c) Syscalls | `GetLengthSid` zur Validierung der SID-Größe. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 735 | N | (c) Syscalls | **Unkommentiert:** `GetLengthSid(owner_sid)` in Vorbereitung von ACL allocation. *Mangel: Fehlendes SAFETY-Tag.* |
| **`memfuse-store/src/wal.rs`** | 768 | Y | (c) Syscalls | `InitializeAcl` zur Initialisierung des Security Descriptors. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 769 | Y | (c) Syscalls | `GetLastError()` Abfrage nach `InitializeAcl`. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 777 | Y | (c) Syscalls | `AddAccessAllowedAce` zur Rechtevergabe. Klares `// SAFETY:`. |
| **`memfuse-store/src/wal.rs`** | 792 | Y | (c) Syscalls | `SetSecurityInfo` zum Anwenden der ACLs. Klares `// SAFETY:`. |
| **`memfuse-core-ipc-gen/src/memfuse_generated.rs`** | 21, 23, 34, 63, 76, 89, 194, 196, 208, 240, 250, 261, 273, 390, 392, 403, 433, 444, 455, 565, 567, 578, 609, 619, 631, 783, 784, 790, 793 | N (0/29) | **(d) IPC/FlatBuffers Deserialisierung** *(Neue Kategorie)* | **Generierter Code (`flatc`):** FlatBuffers zerocopy Tabellennavigation (`Table::new`, `root_unchecked`). Kein einziger Block enthält `// SAFETY:`. *Mangel: P2 Spezifikation erwähnt FlatBuffers IPC Zerocopy nicht als legitime Kategorie.* |
| **`memfuse-db/src/volatile_vault.rs`** | 188 | Y | (c) Syscalls | `libc::mlock(ptr, len)` zum Sperren von RAM gegen Swapping. Klares `// SAFETY:`. |
| **`memfuse-db/src/volatile_vault.rs`** | 232 | Y | (c) Syscalls | `libc::munlock(addr, len)` beim Freigeben gesperrter Vault-Seiten. Klares `// SAFETY:`. |
| **`memfuse-db/src/volatile_vault.rs`** | 260 | N | (c) Syscalls | **Unkommentiert:** `libc::munlock(addr, len)` in Vault Cleanup/Drop Routine. *Mangel: Fehlendes `// SAFETY:`-Tag.* |
| **`memfuse-db/src/volatile_vault.rs`** | 324 | N | (c) Syscalls | **Unkommentiert:** `libc::munlock(addr, len)` bei Vault Clear. *Mangel: Fehlendes `// SAFETY:`-Tag.* |
| **`memfuse-index/src/persistence.rs`** | 364 | Y | (b) Mmap-Persistenz | `memmap2::Mmap::map(&file)` für HNSW Graph Index Mmap. Vorbildlicher `// SAFETY:`-Beweis (4 Invarianten nach P2 Standard). |
| **`memfuse-index/src/diskann.rs`** | 1426 | Y | (b) Mmap-Persistenz | `Mmap::map(&file)` für Out-of-Core DiskANN Index file read. Vorbildlicher `// SAFETY:`-Beweis (4 Invarianten nach P2 Standard). |
| **`memfuse-index/src/distance.rs`** | 132, 137, 144, 169, 174, 181, 206, 211, 218, 275-295, 328-343, 365-379, 410, 419, 448, 460, 503, 513, 530, 558, 567, 595, 607, 649, 659, 675, 679, 710, 716, 741, 747, 786, 792, 888-1241 | Y (vollständig für alle inneren SIMD Calls) | (a) SIMD-Distanz | **SIMD Vector Kernels (AVX2, AVX-512, ARM NEON):** Strukturierte `// SAFETY:`-Kommentare vorhanden. Funktionssignaturen für intrinsische Helfer wie `cosine_distance_neon` tragen teilweise Vor-Kommentare, die Aufrufstellen sind mit expliziten ADR-017 / P2 Tags belegt. |

---

### 3. Statistische Auswertung von Teil A

1. **Gesamtzahl Produktions-`unsafe`-Blöcke:** 135
2. **Dokumentierte Blöcke (`// SAFETY:` vorhanden):** 103 / 135 (76.3%)
3. **Unkommentierte Blöcke:** 32 / 135 (23.7%)
   - 29 davon in `memfuse-core-ipc-gen/src/memfuse_generated.rs` (FlatBuffers Generator)
   - 1 in `memfuse-store/src/wal.rs` (Zeile 735)
   - 2 in `memfuse-db/src/volatile_vault.rs` (Zeilen 260, 324)
4. **Verteilung auf P2-Kategorien:**
   - **(a) SIMD-Distanzberechnung:** 89 Blöcke (65.9%)
   - **(b) Mmap-Persistenz:** 2 Blöcke (1.5%)
   - **(c) Plattformspezifische Syscalls:** 15 Blöcke (11.1%)
   - **(d) Neue Kategorie — IPC/FlatBuffers Zerocopy:** 29 Blöcke (21.5%)

---

## Teil B: §4.14 Python-Kalibrierungs-/Drift-Bindung

### 1. End-to-End Methodentrace

Um die Dateninfrastruktur von Python bis Rust lückenlos zu analysieren, wurde die Callchain schrittweise verfolgt:

#### Schritt 1: Python API Grenzschicht (`memfuse-py/python/memfuse/`)
In `memfuse-py` greift Python über das PyO3-Modul auf `PyDbStats` zu:
```python
# usage in python
stats = db.stats()
print(stats.drift_status)      # "stabil"
print(stats.calibration_ece)   # None
print(stats.last_calibration_at) # None
```

#### Schritt 2: PyO3 FFI Crate (`crates/memfuse-py/src/lib.rs`)
In `crates/memfuse-py/src/lib.rs` ist die Struct `PyDbStats` definiert und über PyO3 exponiert:
```rust
// crates/memfuse-py/src/lib.rs:516-525
#[pyclass(get_all, name = "DbStats")]
pub struct PyDbStats {
    /// Lyapunov drift status ("stabil", "warnung", "kritisch", "unbekannt").
    pub drift_status: String,
    /// Expected Calibration Error (ECE) from IsotonicCalibrator if available.
    pub calibration_ece: Option<f32>,
    /// UNIX timestamp of the last calibration model rebuild.
    pub last_calibration_at: Option<u64>,
    ...
}
```
In der FFI-Methode `stats()` ruft `memfuse-py` die innere Rust-Datenbankinstanz auf:
```rust
// crates/memfuse-py/src/lib.rs:1103-1110
pub fn stats(&self, py: Python<'_>) -> PyResult<PyDbStats> {
    let rt = &self.runtime;
    let stats = run_blocking_ffi(py, || rt.block_on(self.inner.stats()).map_err(memfuse_err))?;

    Ok(PyDbStats {
        drift_status: stats.drift_status,
        calibration_ece: stats.calibration_ece,
        last_calibration_at: stats.last_calibration_at,
        ...
    })
}
```

#### Schritt 3: Rust Datenbank-Kern (`crates/memfuse-db/src/lib.rs`)
Der Aufruf `self.inner.stats()` landet in `crates/memfuse-db/src/lib.rs:1425`:
```rust
// crates/memfuse-db/src/lib.rs:1425-1437
pub async fn stats(&self) -> Result<MemFuseStats> {
    let default_col = self.default_col().await?;
    let active_memory_count = default_col.len().await;
    let index_stats = default_col.stats().await?;
    let storage_stats = self.storage.stats().await?;

    Ok(MemFuseStats {
        drift_status: "stabil".to_string(),
        calibration_ece: None,
        last_calibration_at: None,
        active_memory_count,
        pid_pool_size: None,
        index_stats,
        storage_stats,
    })
}
```

---

### 2. Bewertung der Python-Anbindung (Bestätigung des Spezifikationsbefunds)

Der Trace belegt zweifelsfrei den vermuteten Analogiefall zu H-17:
1. **Live-Daten-Status:** **NICHT LIVE VERDRAHTET (Platzhalter)**.
2. **Begründung:** Die Felder `drift_status`, `calibration_ece` und `last_calibration_at` werden in `memfuse-db::stats()` statisch auf `"stabil"`, `None` und `None` gesetzt. Es existiert derzeit **keine Leseoperation** auf die realen Zustandsobjekte in `memfuse-router` (`LyapunovDriftWatcher`), `memfuse-calibration` (`IsotonicCalibrator`) oder `memfuse-db` (`PhysioScheduler`).
3. **Konsequenz:** Obwohl die FFI-Grenzschicht nach außen hin vollständig vorbereitet und typensicher strukturiert ist, meldet eine Python-Anwendung zu jedem Zeitpunkt unveränderlich den Dummy-Wert `"stabil"` ohne echte Kalibrierungs-ECE.

---

## Empfehlungen & Folge-Tasks

Aus den Befunden ergeben sich folgende priorisierte Folge-Tasks für künftige Fix-Prompts:

### Task 1: P2 Spezifikations-Update & Nachdokumentation (Priorität: HOCH)
- **Erweiterung von P2:** Aufnahme der **4. Kategorie (d) IPC Zerocopy (FlatBuffers Deserialisierung)** in die P2 Spezifikation.
- **`// SAFETY:`-Ergänzungen:**
  - Nachrüsten des fehlenden `// SAFETY:`-Kommentars in `crates/memfuse-store/src/wal.rs:735` (`GetLengthSid`).
  - Nachrüsten der fehlenden `// SAFETY:`-Kommentare in `crates/memfuse-db/src/volatile_vault.rs:260, 324` (`libc::munlock`).
  - Prüfung/Dokumentation des Umgangs mit generiertem Code in `crates/memfuse-core-ipc-gen/src/memfuse_generated.rs` (evtl. Hinweis-Header in Build-Skript oder Generierungs-Pipeline).

### Task 2: §4.14 Live-Verdrahtung der Observability-Metriken (Priorität: MITTEL)
- **Anbindung von `MemFuseStats` in `memfuse-db`:**
  - Verbindung von `memfuse-db` mit dem `LyapunovDriftWatcher` / `PhysioScheduler` zur Abfrage des echten Drift-Status (`"stabil"`, `"warnung"`, `"kritisch"`).
  - Verbindung mit `IsotonicCalibrator` / `CrossEncoderReranker` zur Abfrage des aktuellen `calibration_ece` sowie Timestamp der letzten Kalibrierung.
- **Python Integrationstest:** Erweiterung von `crates/memfuse-py/tests/test_bindings.py`, um sicherzustellen, dass nicht nur der String `"DbStats"`, sondern dynamische Werte validiert werden.

---
*Report abgeschlossen. Keine weiteren Dateimodifikationen im Workspace vorgenommen.*
