# MemFuse — Kritische Architektur- & Designbewertung

> **Analyse-Basis:** Repository `https://github.com/tfufuz1/memfuse` (Stand: 2026-09-04)  
> **Rust-Version:** 1.89 (Workspace)  
> **Crates gesamt:** 15 + xtask  
> **Schweregrade:** 🔴 Kritisch · 🟠 Hoch · 🟡 Mittel · 🔵 Niedrig

---

## Zusammenfassung

MemFuse zeigt eine klare, gut dokumentierte Schichtenarchitektur (Layer 0–3) und eine Reihe echter technischer Stärken: HMAC-gekettetem WAL, SQ8-Quantisierung, shardierter MemTable und einem sauberen RAII-Checkpoint-Guard. Die Qualität der Tests ist bemerkenswert hoch.

Gleichzeitig existieren mehrere **strukturelle Brüche**, die die Langzeitwartbarkeit und Korrektheit des Systems gefährden:

1. Ein **komplett kaputtes Decision-Routing** im Agent-Engine (Code vorhanden, wird nie ausgewertet)  
2. **Embedding-Duplikation** — alle Vektoren werden sowohl im HNSW-Index als auch im LSM-Tree als JSON gespeichert  
3. **`async_trait`-Overhead** auf einem Toolchain-Stand, der native Async-Traits vollständig unterstützt  
4. Dutzende **nicht-workspace-verwalteter Abhängigkeiten** in mehreren Crates  
5. Aktiver Code hinter `#[deprecated]`-Flags in einem Produktions-Crate  

Die folgende Analyse behandelt jedes Crate einzeln und schließt mit crate-übergreifenden Problemen.

---

## 1. `memfuse-core` — Fundament (Layer 0)

### Stärken

- Einheitliche `MemFuseError`-Enum mit `#[non_exhaustive]`, systematischen Konstruktor-Helpers und vollständiger Testabdeckung für alle Display-Formate
- `CapabilityUnsupported`-Pattern für optionale Subsystem-Features: klar, erweiterbar, typsicher
- Kommentierte Dyn-Safety-Tests (`_assert_dyn_*`) – gute Dokumentation der vtable-Kompatibilitäts-Anforderungen
- `TxBuffer` und `SnapshotRegistry` als saubere Cross-Cutting-Concerns im Fundament

### Kritische Befunde

**🔴 `last_tx_id()` gibt inkonsistente Typen zurück**

`StorageEngine::last_tx_id()` gibt `Result<TxId>` zurück, während `VectorIndex`, `TextIndex` und `GraphIndex` `Result<u64>` zurückgeben. Dies erzwingt an jeder Verwendungsstelle manuelles `.0`-Unwrapping oder eine versehentliche semantische Verwechslung zwischen einer rohen Sequenznummer und einer Transaktions-ID:

```
traits.rs:192  StorageEngine::last_tx_id -> Result<TxId>    ✅
traits.rs:318  VectorIndex::last_tx_id   -> Result<u64>     ❌
traits.rs:411  TextIndex::last_tx_id     -> Result<u64>     ❌
traits.rs:629  GraphIndex::last_tx_id    -> Result<u64>      ❌
```

**Fix:** `VectorIndex`, `TextIndex` und `GraphIndex` auf `Result<TxId>` vereinheitlichen.

---

**🟠 `async_trait` auf Rust 1.89 — veraltetes Pattern mit realen Kosten**

Alle vier Kern-Traits (`StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`) sowie alle weiteren Traits verwenden `#[async_trait]`. Das Macro wandelt jede `async fn` in `Pin<Box<dyn Future<...>>>` um — eine Heap-Allokation pro Aufruf. Rust hat native Async-in-Traits seit 1.75 stabilisiert. Das Projekt zielt auf 1.89 — `async_trait` ist damit für nicht-dyn-kompatible Traits vollständig unnötig.

Selbst wo `dyn Trait` benötigt wird (z. B. `Arc<dyn StorageEngine>`), kann `trait_variant` oder ein manueller Wrapper eingesetzt werden, statt jede Methode zu boxen.

Betroffene Crates: `memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-graph`, `memfuse-checkpoint`, `memfuse-agent` (13 Cargo.toml-Einträge).

---

**🟠 `scan_prefix` / `scan` materialisieren vollständig in den Heap**

```rust
async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
```

Bei einem Collection-Delete mit 50.000 Einträgen lädt diese Signatur **alle** Key-Value-Paare in einen einzigen Vektor. Für große Collections ist dies nicht nur langsam, sondern kann den Memory-Budget überschreiten. Kein Streaming, kein Cursor, keine Pagination.

**Fix:** Die Trait-Signatur sollte einen `Stream`-basierten Rückgabetyp oder zumindest ein `limit`-/`cursor`-Argument vorsehen. Als Übergangsmaßnahme: `scan_prefix_with_limit(prefix, limit, offset)`.

---

**🟡 Zwei parallele Checkpoint-Abstraktionen ohne klare Komposition**

Das `Checkpoint`-Trait (`take_snapshot`, `restore`) und das `CheckpointCoordinator`-Trait (mit `type Meta`) existieren nebeneinander, ohne sich gegenseitig zu referenzieren oder zu verlängern. Downstream-Code (memfuse-checkpoint) implementiert keines der beiden Traits direkt — es implementiert stattdessen ein eigenes `CheckpointRegistry`-Trait. Dies erzeugt drei parallele Checkpoint-Abstraktionen in einem System, das konzeptionell eine braucht.

**Fix:** Eines von `Checkpoint`/`CheckpointCoordinator` als primäre Abstraktion wählen; das andere als Deprecated markieren oder als Convenience-Methoden im primären Trait integrieren.

---

**🟡 `multi_traverse()` verwendet `std::collections::HashMap` statt `AHashMap`**

Die gesamte Codebasis nutzt `ahash` als Workspace-Dependency und `AHashMap` überall für Performance. Die Default-Implementierung von `multi_traverse()` in `traits.rs` verwendet jedoch `std::collections::HashMap` — ein inkonsistentes Detail, das genau in der heißesten Traversal-Methode auftritt.

---

**🔵 `VectorIndex::len()` ist async ohne Grund**

```rust
async fn len(&self) -> usize;
async fn is_empty(&self) -> bool { self.len().await == 0 }
```

Die Länge eines Index wird intern durch eine atomare Variable gehalten (z. B. `AtomicUsize`). Async-Overhead für eine nicht-blockierende, nicht-I/O-gebundene Operation ist regressiv. Die Methode sollte `fn len(&self) -> usize` sein. Das gleiche gilt für `TextIndex::len()` und `GraphIndex::len()`.

---

## 2. `memfuse-store` — LSM-Tree-Storage-Engine (Layer 1)

### Stärken

- HMAC-Chaining über WAL-Einträge mit klar dokumentierten V1/V2/V3-Versionen
- Shardierte MemTable (16 Shards, BLAKE3-basiertes Routing) für hohen Schreibdurchsatz
- Dokumentierte Lock-Hierarchie (`commit_mutex` → `state` → `sstables`) mit explizitem Deadlock-Schutz
- Atomares Salt-File-Schreiben via tmp-Datei → fsync → rename-Pattern

### Kritische Befunde

**🟠 `FLUSH_COUNTER` als globales Atomic — Testpollution und Multi-Instanz-Konflikt**

```rust
static FLUSH_COUNTER: AtomicU64 = AtomicU64::new(0);
```

Dieser globale Zähler ist prozessweit geteilt. In parallelen Tests (Tokio `#[test]`) führt das zu nicht-deterministischen WAL-Dateinamen und Zählerständen, die zwischen Tests überlaufen. In Produktionsumgebungen mit mehreren `LsmStorage`-Instanzen (z. B. mehrere Collections auf einem Prozess) teilen sich alle Instanzen denselben Namespacing-Kontext.

**Fix:** `FLUSH_COUNTER` als Instanzvariable (`AtomicU64`) in `LsmStorage` verankern.

---

**🟠 `LEGACY_INTEGRITY_KEY` — menschenlesbarer Hard-Coded-Key im Binary**

```rust
pub(crate) const LEGACY_INTEGRITY_KEY: [u8; 32] = *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";
```

Dieser Schlüssel ist im Klartext in das Binary eingebettet. Jeder mit Zugriff auf die kompilierte Binary (auch mit `strings`) kann ihn extrahieren und WAL-Daten von Legacy-Datenbanken fälschen, die noch V1-Format verwenden. Die Kommentare behaupten, er werde nur für Legacy-Replay verwendet — aber das schützt nicht vor HMAC-Fälschung auf diesen Dateien.

**Fix:** Den Key als nicht-ASCII-Bytearray speichern; einen Migrationspfad implementieren, der alle V1-WAL-Dateien beim ersten Öffnen in V3 konvertiert und danach Legacy-Replay endgültig deaktiviert.

---

**🟠 Nicht-workspace-verwaltete Abhängigkeiten**

Folgende Crates werden direkt in `memfuse-store/Cargo.toml` versioniert, ohne in `[workspace.dependencies]` zu erscheinen:

| Dependency | Version in Store |
|---|---|
| `lru` | `0.16.3` |
| `uuid` | `1` (features: `v4`) |
| `tokio-util` | `0.7.18` |

`tokio-util` erscheint in vier Crates (`store`, `agent`, `db`, `mcp`) mit unterschiedlichen Feature-Sets und teilweise unterschiedlichen Versionsnummern (`0.7` vs. `0.7.18`). Das ist ein Cargo-Rezept für subtile Feature-Unification-Konflikte.

**Fix:** Alle drei in `[workspace.dependencies]` aufnehmen; `uuid` kann durch `rand` (bereits Workspace-Dep) ersetzt werden, da UUID nur für WAL-Dateinamen verwendet wird.

---

**🟡 WAL V1 (kein HMAC) bleibt lesbar — Integrity-Lücke**

`WalVersion::V1` ist markiert als "Legacy: kein HMAC". Beim WAL-Replay wird V1 ohne Integritätsprüfung akzeptiert. Jede Datei, die sich als V1 ausgibt (z. B. durch Überschreiben des Magic Headers), umgeht damit die gesamte HMAC-Schutzschicht.

**Fix:** V1-Dateien beim Öffnen sofort in V3 upgraden; Öffnen von V1-Dateien ab einer konfigurierbaren Version verweigern (`min_wal_version`-Konfigurationsparameter).

---

## 3. `memfuse-index` — Vektor-Index (Layer 1)

### Stärken

- HNSW mit Soft-Delete-Tombstones und automatischem Hintergrund-Rebuild via atomarem Arc-Swap
- SQ8-Skalarer Quantisierer mit Mmap-basierter Persistenz
- Stabile SIMD-Distanzberechnung (kein `#[feature(portable_simd)]`)
- DiskANN hinter Feature-Flag für experimentelle Features

### Kritische Befunde

**🔴 `experimental-diskann` ist in `default`-Features**

```toml
[features]
default = ["experimental-diskann"]
experimental-diskann = []
```

Ein Feature, das explizit als "experimental" benannt ist, ist standardmäßig aktiviert. Das widerspricht dem Zweck des Feature-Flags und bedeutet, dass jeder, der `memfuse-index` als Dependency einfügt, ohne explizite `default-features = false` DiskANN mitbekommt.

**Fix:** `default = []` setzen. DiskANN muss opt-in bleiben.

---

**🟠 Gemischte Sync/Async-Locks in derselben Struct — potenzielle Deadlocks**

`HnswIndex` verwendet gleichzeitig:
- `parking_lot::RwLock` für lesenden Graph-Zugriff (Nodes, Doc-To-Node-Map)
- `tokio::sync::Mutex` als `write_mutex` für exklusive Mutationen/Rebuilds

Das Halten eines `parking_lot::RwLock` über einen `.await`-Punkt blockiert den Tokio-Thread, weil `parking_lot`-Locks nicht async-aware sind. Wenn ein `await` innerhalb eines `parking_lot`-Lock-Scopes aufgerufen wird (z. B. bei Logging oder beim Aufruf einer anderen async-Methode), blockiert das die gesamte Tokio-Task-Queue auf diesem Thread.

**Fix:** Konsequent entweder `tokio::sync::RwLock` für alle Index-Locks verwenden, oder alle async-kritischen Operationen so umstrukturieren, dass kein `parking_lot`-Lock über ein `await` gehalten wird. Eine klare Regel im AGENTS.md dokumentieren.

---

**🟡 Rebuild ohne Backpressure oder Cancellation**

`trigger_rebuild_async()` spawnt einen neuen Tokio-Task für den Hintergrund-Rebuild, ohne:
- Zu prüfen, ob bereits ein Rebuild läuft
- Einen Cancellation-Token zu akzeptieren
- Den Task-Handle zu behalten (d. h. keine `JoinHandle`-Verwaltung)

Bei lösch-intensiven Workloads können sich Rebuilds aufstapeln. Jeder Rebuild kopiert den gesamten Index in eine neue Arc, was kurzzeitig den doppelten Speicher benötigt.

**Fix:** Einen `AtomicBool is_rebuilding`-Guard einführen; `trigger_rebuild_async()` wird zur No-Op, wenn bereits ein Rebuild läuft.

---

## 4. `memfuse-db` — Orchestrator-Facade (Layer 2)

### Stärken

- Klare Trennung zwischen `Collection` und `MemFuse`-Facade
- Prefix-basierte Namespace-Isolation zwischen Collections
- Hybrid-Search-Fusion (HNSW + BM25 + Graph + Importance) als RRF-Implementierung
- `QueryBuilder`-Fluent-API für Suche

### Kritische Befunde

**🔴 Embedding-Duplikation: Vektoren im HNSW UND im LSM als JSON**

Jede `insert()`-Operation speichert den Embedding-Vektor **zweimal**:

1. Im `HnswIndex` (als `Vec<f32>` im RAM mit Disk-Persistenz über `HnswIndex::save()`)
2. Im `LsmStorage` als `StoredDocument { embedding: Vec<f32>, ... }` — JSON-serialisiert

Bei einem 1536-dimensionalen Embedding:
- HNSW-Speicher: ~6 KB (f32-Rohdaten + Graphkanten)
- LSM-Speicher: ~12 KB (JSON-Overhead für Vec<f32>)

Bei 100.000 Dokumenten bedeutet das ~1,2 GB LSM-Speicher allein für Embedding-Spiegelung. Bei `memfuse-embed` mit `MAX_EMBED_BATCH_SIZE = 10.000` und 1536D: ~60 MB nur für eine Batch-Serialisierung.

**Fix:** `StoredDocument` von `StoredDocumentMeta` trennen und die `embedding`-Komponente ausschließlich im Index belassen. Der LSM speichert nur Metadaten; Vektoren werden bei Bedarf aus dem Index abgerufen.

---

**🟠 `#[allow(deprecated)]` auf Crate-Ebene in der aktiven Codebasis**

```rust
// lib.rs, Zeile 6:
#![allow(deprecated)]
```

Dieses Attribut überdeckt alle Verwendungen veralteter APIs im gesamten Crate. Neue Code-Hinzufügungen können versehentlich auf deprecated APIs bauen, ohne Warnung zu erhalten. Damit ist der Deprecation-Mechanismus im Haupt-Orchestrator-Crate effektiv deaktiviert.

**Fix:** Das `allow(deprecated)` entfernen; alle deprecated Aufrufe explizit mit `#[allow(deprecated)]` am Aufrufort markieren und mit einem Tracking-Issue versehen.

---

**🟡 `parse_importance_score()` — fragile Heuristik am falschen Ort**

Die Funktion parst einen LLM-Antwort-String nach einem Float-Score, indem sie Token für Token iteriert und auf `parse::<f32>()` prüft. Sie ist in `collection/mod.rs` definiert und von dort global verwendbar. Parsing-Heuristiken für LLM-Output gehören in ein dediziertes `memfuse-ollama`- oder `memfuse-text`-Modul, nicht in den Collection-Layer.

---

**🟡 `Collection<LsmStorage>` — generischer Parameter ohne echte Generizität**

`Collection<S: StorageEngine>` ist generisch, wird aber überall konkret als `Collection<LsmStorage>` instanziiert. In `memfuse-router`, `memfuse-agent`, und Teilen von `memfuse-tauri` ist `Arc<Collection<LsmStorage>>` der übergebene Typ — die Generizität hilft Nutzern nicht, aber erschwert das Refactoring und Testing (Mock-Storage wird durch konkrete Parameterisierung verhindert).

**Fix:** Entweder den generischen Parameter beibehalten und konsequent in Tests mit Mock-Storage nutzen, oder einen Type-Alias `type DefaultCollection = Collection<LsmStorage>` einführen und Interfaces auf `Arc<dyn StorageEngine>` anpassen.

---

## 5. `memfuse-checkpoint` — Snapshot-Verwaltung

### Stärken

- RAII `CheckpointGuard` mit automatischem `rollback_to_tx` bei Drop — korrekte Panic-Sicherheit
- `PersistentCheckpointStore` mit In-Memory-Cache + LSM-Persistenz

### Kritische Befunde

**🔴 Tote Deprecated-Globals aktiv im Produktionscode**

Das Crate enthält **vier** als `#[deprecated]` markierte globale Strukturen/Funktionen, die aber noch aktiv im Code definiert und durch `#[allow(deprecated)]` weiter zugänglich gehalten werden:

```rust
static ORPHANED_CHECKPOINTS: Mutex<Vec<StateCheckpoint>>
pub static ORPHAN_REGISTRY: OnceLock<OrphanRegistry>
pub fn global_orphan_registry() -> &'static OrphanRegistry
pub struct OrphanRegistry
```

Das ist kein "geplanter Abbau" — die Deprecation-Markierungen referenzieren `InstanceOrphanRegistry` (ADR-053), aber es gibt kein Tracking-Issue, keine Timeline und keinen sichtbaren Migrations-Fortschritt. Die globalen Statics mit `Mutex<Vec<...>>` sind zudem eine ernstzunehmende Testpollution-Quelle in parallelen Testläufen.

**Fix:** Sämtliche `ORPHANED_*` und `ORPHAN_REGISTRY`-Konstrukte entfernen. Einen konkreten ADR-053-Migrationsplan mit Deadline einführen.

---

**🟠 `orphan_pin_file_path()` schreibt in das aktuelle Arbeitsverzeichnis**

```rust
fn orphan_pin_file_path() -> PathBuf {
    std::env::var("MEMFUSE_ORPHAN_PIN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("memfuse_orphaned_pins.json"))
}
```

Eine Bibliothek, die ohne explizite Konfiguration Dateien im CWD anlegt, ist ein ernstes API-Designproblem. In Serverumgebungen ist CWD oft `/`, `/proc` oder ein Verzeichnis ohne Schreibrechte.

**Fix:** Dateipfad als Pflichtparameter im Konstruktor erzwingen; kein Fallback auf CWD.

---

**🟠 `CHECKPOINT_COUNTER` und `SKIPPED_ROLLBACKS` als globale Statics**

```rust
static CHECKPOINT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SKIPPED_ROLLBACKS: AtomicU64 = AtomicU64::new(0);
```

Diese globalen Metriken akkumulieren über alle Instanzen und Tests hinweg. Tests können sich nicht auf konsistente Startwerte verlassen; Monitoring-Code, der diese Werte liest, erhält aggregierte Werte ohne Instanz-Kontext.

**Fix:** Als Instanzvariablen in `PersistentCheckpointStore` verankern.

---

**🔵 `PinId = u64` — kein Newtype-Schutz**

```rust
pub type PinId = u64;
```

`PinId` ist nur ein Typ-Alias. Er kann versehentlich mit `TxId`, `seq_no` oder anderen `u64`-Werten verwechselt werden. Da `TxId` im Core als `#[repr(transparent)]`-Newtype definiert ist, sollte `PinId` dasselbe sein.

---

## 6. `memfuse-crypto` — Kryptographischer Kernel (Layer 1)

### Stärken

- AES-256-GCM-SIV mit nonce-misuse-resistance (RFC 8452)
- HKDF-SHA256 für per-Datei-Schlüsselableitung
- `VolatileEncryptionKey` mit Zeroize-on-Drop
- Lock-frei und I/O-frei — korrekte Isolation

### Befunde

**🟠 `#[cfg_attr(not(test), forbid(unsafe_code))]` — Unsafe im Test-Code undokumentiert**

```rust
#![cfg_attr(not(test), forbid(unsafe_code))]
```

Das erlaubt `unsafe` in Test-Modulen ohne explizite Dokumentation, warum das notwendig ist. Test-Helpers für Krypto-Code sollten besonders sorgfältig sein.

**Fix:** Entweder `forbid(unsafe_code)` durchgehend erzwingen, oder spezifisch dokumentieren, welcher Test-Code Unsafe benötigt und warum.

---

**🟡 Kopplung an `memfuse-core` nur für Fehlertypen**

`memfuse-crypto` importiert `memfuse-core` ausschließlich für `MemFuseError` und `Result`. Das schafft eine Abhängigkeit des Krypto-Kernels auf den Orchestrator-Layer, was die Isolationseigenschaft abschwächt. Im Fehlerfall (z. B. Compile-Fehler in core) ist crypto blockiert.

**Fix:** `memfuse-crypto` bekommt einen eigenen `CryptoError`-Typ; die Konvertierung zu `MemFuseError` erfolgt im Call-Layer via `From`.

---

## 7. `memfuse-graph` — CSR-Graph (Layer 1)

### Stärken

- CSR (Compressed Sparse Row) für speichereffiziente Graph-Repräsentation
- Personalized PageRank (PPR) als vollständige Power-Iteration-Implementierung
- Community Detection (Louvain/Modularity) als eigenständiges Modul
- `SessionBranchTree` für Konversations-Branching

### Befunde

**🟡 Lock-Reihenfolge in `SessionBranchTree` nur dokumentarisch erzwungen**

Die Dokumentation schreibt vor: `nodes` muss vor `edges` oder `active_head` gesperrt werden. Der Compiler erzwingt dies nicht. Eine versehentliche Umkehrung der Reihenfolge in einer neuen Methode führt zu einem nicht-reproduzierbaren Deadlock.

**Fix:** Das Newtype-Pattern `NodeGuard<'a>` einführen, das erst nach dem Erwerb des `nodes`-Locks instanziiert werden kann und den Lock auf `edges` als Methode anbietet — Lock-Ordering wird so durch den Typ erzwungen.

---

**🟡 CSR-Rebuild bei jedem Commit — Inkonsistenz-Fenster**

CSR ist eine read-optimierte Datenstruktur. Kanten-Einfügungen gehen in einen "Pending-Edges"-Puffer und werden erst beim Commit in die CSR-Repräsentation übernommen. Zwischen dem Einfügen einer Kante und dem Commit existiert ein Fenster, in dem Traversal-Anfragen die neue Kante nicht sehen — das ist beabsichtigt, aber nirgendwo als explizite MVCC-Semantik dokumentiert. Für Multi-Hop-Traversal über mehrere unfertige Commits kann das zu inkonsistenten Pfadergebnissen führen.

---

## 8. `memfuse-text` — BM25/Inverted-Index (Layer 1)

### Stärken

- Deutsche Morphologie (Compound-Splitting, Umlaut-Normalisierung) gut abstrahiert
- `StorageEngine`-basierter `InvertedIndex` — persistenz-agnostisch

### Befunde

**🟡 `Bm25Scorer<S>` ist reiner Delegation-Wrapper ohne Mehrwert**

```rust
pub struct Bm25Scorer<S: StorageEngine> {
    index: InvertedIndex<S>,
}
// Alle Methoden delegieren 1:1 an self.index
```

`InvertedIndex<S>` könnte `TextIndex` direkt implementieren. Der `Bm25Scorer`-Wrapper fügt keine Logik hinzu, führt aber einen zusätzlichen Indirektionstyp ein, den alle Downstream-Crates kennen müssen.

**Fix:** `TextIndex` direkt auf `InvertedIndex<S>` implementieren. `Bm25Scorer` entweder entfernen oder für echte BM25-Parameterisierung (`k1`, `b`) erweitern.

---

**🔵 Sprachspezifische Morphologie ohne Feature-Flag immer kompiliert**

`GermanCompoundSplitter`, `MorphologicalTokenizer`, `normalize_umlauts` werden immer kompiliert, auch wenn das System auf Englisch oder einer anderen Sprache läuft. Für ein Embedded-System ist unnötiger kompilierter Code ein echtes Problem.

**Fix:** `[features] german-morphology` einführen; `GermanCompoundSplitter` und `MorphologicalTokenizer` dahinter gaten.

---

## 9. `memfuse-agent` — Workflow-Engine (Layer 3)

### Stärken

- Klarer `checkpoint → execute → commit → audit`-Loop mit RAII-Schutz
- Token-Budget mit Pre-Reserve und Post-Reconciliation — korrekte Semantik
- Gut dokumentierte Zustandsmaschine und Crash-Recovery-Semantik

### Kritische Befunde

**🔴 `NodeType::Decision` wertet Bedingungen nie aus — Dead Code**

`WorkflowEdge` hat ein `condition: Option<String>`-Feld. `evaluate_decision()` ignoriert dieses Feld vollständig:

```rust
fn evaluate_decision(&self, graph: &StateGraph, node: &AgentNode, _ctx: &AgentContext) -> Result<String> {
    let edges = graph.edges.iter().filter(|e| e.from == node.id).collect::<Vec<_>>();
    // Wählt einfach die Kante mit höchster Priorität — kein Condition-Checking!
    let edge = edges.iter().max_by_key(|e| e.priority)...
    Ok(edge.to.to_string())
}
```

Das bedeutet: Decision-Nodes sind funktional identisch mit einem Task-Node ohne Handler. Jeder Workflow, der Decision-basiertes Routing erwartet, läuft lautlos falsch. Kein Fehler, kein Log, falscher Branch.

**Fix:** Entweder (a) `condition` als Matcher-Ausdruck gegen `ctx.memory` evaluieren, oder (b) `NodeType::Decision` entfernen und explizit dokumentieren, dass Branching ausschließlich über `StepResult.next_edge` aus Tool-Executors erfolgt.

---

**🟠 `AuditLog::new()` wird pro Aufruf instanziiert**

```rust
async fn audit_log(&self, ctx: &AgentContext, result: &StepResult) -> Result<()> {
    let audit_log = crate::audit::AuditLog::new(Arc::clone(&ctx.state_collection));
    audit_log.append(&entry).await
}
async fn audit_log_failure(&self, ctx: &AgentContext, error_message: &str) -> Result<()> {
    let audit_log = crate::audit::AuditLog::new(Arc::clone(&ctx.state_collection));
    audit_log.append(&entry).await
}
```

`AuditLog` wird für jeden Schritt (Erfolg und Fehler) neu erstellt — das bedeutet zwei `Arc::clone()`-Operationen pro Step, plus ggf. interne Initialisierungskosten. In einem hochfrequenten Workflow ist das vermeidbar.

**Fix:** `AuditLog` als Feld im `OrchestratorEngine` halten (oder einmalig erstellen und weitergeben).

---

**🟠 Event-Loop-Busy-Wait**

```rust
_ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
```

Wenn keine Events vorhanden sind, schläft der Event-Loop 50 ms und prüft dann erneut. Das ist ein Busy-Wait-Anti-Pattern. Bei N gleichzeitigen Agents bedeutet das N×20 unnötige Wake-Ups pro Sekunde.

**Fix:** `EventSource` soll einen async-fn `wait_for_event()` bereitstellen, der echte Backpressure durch einen `tokio::sync::Notify` oder `watch::Receiver` implementiert.

---

**🟠 Tool-Name als HashMap-Key dupliziert `tool.name()`**

```rust
self.tools.insert(name.to_string(), tool);
// name = tool.name() — beide Strings müssen übereinstimmen
```

Wenn `try_register_tool()` den Key aus `tool.name()` ableitet, kann ein Tool unter einem anderen Namen registriert werden als es selbst zurückgibt — was zu unauffindbaren Lookups führt. Die Konsistenz ist nicht durch das Typsystem erzwungen.

**Fix:** Der HashMap-Key soll ausschließlich aus `tool.name()` abgeleitet werden, ohne einen externen String-Parameter.

---

**🔵 `OrchestratorEngine::new()` nimmt `Arc<LsmStorage>` direkt**

Die Engine exponiert den konkreten Storage-Typ statt `Arc<dyn StorageEngine>`. Das verhindert Testing ohne echtes Dateisystem.

---

## 10. `memfuse-mcp` — MCP stdio-Server

### Stärken

- Bounded Message-Size (`MAX_RPC_BYTES = 16 MB`) mit korrektem Line-Draining bei Overflow
- Prompt-Injection-Detection als dediziertes Modul
- Sandbox-Isolierung für Code-Ausführung
- stdio-Transport (kein HTTP) — korrekte Entscheidung für eingebettete Use-Cases

### Befunde

**🟡 `MAX_RPC_BYTES = 16 MB` ist zu groß für ein embedded System**

16 MB pro JSON-RPC-Nachricht in einem System mit 128 MB Memory-Budget (MemoryBudget-Constraint) bedeutet, dass eine einzelne böswillige oder fehlerhafte Nachricht 12,5 % des Gesamtbudgets belegen kann. In Kombination mit dem 50 ms Event-Loop-Pattern in `memfuse-agent` kann ein langsamer Sender das System in einen dauerhaft degradierten Zustand bringen.

**Fix:** `MAX_RPC_BYTES` auf 1–4 MB reduzieren; für große Daten-Uploads eine separate Chunk-basierte API bereitstellen.

---

**🟡 Regex-basierte Prompt-Injection-Detection**

Regex-Pattern-Matching für Injection-Detection ist inherent cat-and-mouse: Angreifer können Unicode-Homoglyphen, Whitespace-Variationen oder syntaktische Umgehungen einsetzen. Gleichzeitig erzeugen False Positives bei legalen Texten, die Muster wie "ignore previous instructions" enthalten (z. B. in Zitaten oder Anleitungen), fehlerhafte Ablehnung.

**Fix:** Zusätzlich zu Regex eine strukturelle Validierung (schema-basiert) und Rate-Limiting einführen; Regex als erste Heuristik behalten, aber nicht als alleinige Sicherheitslinie.

---

## 11. `memfuse-ollama` — Ollama HTTP-Client

### Befunde

**🟠 Kein Trait-Abstraktions-Layer — nicht testbar ohne Live-Service**

`OllamaClient` ist eine konkrete Struct ohne zugehöriges Trait. Jeder Crate, der Ollama verwendet, ist auf einen laufenden Ollama-Server angewiesen. Integration-Tests in CI müssen entweder Ollama mitschippen oder Tests skippen.

**Fix:** `trait OllamaApi { async fn embed(...); async fn chat(...); }` definieren; `OllamaClient` implementiert den Trait. Tests können eine `MockOllamaClient`-Implementierung verwenden.

---

**🟡 Domain-Logik im Client-Modul (`build_rag_prompt`, `xml_escape`)**

`build_rag_prompt()` und `xml_escape()` sind Utility-Funktionen, die thematisch in ein Prompt-Engineering-Modul gehören, nicht in einen HTTP-Client.

---

**🟡 Importance-Scoring, Context-Prefixing und Embedding in einem Crate**

Diese drei Funktionen haben unterschiedliche Abstraktionsebenen: Embedding ist ein Infrastruktur-Concern, Context-Prefixing ist ein Domain-Concern, und Importance-Scoring ist ein ML-Concern. Das Crate fungiert als "Alles, was mit Ollama zu tun hat"-Sammlung, statt als klar abgegrenztes Modul.

---

## 12. `memfuse-router` — SLM-Routing-Engine

### Befunde

**🟠 Kalibrierungszustand wird nicht persistiert — lost on restart**

`calibration: RwLock<HashMap<String, ProfileCalibrationState>>` wird im RAM gehalten. Nach einem Neustart beginnt die konforme Vorhersage wieder von vorn mit leeren Kalibrierdaten — was bedeutet, dass alle Routing-Entscheidungen nicht kalibriert sind, bis genug neue Messungen vorhanden sind.

**Fix:** `ProfileCalibrationState` über `PersistentCheckpointStore` oder direkt in die Collection serialisieren.

---

**🔵 `ConfidenceMetrics` — zu viele optionale Felder**

```rust
pub score_lower: Option<f32>,
pub score_upper: Option<f32>,
pub calibrated: bool,
pub quantile_threshold: f32,
pub non_conformity_score: f32,
pub selection_margin: f32,
```

Wenn `calibrated = false`, sind `score_lower` und `score_upper` `None`, aber die anderen Felder sind trotzdem vorhanden. Das Struct versucht zwei Zustände (kalibriert / nicht kalibriert) in einem Typ auszudrücken.

**Fix:** `enum ConfidenceMetrics { Uncalibrated { non_conformity_score, selection_margin }, Calibrated { ... } }` oder zwei separate Structs.

---

## 13. `memfuse-embed` — ONNX Embedding Engine

### Stärken

- ONNX-Feature korrekt hinter `[features] onnx` gesperrt
- `spawn_blocking` für CPU-intensive Inference — korrekte Tokio-Hygiene

### Befunde

**🟠 `MAX_EMBED_BATCH_SIZE = 10_000` — gefährlich groß**

```rust
pub const MAX_EMBED_BATCH_SIZE: usize = 10_000;
```

Bei 1536D × 10.000 Vektoren × 4 Bytes = 60 MB Eingabe-Daten. Mit ONNX-Modell-Gewichten und Aktivierungen können das leicht 200–500 MB pro Batch-Inference werden — bei einem System mit 128 MB Memory-Budget ein direkter OOM.

**Fix:** `MAX_EMBED_BATCH_SIZE = 512` als konservativem Default; konfigurierbar über `TextEmbedderConfig`.

---

**🔵 `reranker`-Modul ist immer kompiliert, unabhängig vom `onnx`-Feature**

```toml
[features]
onnx = ["ort", "tokenizers", "ndarray"]
```

`pub mod reranker` und `pub use reranker::{CrossEncoderReranker, ...}` sind ohne Feature-Guard. `CrossEncoderReranker` erfordert ein ONNX-Modell — er ist ohne das `onnx`-Feature nicht sinnvoll nutzbar, wird aber trotzdem kompiliert.

---

## 14. `memfuse-py` — Python-Bindings (Layer 3)

### Stärken

- Per-Interpreter Tokio-Runtime für PEP 684 Sub-Interpreter-Kompatibilität
- GIL korrekt freigegeben während async-Calls
- Dedizierte `MemFuseError`-Python-Exception-Klasse

### Befunde

**🔵 `#![forbid(unsafe_code)]` ist irreführend für PyO3-Code**

PyO3's `#[pyclass]` und `#[pymethods]`-Macros generieren intern `unsafe`-Code, der jedoch nicht im Crate-eigenen Code auftaucht. Das `forbid` wirkt, schützt aber nicht gegen unsafe in Macro-Expansion. Dies schafft ein falsches Sicherheitsgefühl.

**Fix:** Eine klarstellende Kommentierung ergänzen: `// #[forbid(unsafe_code)] gilt für handgeschriebenen Code. PyO3-Macros generieren intern unsafe; dieser Overhead wird als akzeptiert betrachtet.`

---

## 15. Crate-übergreifende Befunde

### 🔴 Typ-Inkonsistenz `last_tx_id` (Zusammenfassung)

Wie in Abschnitt 1 gezeigt: `StorageEngine` gibt `TxId` zurück, alle Index-Traits geben `u64` zurück. Da `OrchestratorEngine`, `Collection` und `CheckpointGuard` diese Methode cross-subsystem aufrufen, entstehen stille Cast-Fehler.

---

### 🟠 Bincode + FlatBuffers — zwei binäre Serialisierungsframeworks

`bincode = "1.3.3"` ist Workspace-Dependency und wird für die primäre KV-Serialisierung im LSM verwendet. `flatbuffers = "24.3"` ist ebenfalls Workspace-Dependency und erscheint in `memfuse-core/src/ipc/memfuse_generated.rs`. Zwei binäre Serialisierungsframeworks ohne klare Abgrenzung ihres Anwendungsbereichs erhöhen Compile-Zeiten und kognitive Last.

**Fix:** Anwendungsbereich dokumentieren (z. B. FlatBuffers nur für IPC-Protokoll, Bincode nur für LSM-Values); beide als nicht-default-aktivierte Features konfigurierbar machen, wo möglich.

---

### 🟠 `tokio-util` in 4 Crates ohne Workspace-Verwaltung

| Crate | Versionsangabe | Features |
|---|---|---|
| `memfuse-store` | `0.7.18` | `rt` |
| `memfuse-agent` | `0.7.18` | `rt` |
| `memfuse-db` | `0.7.18` | `rt` |
| `memfuse-mcp` | `0.7` | `codec` |

Die divergierende Version (`0.7` vs. `0.7.18`) kann zu Feature-Unification-Konflikten führen. Cargo wählt die höhere Version, aktiviert aber die vereinigten Features — das `codec`-Feature aus `mcp` wird überall hinzugefügt, auch wo es nicht benötigt wird.

**Fix:** `tokio-util = { version = "0.7", features = [] }` in `[workspace.dependencies]`; jedes Crate aktiviert nur die benötigten Features.

---

### 🟠 Kein Backpressure-Mechanismus in der Ingestion-Pipeline

Die Pipeline `Ingest → Embed → Insert → Compact` hat keine Backpressure:

- `embed_batch()` default-Impl führt sequenzielle Aufrufe aus (träge, aber unbounded)
- `insert_batch()` hat kein Limit auf gleichzeitige Batches
- `CompactionEngine` hat kein Signal, wenn der Schreibdruck zu hoch ist

Bei schnellem Ingest und langsamem Embedding (Ollama-Latenz) wächst der unbounded Queue-Druck im MemTable-Puffer unkontrolliert.

**Fix:** Einen `SemaphorePermit`-basierten Ingest-Throttle in `Collection::insert_text()` einführen; `CompactionEngine` soll bei Überlastung eine `pause_writes()`-Methode auf dem `LsmStorage` aufrufen können.

---

### 🟡 `uuid`- und `lru`-Crates ohne Workspace-Management

Beide sind exklusive Abhängigkeiten von `memfuse-store` ohne Workspace-Eintrag. `uuid` kann vollständig durch `rand` ersetzt werden (bereits Workspace-Dep). `lru 0.16.3` ist ein Block-Cache-Mechanismus, der in Workspace-Dependencies aufgenommen werden sollte.

---

## Priorisierter Maßnahmenplan

### Sofortige Maßnahmen (Sprint 1)

| # | Crate | Problem | Aufwand |
|---|---|---|---|
| 1 | `memfuse-agent` | `evaluate_decision()` wertet `condition` nie aus | S |
| 2 | `memfuse-store` | `FLUSH_COUNTER` als globales Static | S |
| 3 | `memfuse-index` | `experimental-diskann` aus `default` Features entfernen | XS |
| 4 | `memfuse-checkpoint` | Deprecated Globals entfernen (`ORPHANED_CHECKPOINTS` etc.) | M |
| 5 | `memfuse-db` | `#![allow(deprecated)]` vom Crate-Level entfernen | S |
| 6 | Workspace | `tokio-util`, `uuid`, `lru` in `[workspace.dependencies]` | XS |
| 7 | `memfuse-core` | `last_tx_id()` Rückgabetyp auf `TxId` vereinheitlichen | S |

### Mittelfristig (Sprint 2–3)

| # | Crate | Problem | Aufwand |
|---|---|---|---|
| 8 | `memfuse-db` | Embedding-Duplikation im LSM beseitigen | L |
| 9 | `memfuse-core` | `async_trait` → AFIT (native Rust 1.75+) | L |
| 10 | `memfuse-index` | Gemischte Sync/Async-Locks in HNSW bereinigen | M |
| 11 | `memfuse-agent` | `AuditLog`-Instanz aus Hot-Path entfernen | XS |
| 12 | `memfuse-agent` | Event-Loop Busy-Wait durch echte Backpressure ersetzen | M |
| 13 | `memfuse-checkpoint` | `orphan_pin_file_path()` als Pflicht-Parameter | S |
| 14 | `memfuse-embed` | `MAX_EMBED_BATCH_SIZE` auf 512 senken | XS |
| 15 | `memfuse-ollama` | Trait-Abstraktion für `OllamaClient` einführen | M |

### Langfristig (Architektur-Sprint)

| # | Crate | Problem | Aufwand |
|---|---|---|---|
| 16 | `memfuse-core` | `scan_prefix` → Stream-basierte API | XL |
| 17 | `memfuse-store` | WAL V1 Migration erzwingen; Legacy-Key härten | L |
| 18 | `memfuse-router` | Kalibrierungszustand persistieren | M |
| 19 | `memfuse-text` | German-Morphologie hinter Feature-Flag | S |
| 20 | Pipeline | Backpressure-Mechanismus für Ingest-Pipeline | L |

---

*Bewertung erstellt auf Basis von Quellcode-Analyse, ohne Kompilierung. Alle Zeilenangaben sind approximativ und auf Basis der geklonten Repository-Version.*
