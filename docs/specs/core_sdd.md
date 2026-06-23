# SDD Specification: `memfuse-core`

**Status:** DRAFT  
**Crate-Layer:** 0 (Foundation)  
**Souveränität:** 100% Rust, `#![deny(unsafe_code)]`

---

## 1. Systemgrenzen & Verantwortlichkeit (MECE)

`memfuse-core` ist das fundamentale Triebwerk des gesamten Workspace. Es definiert die Sprache, in der alle anderen Komponenten kommunizieren.

### Verantwortlichkeiten:
- **Typ-Definitionen:** Zentrale Newtypes (`DocId`, `TxId`, `EntityId`) zur Vermeidung von Primitive Obsession.
- **Fehler-Souveränität:** Bereitstellung der `MemFuseError` Enum, die alle Fehlerzustände des Systems abbildet.
- **Abstrakte Verträge (Traits):** Definition der `StorageEngine`, `VectorIndex`, `TextIndex` und `GraphIndex` Interfaces.
- **Nebenläufigkeit & Staging:** Sharded `TxBuffer` für lock-freie Transaktions-Vorbereitung.
- **MVCC-Snapshots:** `SnapshotRegistry` zur Verwaltung aktiver Read-Snapshots und Verhinderung von verfrühtem GC.

### Nicht-Verantwortlichkeiten:
- **I/O & Persistenz:** Keine Dateisystem-Interaktion (delegiert an Layer 1).
- **Netzwerk:** Keine Cloud/Network-Abhängigkeiten.
- **Algorithmen:** Keine Implementierung von HNSW oder BM25 Logik.

---

## 2. Kritische Invarianten & SDD-Garantien

| ID | Invariante | Beschreibung |
|---|---|---|
| **CORE-INV-001** | **Zero-Panic** | Alle Funktionen geben `Result` zurück. `.unwrap()` ist außerhalb von Tests verboten. |
| **CORE-INV-002** | **Sharded Concurrency** | `TxBuffer` nutzt 64 Shards (`TxId % 64`), um Lock-Contention bei parallelen Schreibzugriffen zu minimieren. |
| **CORE-INV-003** | **MVCC Protection** | Die `SnapshotRegistry` hält den `min_active_seqno`. I/O Engines MÜSSEN Daten >= `min_active_seqno` vor der Garbage Collection schützen. |
| **CORE-INV-004** | **No-Unsafe** | Strikte Einhaltung von `#![deny(unsafe_code)]`. |

---

## 3. Schnittstellen-Spezifikation (High-Precision)

### 3.1 Error-Interface (`error.rs`)
Dient als zentraler Konverter für alle Subsysteme.
- **Varianten:** `Storage`, `WalCorruption`, `Index`, `Conflict`, `MemoryBudgetExceeded`, etc.
- **Regel:** Neue Varianten werden nur angehängt, um binäre Kompatibilität innerhalb des Releases zu wahren.

### 3.2 Trait-Contracts (`traits.rs`)
Alle Engines müssen diese `#[async_trait]` Schnittstellen implementieren:
- **`StorageEngine`**: CRUD + Atomic Commits + Rollback-to-TX (Crash Recovery).
- **`VectorIndex`**: Search + Insert_Batch + stats + len.
- **`DistanceCalculator`**: Abstraktion für SIMD vs. Skalar Distanzberechnung.

### 3.3 Transaction Buffer (`tx_buffer.rs`)
- **Isolation:** Operations-Staging erfolgt atomar per `TxId`. 
- **Orphan Reaping:** Automatisches Entfernen von Transaktionen nach einem Timeout (Default: 30s) zur Vermeidung von Memory-Leaks.

---

## 4. Speicher-Modell & OOM-Resilienz

- **Fixed Allocation:** `TxBuffer` Shards starten mit Kapazität 16 (`Vec::with_capacity(16)`), um Re-Allokations-Overhead zu reduzieren.
- **Budgeting:** `MemFuseError::MemoryBudgetExceeded` ist vorgesehen für Engine-seitige Limits (Layer 1).

---

## 5. Codebase-Checklist (src/)

| Modul | Status | Bezug auf Spec |
|---|---|---|
| `lib.rs` | ✅ | Zentraler Export & Invarianten-Deklaration. |
| `error.rs` | ✅ | Implementiert CORE-INV-001. |
| `traits.rs` | ✅ | Definiert die 4-Signal-Fusion Schnittstellen. |
| `tx_buffer.rs` | ✅ | Implementiert CORE-INV-002 (Sharding). |
| `snapshot.rs` | ✅ | Implementiert CORE-INV-003 (MVCC). |
| `types.rs` | ✅ | Grundlegende Datenstrukturen. |

---

## 6. Verifikation (Triple-Gate)

- **I - Kompilierbarkeit:** `cargo check -p memfuse-core`
- **II - Stil:** `cargo clippy -p memfuse-core`
- **III - Verhalten:** 
  - `test_tx_buffer_isolation`: Proptest zur Verifizierung der Shard-Isolation.
  - `test_snapshot_registry_basic`: RAII Lifecycle Test für Snapshots.
