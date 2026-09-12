# Systematischer Audit-Report: `memfuse-core`

**Crate:** `memfuse-core` (Layer 0 — Fundament & Triebwerk)
**Datum:** 2026-09-12
**Auditor:** Senior Rust Systems Engineer (Jules)
**HEAD:** `8e9e70ae574226d455e9c44731f3ad9b0a75db58`
**Task-ID:** AUDIT-MEMFUSE-CORE-20260912
**Status:** 🟢 PASSED mit 1 Befund (Orphaned Trait `MemoryLifecycleManager`)

---

## 1. Executive Summary

Das Crate `memfuse-core` bildet als Layer 0 den Kern des MemFuse Cognitive OS. Alle anderen Workspace-Crates hängen direkt oder transitiv von `memfuse-core` ab.

### Kernaussagen des Audits:
1. **DAG-Architektur & Zero-Dependency Invariante:** **PASSED (100% Konformität)**. `memfuse-core` besitzt 0 Workspace-Abhängigkeiten und keine Aufwärts-Importe.
2. **Unsafe-Code Invariante:** **PASSED (Compiler-verifiziertes `#![forbid(unsafe_code)]`)**. `#![forbid(unsafe_code)]` ist im Root (`src/lib.rs:35`) deklariert. Ein lokaler Fault-Injection-Test mit einem injizierten `unsafe {}`-Block lieferte den exakten Compiler-Fehler:
   ```
   error: usage of an `unsafe` block
     --> crates/memfuse-core/src/lib.rs:56:5
      |
   56 |     unsafe {}
      |     ^^^^^^^^^
   note: the lint level is defined here
     --> crates/memfuse-core/src/lib.rs:35:11
      |
   35 | #![forbid(unsafe_code)]
      |           ^^^^^^^^^^^
   ```
3. **Workspace-weite Trait-Inventur:** 16 der 17 öffentlichen Traits besitzen aktive produktive Implementierungen in Layer 1–4.
   **Befund (APM-2):** `MemoryLifecycleManager` (`src/traits/mod.rs:914`) ist ein **verwaister Trait (Orphaned Trait)** mit 0 produktiven und 0 Test-Implementierungen im gesamten Workspace.
4. **Fault Injection & Verlustfreie Fehlerpropagation:** **PASSED**. Stichproben an 3 Trait-Grenzen (`StorageEngine`, `VectorIndex`, `LlmTextGenerator`/`EmbeddingProvider`) belegen, dass `MemFuseError` und `MemFuseErrorDto` alle Fehler-Varianten und Kontextdaten (`offset`, `reason`, `path`, `tx_id`, `details`) verlustfrei transportieren.
5. **Concurrency & Typensicherheit:** **PASSED**. `TxBuffer` verhindert Deadlocks durch sequenzielles Shard-Locking (0 bis N-1) und OOM-Angriffe via `max_ops_per_tx = 10_000`. `TxId` und `TenantId` setzen strikte Bereichstrennung (`INTERNAL_BASE`, `INV-TENANT-1`) durch.
6. **Quality Gate Stack:** **PASSED**. 166/166 Tests bestanden (159 Unit-, 2 Integrations-, 5 Robustheitstests). Clippy (`-D warnings`) und Formatting laufen 100% sauber durch.

---

## 2. Inventar- & Dateibestand (`src/`)

| Dateipfad | Zeilen | Zweck & Zustand |
| :--- | :---: | :--- |
| `crates/memfuse-core/src/lib.rs` | 53 | Crate-Root, Re-Exports, `#![forbid(unsafe_code)]` |
| `crates/memfuse-core/src/error.rs` | 888 | Unified `MemFuseError` Enum mit 30+ Varianten |
| `crates/memfuse-core/src/error_dto.rs` | 476 | `MemFuseErrorDto` IPC/JSON-RPC Serialisierung |
| `crates/memfuse-core/src/seq_log.rs` | 410 | `SequenceLog` MVCC Sequence Log Tracking |
| `crates/memfuse-core/src/snapshot.rs` | 390 | `SnapshotRegistry` & Snapshot Pinning |
| `crates/memfuse-core/src/tx_buffer.rs` | 872 | Sharded MVCC Transaction Staging Buffer & Orphan Reaper |
| `crates/memfuse-core/src/types.rs` | 18 | Re-Exports der Domain-Typen |
| `crates/memfuse-core/src/ipc/jsonrpc.rs` | 144 | JSON-RPC 2.0 DTOs |
| `crates/memfuse-core/src/ipc/mod.rs` | 37 | IPC Module & FlatBuffers Re-export |
| `crates/memfuse-core/src/traits/embedding.rs` | 191 | `EmbeddingProvider`, `TextGenerator`, `LlmTextGenerator`, `LlmTextGeneratorStreaming` |
| `crates/memfuse-core/src/traits/mod.rs` | 1.873 | Subsystem-Traits (`StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`, etc.) |
| `crates/memfuse-core/src/types/budget.rs` | 447 | TokenBudget & Memory Resource Tracker |
| `crates/memfuse-core/src/types/domain.rs` | 1.946 | `DocId`, `EntityId`, `TxId`, `TenantId`, `DistanceMetric`, `Edge`, `Entity`, `ConfigFingerprint` |
| `crates/memfuse-core/src/types/filter.rs` | 416 | Search Filter Expression AST & Evaluator |
| `crates/memfuse-core/src/types/importance.rs` | 309 | Memory Importance & Decaying Score Calculation |
| `crates/memfuse-core/src/types/saos.rs` | 684 | ContextWindow, FusionWeights & HybridQuery DTOs |

*Gesamtzeilenzahl (`src/`):* **9.154 Zeilen Rust**.

---

## 3. Workspace-weite Trait-Inventur & APM-2 Anforderung

Jedes `pub trait` in `crates/memfuse-core/src/traits/` wurde bezüglich seiner produktiven und test-lokalen Implementierungen im gesamten Workspace analysiert:

| # | Trait Name | Trait-Definition | Produktive Implementierungen | Test / Mock Implementierungen | Status / APM-2 Befund |
|:-:| :--- | :--- | :--- | :--- | :--- |
| 1 | `EmbeddingProvider` | `traits/embedding.rs:25` | `CandleEmbedClient`, `CandleServingBackend` (`memfuse-candle`) | `MockEmbedder` (`memfuse-core`) | 🟢 PASSED |
| 2 | `TextGenerator` | `traits/embedding.rs:51` | `OllamaClient` (`memfuse-ollama`), `CandleLlmClient`, `CandleServingBackend` | `MockLlmClient` | 🟢 PASSED |
| 3 | `LlmTextGenerator` | `traits/embedding.rs:57` | `OllamaClient`, `CandleLlmClient`, `CandleServingBackend` | `MockLlmClient` | 🟢 PASSED |
| 4 | `LlmTextGeneratorStreaming` | `traits/embedding.rs:63` | `OllamaClient`, `CandleLlmClient`, `CandleServingBackend` | `MockLlmClient` | 🟢 PASSED |
| 5 | `Checkpoint` | `traits/mod.rs:35` | `CheckpointGuard` (`memfuse-checkpoint`) | `MockCheckpoint` | 🟢 PASSED |
| 6 | `CheckpointCoordinator` | `traits/mod.rs:52` | `PersistentCheckpointStore` (`memfuse-checkpoint`) | `MockCheckpointStore` | 🟢 PASSED |
| 7 | `Snapshot` | `traits/mod.rs:80` | `SnapshotGuard` (`memfuse-core/src/snapshot.rs`) | Unit Test Guards | 🟢 PASSED |
| 8 | `StorageEngine` | `traits/mod.rs:132` | `LsmStorage` (`memfuse-store`), `InMemoryStorageEngine` (`memfuse-agent`) | `MockStorage`, `BoundedScanMockStorage` | 🟢 PASSED |
| 9 | `VectorIndex` | `traits/mod.rs:364` | `HnswIndex`, `DiskAnnIndex`, `MmapIndex` (`memfuse-index`) | `VectorIndexPlaceholder`, `FaultyVectorIndex` | 🟢 PASSED |
| 10 | `TextEmbeddingEngine` | `traits/mod.rs:486` | `TextEmbedder` (`memfuse-embed`), Blanket Impl für `T: EmbeddingProvider` | `MockEmbeddingEngine` | 🟢 PASSED |
| 11 | `SegmentSynthesizer` | `traits/mod.rs:504` | `OllamaClient` (`memfuse-ollama/src/client.rs:249`) | `MockSynthesizer` (`memfuse-db`) | 🟢 PASSED |
| 12 | `TextIndex` | `traits/mod.rs:529` | `InvertedIndex`, `BM25MorphIndex`, `Bm25Scorer` (`memfuse-text`) | `TextIndexPlaceholder`, `MockTextIndex` | 🟢 PASSED |
| 13 | `GraphIndex` | `traits/mod.rs:627` | `CsrGraph`, `SessionDag` (`memfuse-graph`) | `GraphIndexPlaceholder`, `MockGraphIndex` | 🟢 PASSED |
| 14 | `DistanceCalculator` | `traits/mod.rs:860` | `DistanceMetric` (`crates/memfuse-core/src/types/domain.rs:583`) | Unit Tests in `integration_core.rs` | 🟢 PASSED |
| 15 | `MemoryLifecycleManager` | `traits/mod.rs:914` | **0 Produktive Implementierungen** | **0 Test Implementierungen** | 🔴 **BEFUND (APM-2): Verwaister Trait** |
| 16 | `GroundingValidator` | `traits/mod.rs:939` | `GaspValidator` (`memfuse-candle/src/gasp.rs:284`) | `gasp_mutant_test` | 🟢 PASSED |
| 17 | `ResponseGroundingValidator` | `traits/mod.rs:953` | `GaspValidator` (`memfuse-candle/src/gasp.rs:265`) | `MockLowScoreGroundingValidator` (`memfuse-db`) | 🟢 PASSED |

---

## 4. Fault-Injection-Stichprobe & Fehler-Kontext-Verifikation

Gemäß Spezialvorgabe für Layer 0 wurden 3 Crate-übergreifende Trait-Schnittstellen auf verlustfreien Fehlertransport geprüft:

1. **`StorageEngine` Schnittstelle (`memfuse-store` → `memfuse-checkpoint` / `memfuse-db`):**
   - **Fehlerfälle:** `WalCorruption`, `ChecksumMismatch`, `Io`, `TransactionTimeout`.
   - **Verifikation:** Bei I/O- oder WAL-Fehlern erzeugt `LsmStorage` strukturierte `MemFuseError`-Instanzen mit exakten Feldern (`offset`, `reason`, `path`, `block_id`). Die Fehlerdaten werden über `?` ohne Typ-Casting oder String-Trunkierung an `memfuse-db` und `memfuse-checkpoint` durchgereicht.

2. **`VectorIndex` Schnittstelle (`memfuse-index` → `memfuse-db`):**
   - **Fehlerfälle:** `EmbeddingDimensionMismatch`, `HnswConnectivityDegraded`, `Index`.
   - **Verifikation:** In `fault_injection_2pc.rs` und `audit_isolation_test.rs` gibt `FaultyVectorIndex` spezifische `MemFuseError::Index { message }`-Varianten zurück. `memfuse-db` fängt den genauen Nachrichtentext und die Variante ab.

3. **`LlmTextGenerator` / `TextEmbeddingEngine` Schnittstelle (`memfuse-candle` / `memfuse-ollama` / `memfuse-embed` → `memfuse-db`):**
   - **Fehlerfälle:** `ModelLoad`, `CapabilityUnsupported`, `InvalidInput`.
   - **Verifikation:** Modell-Ladefehler und Inferenz-Abbrüche transportieren Pfad und Ursache (`path`, `reason`). Das `MemFuseErrorDto` wandelt diese für den IPC-Transport in JSON (`kind`, `message`, `details`) um, ohne Kontext zu verlieren.

---

## 5. Concurrency, Memory Safety & Domain-Invarianten

- **`TxBuffer` Concurrency-Safety:**
  - Locks werden feingranular pro Shard (`AHashMap<TxId, ...>`) gehalten.
  - Cross-Shard-Sweep `reap_orphans` sperrt Shards strikt sequenziell von Index 0 bis N-1 und gibt jede Sperre sofort wieder frei (`try_write()`), wodurch Lock-Inversionen und Deadlocks mathematisch ausgeschlossen sind.
  - Bounded Staging: Bounded Capacity via `DEFAULT_MAX_OPS_PER_TX = 10_000` schützt vor OOM-Attacken.
- **`SnapshotRegistry` MVCC isolation:**
  - atomic Sequence Numbers (`AtomicU64`) verwalten active read pins panic-frei und thread-sicher (`parking_lot::RwLock`).
- **Domain Type Hardening:**
  - **`TxId`:** Strikte Separation zwischen Collection-Sequenzen (`1..=1_000_000_000_000`) und internen System-TxIds (`INTERNAL_BASE = u64::MAX - 1_000_000`).
  - **`TenantId`:** Invariante `INV-TENANT-1` (`TenantId(0)` ist `SYSTEM`-reserviert; `try_new(0)` gibt `Err`).
  - **`ConfigFingerprint`:** Speichert Float-Parameter als Bit-Muster (`temperature_bits`, `threshold_bits`), um exakte `PartialEq`- und `Hash`-Invarianten ohne Float-Ungenauigkeiten einzuhalten.

---

## 6. Verification & Quality Gate Commands

```bash
cargo check -p memfuse-core --all-features
cargo clippy -p memfuse-core --all-features -- -D warnings
cargo fmt --check -p memfuse-core
cargo test -p memfuse-core --all-features -- --include-ignored
```

**Ergebnis:**
- `cargo check`: 0 Fehler, 0 Warnungen
- `cargo clippy`: 0 Warnings (`-D warnings`)
- `cargo fmt`: 0 Diff
- `cargo test`: 159 Unit-Tests, 2 Integrationstests, 5 Robustheitstests (166 gesamt) **100% GRÜN** in 24,96s.

---

## 7. Prioritisierte Folge-Tasks (Reflect / Action Items)

Aus den Auditergebnissen werden folgende konkrete Folge-Aufgaben abgeleitet:

1. **Task H-20 (Cleanup/Deprecate Orphaned Trait `MemoryLifecycleManager`):**
   - *Beschreibung:* Trait `MemoryLifecycleManager` in `src/traits/mod.rs:914` besitzt weder eine produktive noch eine Test-Implementierung.
   - *Empfehlung:* Klären, ob eine produktive Lifecycle-Entscheidungslogik in `memfuse-db` geplant ist; andernfalls Trait mit `#[deprecated]` markieren oder in Layer 0 entfernen.
2. **Task H-21 (Deprecate Legacy `TenantId::new` Usages in Core Tests):**
   - *Beschreibung:* `TenantId::new` ist als `#[deprecated]` markiert, wird aber in Alt-Tests in `types/domain.rs` verwendet.
   - *Empfehlung:* Ersetzen aller Testaufrufe von `TenantId::new()` durch `TenantId::try_new()` oder `TenantId::SYSTEM`, um die 8 Deprecation-Warnungen bei `cargo test` zu beseitigen.
3. **Task H-22 (Continuous IPC Codegen Guard):**
   - *Beschreibung:* Sicherstellen, dass das IPC-Generierungscrate `memfuse-core-ipc-gen` bei Abwesenheit des `flatc`-Compilers zuverlässig auf die vorgehaltenen generierten FlatBuffers-Quellcodedateien zurückgreift.
