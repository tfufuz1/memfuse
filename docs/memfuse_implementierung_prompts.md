# MemFuse — Lead Senior Rust Architekt: Implementierungs-Prompts
## Forensische Analyse & Stabilisierungs-Direktiven

> **Autor:** Lead Senior Rust Architekt  
> **Datum:** 2026-05-29  
> **Basis:** `clippy.log` (3059 Zeilen, 130 KB), `AGENTS.md`, `Cargo.toml`, Codebase-Analyse  
> **Doctrine:** Sovereign Core — Zero-Panic, 100% Safe Rust, `cargo clippy -- -D warnings` = 0 Warnings  
> **Triple-Test-Gate:** Jedes Work Package gilt erst als DONE wenn `cargo test --workspace` und `cargo clippy --all-targets -- -D warnings` 3× hintereinander grün sind.

---

## Forensische Befunde — Priorisierte Fehler-Taxonomie

| ID | Severity | Crate | Fehler | Datei |
|---|---|---|---|---|
| **CRIT-A** | 🔴 BLOCKER | `memfuse-core` | `StorageEngine` ist nicht dyn-kompatibel (E0038) — blockiert 3 Crates | `traits.rs` |
| **CRIT-B** | 🔴 BLOCKER | `memfuse-graph` | `GraphIndex` impl lifetime mismatch (E0195) — 6 Methoden | `csr.rs` |
| **CRIT-C** | 🔴 BLOCKER | `memfuse-text` | `TextIndex` impl lifetime mismatch (E0195) — 2 impl-Blöcke × 6 Methoden | `inverted.rs` |
| **CRIT-D** | 🔴 BLOCKER | `memfuse-checkpoint` | `[u8]` ist nicht Sized (E0277) + dyn StorageEngine Fehler | `lib.rs` |
| **HIGH-001** | 🟠 HIGH | `memfuse-store` | WAL-Einträge werden bei Replay nicht CRC-verifiziert | `wal.rs` |
| **HIGH-002** | 🟠 HIGH | `memfuse-checkpoint` | `PersistentCheckpointStore` hat keinen Locking-Mechanismus | `lib.rs` |

**Gesamt-Diagnose:** Die Compilation schlägt vollständig fehl. Die Ursache ist eine architektonische Inkompatibilität zwischen dem `async fn`-Design der Kern-Traits und ihrer Verwendung als Trait-Objekte (`dyn Trait`). Seit Rust 1.75 sind native `async fn` in Traits **nicht dyn-kompatibel**. `async-trait = "0.1"` ist bereits als Workspace-Dependency deklariert, wird aber nicht auf die Traits angewendet. Das ist die Wurzel von ~90% aller Kompilierungsfehler.

---

---

# PROMPT #1 — KRITISCH: Fix `StorageEngine` Dyn-Kompatibilität

**Datei:** `crates/memfuse-core/src/traits.rs`  
**Priorität:** 🔴 MUSS ZUERST IMPLEMENTIERT WERDEN — blockiert alle anderen Crates  
**Estimierter Aufwand:** 30-45 Minuten  
**Abhängigkeiten:** Keine (Basis-Fix)

---

## KONTEXT FÜR DEN IMPLEMENTIERER

Du bist Implementierer im MemFuse-Projekt. Das Crate `memfuse-core` definiert alle Kern-Traits. Der Trait `StorageEngine` enthält 13 native `async fn`-Methoden. Seit Rust 1.75 sind Traits mit nativen `async fn`-Methoden **nicht dyn-kompatibel** — d.h. sie können nicht als `Arc<dyn StorageEngine>` oder `Box<dyn StorageEngine>` verwendet werden, weil der Compiler keine Vtable für Futures mit opaken Typen erstellen kann.

Der Workspace hat `async-trait = "0.1"` als Dependency in `[workspace.dependencies]` in der Root-`Cargo.toml`. Es wird aber noch **nicht** auf den Trait angewendet.

Der `#[async_trait::async_trait]`-Makro transformiert jede `async fn foo(&self) -> T` in `fn foo<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = T> + Send + 'async_trait>>`. Das macht den Trait vollständig vtable-kompatibel und löst **alle** `E0038`-Fehler im Workspace.

## AUFGABE

Öffne `crates/memfuse-core/src/traits.rs`. Lies die Datei vollständig. Dann führe folgende Änderungen durch:

### Schritt 1: Import hinzufügen

Stelle sicher, dass am Anfang der Datei (nach den bestehenden `use`-Statements) folgendes steht:

```rust
use async_trait::async_trait;
```

Falls `async_trait` nicht in `crates/memfuse-core/Cargo.toml` unter `[dependencies]` steht, füge es dort hinzu:

```toml
async-trait = { workspace = true }
```

### Schritt 2: `StorageEngine`-Trait mit `#[async_trait]` annotieren

Direkt ÜBER der `pub trait StorageEngine` Deklaration, füge das Attribut hinzu:

```rust
/// Storage Engine trait — abstrahiert die LSM-Tree-Persistenz.
///
/// # Dyn-Kompatibilität
/// Dieser Trait ist durch `#[async_trait]` vtable-kompatibel (dyn-safe).
/// Alle `async fn`-Methoden werden zu `Pin<Box<dyn Future<...>>>` desugared.
///
/// # Invarianten
/// - Implementierungen DÜRFEN NICHT paniken (Zero-Panic Doctrine)
/// - Alle Fehler werden über `crate::Result<T>` propagiert
#[async_trait]
pub trait StorageEngine: Send + Sync + 'static {
    // ... alle bestehenden Methoden bleiben unverändert ...
}
```

**WICHTIG:** Die Methodensignaturen innerhalb des Traits bleiben **exakt unverändert**. Nur das Attribut `#[async_trait]` wird über den Trait gesetzt.

### Schritt 3: `TextIndex`-Trait mit `#[async_trait]` annotieren

Suche den `TextIndex`-Trait in derselben Datei. Wende dasselbe Muster an:

```rust
/// Text-Index Trait — abstrahiert BM25/Inverted-Index-Operationen.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
#[async_trait]
pub trait TextIndex: Send + Sync + 'static {
    // ... alle bestehenden Methoden bleiben unverändert ...
}
```

### Schritt 4: `GraphIndex`-Trait mit `#[async_trait]` annotieren

Suche den `GraphIndex`-Trait. Wende dasselbe Muster an:

```rust
/// Graph-Index Trait — CSR-basierte Entity-Relation-Traversal.
///
/// # Dyn-Kompatibilität
/// Durch `#[async_trait]` vtable-kompatibel.
#[async_trait]
pub trait GraphIndex: Send + Sync + 'static {
    // ... alle bestehenden Methoden bleiben unverändert ...
}
```

### Schritt 5: `VectorIndex`-Trait prüfen und ggf. annotieren

Prüfe ob der `VectorIndex`-Trait ebenfalls `async fn`-Methoden enthält. Falls ja, annotiere ihn identisch:

```rust
#[async_trait]
pub trait VectorIndex: Send + Sync + 'static {
    // ...
}
```

### Schritt 6: Validierung

Führe nach den Änderungen aus:

```bash
cargo check -p memfuse-core 2>&1
```

Erwartetes Ergebnis: `0 errors`. Dann:

```bash
cargo check --workspace 2>&1 | grep "^error" | head -30
```

Die Anzahl der `E0038`-Fehler muss auf 0 fallen. Die `E0195` (lifetime mismatch) Fehler in `memfuse-graph` und `memfuse-text` werden durch PROMPT #2 und #3 behoben.

## VERBOTEN

- ❌ Die Methodensignaturen innerhalb der Traits verändern
- ❌ `?Send` verwenden (alle Engines müssen `Send` sein — multi-threaded Tokio Runtime)
- ❌ `.unwrap()` hinzufügen
- ❌ Bestehende public API-Signaturen brechen

## VALIDIERUNGS-GATE

```bash
# Muss grün sein:
cargo check -p memfuse-core
cargo test -p memfuse-core
cargo clippy -p memfuse-core -- -D warnings
```

---

---

# PROMPT #2 — KRITISCH: Fix `GraphIndex` Lifetime-Mismatches in `csr.rs`

**Datei:** `crates/memfuse-graph/src/csr.rs`  
**Priorität:** 🔴 BLOCKER (nach PROMPT #1)  
**Estimierter Aufwand:** 45-60 Minuten  
**Abhängigkeiten:** PROMPT #1 muss abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

**PREREQUISITE:** PROMPT #1 muss vollständig abgeschlossen und validiert sein, bevor du hier beginnst.

Die Datei `crates/memfuse-graph/src/csr.rs` implementiert `GraphIndex` für `CsrGraph`. Nachdem PROMPT #1 den Trait mit `#[async_trait]` annotiert hat, werden die Methoden in der Trait-Deklaration intern zu Signaturen mit `'async_trait`-Lifetimes desugared.

Der Compiler meldet `E0195` (Lifetime parameter mismatch) für 6 Methoden:
- `add_entity` (Zeile 173)
- `add_edge` (Zeile 186)  
- `traverse` (Zeile 200)
- `commit` (Zeile 264)
- `rollback` (Zeile 269)
- `stats` (Zeile 276)

**Ursache:** Die `impl`-Blöcke haben manuell Lifetimes definiert, die nicht mit den vom `#[async_trait]`-Makro erwarteten übereinstimmen.

**Lösung:** Das `#[async_trait]`-Makro auf den `impl`-Block anwenden. Das Makro normalisiert alle Lifetime-Annotationen automatisch.

## AUFGABE

### Schritt 1: Import in `csr.rs` hinzufügen

Am Anfang von `crates/memfuse-graph/src/csr.rs`, nach den bestehenden `use`-Statements:

```rust
use async_trait::async_trait;
```

Falls `async-trait` nicht in `crates/memfuse-graph/Cargo.toml` steht, füge hinzu:

```toml
[dependencies]
async-trait = { workspace = true }
```

### Schritt 2: `impl GraphIndex for CsrGraph` annotieren

Suche den Block `impl GraphIndex for CsrGraph` in `csr.rs`. Setze `#[async_trait]` direkt darüber:

```rust
#[async_trait]
impl GraphIndex for CsrGraph {
    // Die Methodensignaturen MÜSSEN exakt mit dem Trait übereinstimmen.
    // Das Makro kümmert sich um die Lifetime-Desugaring.
    // ...
}
```

### Schritt 3: Lifetime-Annotationen aus den Methoden entfernen

**KRITISCH:** Wenn die Methoden im `impl`-Block manuelle Lifetime-Parameter haben, die **nicht** im Trait deklariert sind, entferne diese. Das `#[async_trait]`-Makro übernimmt das Lifetime-Management vollständig.

**Vorher (fehlerhaft):**
```rust
async fn add_entity<'a>(&'a self, tx: TxId, entity: EntityId) -> crate::Result<()> {
    // ...
}
```

**Nachher (korrekt):**
```rust
async fn add_entity(&self, tx: TxId, entity: EntityId) -> crate::Result<()> {
    // ...
}
```

Wende das auf alle 6 betroffenen Methoden an: `add_entity`, `add_edge`, `traverse`, `commit`, `rollback`, `stats`.

### Schritt 4: Sicherstellen dass alle Trait-Methoden implementiert sind

Das `async_trait`-Makro erfordert dass der `impl`-Block **alle** im Trait deklarierten Methoden implementiert. Prüfe ob es Default-Implementierungen im Trait gibt, die in der `impl` nicht überschrieben werden müssen.

### Schritt 5: Module-Doc-Comment prüfen

Die Sovereign Core Doctrine verlangt ein `//!`-Modul-Doc-Comment. Stelle sicher dass `csr.rs` am Anfang hat:

```rust
//! CSR-Graph-Implementierung für Entity-Relation-Traversal.
//!
//! Implementiert [`memfuse_core::GraphIndex`] via Compressed Sparse Row (CSR)
//! Datenstruktur für cache-effizienten Graph-Traversal.
```

### Schritt 6: Zero-Panic Audit

Suche in der gesamten `csr.rs` nach:
- `.unwrap()` → ersetze durch `?` oder explizites Error-Handling
- `.expect("...")` → ersetze durch `return Err(MemFuseError::...)` 
- `panic!(...)` → ersetze durch `return Err(...)`
- `todo!()` → implementiere die Funktion oder gib `Err(MemFuseError::NotImplemented)` zurück

**Ausnahme:** Innerhalb von `#[cfg(test)]` sind `.unwrap()` und `.expect()` erlaubt.

## VALIDIERUNGS-GATE

```bash
cargo check -p memfuse-graph
cargo test -p memfuse-graph
cargo clippy -p memfuse-graph -- -D warnings
```

Erwartetes Ergebnis: 0 Errors, 0 Warnings.

---

---

# PROMPT #3 — KRITISCH: Fix `TextIndex` Lifetime-Mismatches in `inverted.rs`

**Datei:** `crates/memfuse-text/src/inverted.rs` und `crates/memfuse-text/src/lib.rs`  
**Priorität:** 🔴 BLOCKER (nach PROMPT #1)  
**Estimierter Aufwand:** 60-90 Minuten  
**Abhängigkeiten:** PROMPT #1 muss abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

**PREREQUISITE:** PROMPT #1 muss vollständig abgeschlossen sein.

Der `clippy.log` zeigt **zwei** separate Sätze von `E0195` Lifetime-Fehlern in `inverted.rs`:

**Satz 1 (Zeilen 376-400):** Methoden `search`, `insert`, `delete`, `commit`, `rollback`, `stats` — dies ist wahrscheinlich `impl TextIndex for InvertedIndex`  
**Satz 2 (Zeilen 460-484):** Identische Methoden — dies ist wahrscheinlich `impl TextIndex for BM25MorphIndex` oder ein anderes Struct

Zusätzlich zeigt `crates/memfuse-text/src/inverted.rs:24` und `inverted.rs:31` sowie `inverted.rs:443` Fehler **E0038** (`StorageEngine` not dyn compatible) — die nach PROMPT #1 behoben sind, aber die `impl`-Blöcke müssen noch annotiert werden.

## AUFGABE

### Schritt 1: Import in `inverted.rs` hinzufügen

```rust
use async_trait::async_trait;
```

Falls `async-trait` nicht in `crates/memfuse-text/Cargo.toml` steht:

```toml
[dependencies]
async-trait = { workspace = true }
```

### Schritt 2: Ersten `impl TextIndex`-Block annotieren

Suche den ersten `impl TextIndex for ...` Block (ca. Zeile 370). Annotiere ihn:

```rust
#[async_trait]
impl TextIndex for InvertedIndex {
    async fn search(&self, query: &str, k: usize) -> crate::Result<Vec<ScoredDocument>> {
        // Keine Lifetime-Annotationen in der Signatur!
        // ...
    }

    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> crate::Result<()> {
        // ...
    }

    async fn delete(&self, tx: TxId, id: DocId) -> crate::Result<()> {
        // ...
    }

    async fn commit(&self, tx: TxId) -> crate::Result<()> {
        // ...
    }

    async fn rollback(&self, tx: TxId) -> crate::Result<()> {
        // ...
    }

    async fn stats(&self) -> crate::Result<IndexStats> {
        // ...
    }
}
```

**KRITISCH:** Entferne alle manuellen Lifetime-Parameter aus den Methodensignaturen. Das Makro übernimmt das Lifetime-Management.

### Schritt 3: Zweiten `impl TextIndex`-Block annotieren

Suche den zweiten `impl TextIndex for ...` Block (ca. Zeile 456). Wende identisch `#[async_trait]` an und entferne manuelle Lifetimes.

### Schritt 4: `InvertedIndex` Struct — `Arc<dyn StorageEngine>` Verwendung prüfen

Der `clippy.log` zeigt dass `InvertedIndex` das Feld `Arc<dyn StorageEngine>` hält (Zeile 24, 31, 443). Nach PROMPT #1 ist `StorageEngine` dyn-kompatibel. Stelle sicher, dass:

**a)** Das Struct korrekt definiert ist:
```rust
pub struct InvertedIndex {
    storage: Arc<dyn memfuse_core::StorageEngine>,
    namespace: String,
    // ... weitere Felder
}
```

**b)** Der Konstruktor `new()` korrekt ist:
```rust
impl InvertedIndex {
    pub fn new(storage: Arc<dyn memfuse_core::StorageEngine>, namespace: &str) -> Self {
        Self {
            storage,
            namespace: namespace.to_owned(),
        }
    }
}
```

**c)** Die zweite Struct (Zeile 443) analog korrigiert ist.

### Schritt 5: `crates/memfuse-text/src/lib.rs` prüfen

Der Fehler in `lib.rs:25` zeigt `Arc<dyn memfuse_core::StorageEngine>`. Nach PROMPT #1 ist das korrekt. Stelle sicher dass in `lib.rs` die richtige Re-Export-Struktur vorliegt und keine manuellen Lifetime-Annotations auf `impl TextIndex` stehen.

### Schritt 6: Module-Doc-Comments und Zero-Panic Audit

Stelle sicher:
- `inverted.rs` hat `//!` Modul-Kommentar am Anfang
- Alle `.unwrap()` / `.expect()` außerhalb von `#[cfg(test)]` sind eliminiert
- Alle `todo!()` sind durch `Err(MemFuseError::NotImplemented)` oder echte Implementierungen ersetzt

## VALIDIERUNGS-GATE

```bash
cargo check -p memfuse-text
cargo test -p memfuse-text
cargo clippy -p memfuse-text -- -D warnings
```

---

---

# PROMPT #4 — KRITISCH: Fix `CheckpointStore` — Dyn-Kompatibilität + Sized-Bug

**Datei:** `crates/memfuse-checkpoint/src/lib.rs`  
**Priorität:** 🔴 BLOCKER (nach PROMPT #1)  
**Estimierter Aufwand:** 90-120 Minuten  
**Abhängigkeiten:** PROMPT #1 muss abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

`memfuse-checkpoint/src/lib.rs` hat zwei strukturell verschiedene Fehlerklassen:

**Fehlerklasse 1: `Arc<dyn StorageEngine>` Fehler (E0038)**
Zeilen 55, 64, 66, 95, 103, 106, 139, 172, 176, 177 — durch PROMPT #1 behoben, aber die `impl`-Blöcke von `PersistentCheckpointStore` müssen korrekt strukturiert sein.

**Fehlerklasse 2: `[u8]` nicht Sized (E0277)**  
Zeile 141: `for (_, value) in entries { ... }` — `value` wird als `[u8]` (unsized) gebunden statt als `Vec<u8>` oder `Bytes`.

Zusätzlich: **HIGH-002** — `PersistentCheckpointStore` hat keinen Locking-Mechanismus für konkurrierende Zugriffe.

## AUFGABE

### Schritt 1: Import in `lib.rs` hinzufügen

```rust
use async_trait::async_trait;
use parking_lot::Mutex;
```

Falls nicht in `crates/memfuse-checkpoint/Cargo.toml`:
```toml
[dependencies]
async-trait = { workspace = true }
parking_lot = { workspace = true }
memfuse-core = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
```

### Schritt 2: `PersistentCheckpointStore` Struct mit Locking versehen

Der aktuelle Struct hält `Arc<dyn StorageEngine>` ohne Lock. Da `StorageEngine` bereits `Send + Sync` ist (durch das Trait-Bound), ist der direkte Zugriff threadsicher, **aber** der `CheckpointRegistry`-State selbst muss geschützt werden:

```rust
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry für gespeicherte Checkpoints mit Thread-sicherem Zustand.
///
/// # Invarianten
/// - Alle Methoden sind durch `RwLock` thread-sicher
/// - `StorageEngine`-Zugriffe nutzen atomare Transaktionen via `TxId`
/// - Keine Panics (Zero-Panic Doctrine)
pub struct PersistentCheckpointStore {
    storage: Arc<dyn memfuse_core::StorageEngine>,
    /// Registrierte Checkpoints im Arbeitsspeicher — geschützt durch RwLock
    checkpoints: RwLock<HashMap<u64, CheckpointMeta>>,
    /// Namespace-Präfix für Storage-Keys
    namespace: String,
}

impl PersistentCheckpointStore {
    pub fn new(storage: Arc<dyn memfuse_core::StorageEngine>, namespace: impl Into<String>) -> Self {
        Self {
            storage,
            checkpoints: RwLock::new(HashMap::new()),
            namespace: namespace.into(),
        }
    }
}
```

### Schritt 3: Fix des `[u8]` Sized-Bugs (Zeile 141)

Der Fehler liegt in einer `for`-Schleife die über Scan-Ergebnisse iteriert. Das Muster sieht etwa so aus:

**Fehlerhaft:**
```rust
let entries = self.storage.scan_prefix(/* ... */).await?;
for (key, value) in entries {  // value hat Typ [u8] — nicht Sized!
    // ...
}
```

**Fix:** Das `scan_prefix`- oder `scan`-Resultat gibt `Vec<(Vec<u8>, Vec<u8>)>` zurück (oder `Vec<(Bytes, Bytes)>`). Der Pattern-Match muss die konkreten Owned-Typen binden:

```rust
let entries: Vec<(Vec<u8>, Vec<u8>)> = self.storage.scan_prefix(
    TxId::new(0), 
    prefix_key.as_bytes()
).await?;

for (key_bytes, value_bytes) in entries {
    // value_bytes hat Typ Vec<u8> — korrekt!
    let checkpoint_meta: CheckpointMeta = bincode::deserialize(&value_bytes)
        .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
    // ...
}
```

**Spezifische Debugging-Anleitung:** Schaue dir die Rückgabe-Signatur von `StorageEngine::scan_prefix` im Trait (`crates/memfuse-core/src/traits.rs`) genau an. Die Rückgabe ist `Result<Vec<(Vec<u8>, Vec<u8>)>>` oder ähnlich. Passe den Pattern-Match exakt an diesen Typ an.

### Schritt 4: Alle `impl` Blöcke für `PersistentCheckpointStore` korrigieren

Falls `PersistentCheckpointStore` eine `CheckpointRegistry`-Trait implementiert, annotiere diese Implementierung:

```rust
#[async_trait]
impl CheckpointRegistry for PersistentCheckpointStore {
    async fn save_checkpoint(&self, meta: CheckpointMeta) -> crate::Result<()> {
        let key = format!("{}:checkpoint:{}", self.namespace, meta.seq_no);
        let value = bincode::serialize(&meta)
            .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
        
        let tx = memfuse_core::types::TxId::new(0); // Read-only namespace
        self.storage.put(tx, key.as_bytes(), &value).await?;
        self.storage.commit(tx).await?;
        
        // In-Memory Cache aktualisieren
        self.checkpoints.write().insert(meta.seq_no, meta);
        Ok(())
    }

    async fn load_checkpoint(&self, seq_no: u64) -> crate::Result<Option<CheckpointMeta>> {
        // Erst In-Memory prüfen
        if let Some(meta) = self.checkpoints.read().get(&seq_no) {
            return Ok(Some(meta.clone()));
        }
        
        // Dann Storage
        let key = format!("{}:checkpoint:{}", self.namespace, seq_no);
        let tx = memfuse_core::types::TxId::new(0);
        match self.storage.get(tx, key.as_bytes()).await? {
            Some(bytes) => {
                let meta: CheckpointMeta = bincode::deserialize(&bytes)
                    .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    async fn list_checkpoints(&self) -> crate::Result<Vec<CheckpointMeta>> {
        let prefix = format!("{}:checkpoint:", self.namespace);
        let tx = memfuse_core::types::TxId::new(0);
        let entries: Vec<(Vec<u8>, Vec<u8>)> = self.storage
            .scan_prefix(tx, prefix.as_bytes())
            .await?;
        
        let mut result = Vec::with_capacity(entries.len());
        for (_key_bytes, value_bytes) in entries {
            let meta: CheckpointMeta = bincode::deserialize(&value_bytes)
                .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
            result.push(meta);
        }
        Ok(result)
    }
}
```

### Schritt 5: Module-Doc-Comment und Zero-Panic Audit

Stelle am Anfang von `lib.rs` sicher:

```rust
//! Checkpoint-Registry für Time-Travel und MVCC-basiertes Snapshotting.
//!
//! # Status
//! 🛑 FROZEN — Teil des SAOS-Stacks (Phase 5)
//!
//! # Architektur
//! `PersistentCheckpointStore` delegiert Persistenz an ein [`memfuse_core::StorageEngine`]-Objekt
//! und cacht aktive Checkpoints in einem thread-sicheren In-Memory-Store (`parking_lot::RwLock`).
```

## VALIDIERUNGS-GATE

```bash
cargo check -p memfuse-checkpoint
cargo test -p memfuse-checkpoint
cargo clippy -p memfuse-checkpoint -- -D warnings
```

---

---

# PROMPT #5 — HIGH-001: WAL CRC-Verifikation beim Replay

**Datei:** `crates/memfuse-store/src/wal.rs`  
**Priorität:** 🟠 HIGH — Daten-Integrität bei Crash-Recovery  
**Estimierter Aufwand:** 60-90 Minuten  
**Abhängigkeiten:** PROMPT #1-4 müssen abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

**Befund (HIGH-001):** WAL-Einträge werden bei Replay nicht CRC-verifiziert. Das ist ein kritisches Daten-Integritäts-Problem: Bei einem Crash könnte eine korrupte WAL-Datei ohne Fehler eingespielt werden, was zu Silent Data Corruption führt.

Das Crate `crc32fast = "1.3"` ist bereits als Workspace-Dependency deklariert. `blake3 = "1"` ist ebenfalls verfügbar.

**Architektur:** Die WAL schreibt Einträge als Bytestream. Jeder Eintrag hat vermutlich einen Header mit Länge. Die CRC muss beim SCHREIBEN berechnet und beim REPLAY verifiziert werden.

## AUFGABE

### Schritt 1: WAL-Entry-Format mit CRC

Definiere oder erweitere das WAL-Entry-Format um einen CRC32-Prüfwert. Das Format MUSS backward-kompatibel sein:

```rust
/// WAL-Eintrag mit Integritäts-Prüfsumme.
///
/// # Wire-Format (binäres Layout)
/// ```text
/// [crc32: u32 LE] [seq_no: u64 LE] [kind: u8] [key_len: u32 LE] 
/// [value_len: u32 LE] [key: bytes] [value: bytes]
/// ```
/// Der CRC32-Wert deckt alle nachfolgenden Bytes ab (ab seq_no bis Ende des Eintrags).
#[derive(Debug)]
pub struct WalEntry {
    pub seq_no: u64,
    pub kind: WalEntryKind,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl WalEntry {
    /// Serialisiert den Eintrag mit CRC32-Prüfsumme.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.seq_no.to_le_bytes());
        payload.push(self.kind as u8);
        payload.extend_from_slice(&(self.key.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(self.value.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.key);
        payload.extend_from_slice(&self.value);
        
        let crc = crc32fast::hash(&payload);
        let mut out = Vec::with_capacity(4 + payload.len());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Deserialisiert und verifiziert CRC32.
    ///
    /// # Errors
    /// Gibt `MemFuseError::Corruption(...)` zurück wenn CRC-Verifikation fehlschlägt.
    pub fn from_bytes(data: &[u8]) -> crate::Result<Self> {
        if data.len() < 4 {
            return Err(memfuse_core::MemFuseError::Corruption(
                "WAL-Eintrag zu kurz für CRC-Header".into()
            ));
        }
        
        let stored_crc = u32::from_le_bytes(data[..4].try_into().unwrap());
        // SAFETY: try_into auf [u8; 4] kann nicht fehlschlagen wenn len >= 4
        let payload = &data[4..];
        let computed_crc = crc32fast::hash(payload);
        
        if stored_crc != computed_crc {
            return Err(memfuse_core::MemFuseError::Corruption(format!(
                "WAL CRC-Mismatch: erwartet={:#010x}, berechnet={:#010x}. \
                 WAL-Datei ist möglicherweise korrupt.",
                stored_crc, computed_crc
            )));
        }
        
        // Deserialisierung des verifizierten Payloads
        if payload.len() < 17 { // seq_no(8) + kind(1) + key_len(4) + value_len(4)
            return Err(memfuse_core::MemFuseError::Corruption(
                "WAL-Payload zu kurz".into()
            ));
        }
        
        let seq_no = u64::from_le_bytes(payload[..8].try_into().map_err(|_| {
            memfuse_core::MemFuseError::Corruption("Ungültiger seq_no".into())
        })?);
        let kind_byte = payload[8];
        let key_len = u32::from_le_bytes(payload[9..13].try_into().map_err(|_| {
            memfuse_core::MemFuseError::Corruption("Ungültiger key_len".into())
        })?) as usize;
        let value_len = u32::from_le_bytes(payload[13..17].try_into().map_err(|_| {
            memfuse_core::MemFuseError::Corruption("Ungültiger value_len".into())
        })?) as usize;
        
        let expected_total = 17 + key_len + value_len;
        if payload.len() < expected_total {
            return Err(memfuse_core::MemFuseError::Corruption(format!(
                "WAL-Payload zu kurz: erwartet {} bytes, erhalten {}",
                expected_total, payload.len()
            )));
        }
        
        let key = payload[17..17 + key_len].to_vec();
        let value = payload[17 + key_len..17 + key_len + value_len].to_vec();
        
        let kind = WalEntryKind::try_from(kind_byte)
            .map_err(|_| memfuse_core::MemFuseError::Corruption(
                format!("Unbekannter WAL-Entry-Kind: {}", kind_byte)
            ))?;
        
        Ok(Self { seq_no, kind, key, value })
    }
}
```

### Schritt 2: WAL-Replay-Funktion mit CRC-Verifikation

Suche die WAL-Replay-Funktion (wahrscheinlich `Wal::recover()` oder `Wal::replay()`). Stelle sicher, dass sie alle Einträge CRC-verifiziert und bei Fehler klar meldet:

```rust
/// Liest und verifiziert alle WAL-Einträge seit `since_seq_no`.
///
/// # Fehlerverhalten
/// - Bei einem korrumpierten Eintrag: stoppt Replay und gibt Fehler zurück
/// - Loggt jeden gelesenen Eintrag mit `tracing::debug!`
/// - Loggt Korruption mit `tracing::error!`
///
/// # Crash-Safety
/// Unvollständige Einträge am Ende der WAL (letzter Schreibvorgang unterbrochen)
/// werden als "truncated write" erkannt und ignoriert — kein Fehler.
pub async fn replay_from(&self, since_seq_no: u64) -> crate::Result<Vec<WalEntry>> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    let data = tokio::fs::read(&self.path).await
        .map_err(|e| memfuse_core::MemFuseError::Io(e.to_string()))?;
    
    while cursor < data.len() {
        // Längen-Präfix lesen (4 bytes)
        if cursor + 4 > data.len() {
            // Truncated write am Ende — akzeptabel
            tracing::warn!(
                cursor, data_len = data.len(),
                "WAL: Truncated write am Ende erkannt, {} bytes ignoriert",
                data.len() - cursor
            );
            break;
        }
        
        let entry_len = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into().unwrap()
        ) as usize;
        cursor += 4;
        
        if cursor + entry_len > data.len() {
            // Letzter Eintrag unvollständig — truncated write
            tracing::warn!(
                cursor, entry_len, data_len = data.len(),
                "WAL: Unvollständiger letzter Eintrag, ignoriert"
            );
            break;
        }
        
        let entry_data = &data[cursor..cursor + entry_len];
        cursor += entry_len;
        
        match WalEntry::from_bytes(entry_data) {
            Ok(entry) if entry.seq_no > since_seq_no => {
                tracing::debug!(seq_no = entry.seq_no, "WAL: Replay Eintrag");
                entries.push(entry);
            }
            Ok(_) => {
                // Älterer Eintrag, überspringen
            }
            Err(e) => {
                tracing::error!(
                    cursor, error = %e,
                    "WAL: CRC-Fehler — WAL-Datei ist korrupt!"
                );
                return Err(e);
            }
        }
    }
    
    tracing::info!(
        count = entries.len(),
        "WAL: Replay abgeschlossen, {} Einträge angewendet",
        entries.len()
    );
    Ok(entries)
}
```

### Schritt 3: Tests für CRC-Verifikation

Füge am Ende von `wal.rs` Tests hinzu:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wal_entry_crc_roundtrip() {
        let entry = WalEntry {
            seq_no: 42,
            kind: WalEntryKind::Put,
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };
        
        let bytes = entry.to_bytes();
        let decoded = WalEntry::from_bytes(&bytes).expect("Roundtrip muss funktionieren");
        
        assert_eq!(decoded.seq_no, 42);
        assert_eq!(decoded.key, b"test_key");
        assert_eq!(decoded.value, b"test_value");
    }

    #[test]
    fn test_wal_entry_crc_corruption_detected() {
        let entry = WalEntry {
            seq_no: 1,
            kind: WalEntryKind::Put,
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        
        let mut bytes = entry.to_bytes();
        // Korrupte Bytes simulieren
        if bytes.len() > 10 {
            bytes[10] ^= 0xFF;
        }
        
        let result = WalEntry::from_bytes(&bytes);
        assert!(result.is_err(), "Korruption muss erkannt werden");
        
        let err_str = format!("{}", result.unwrap_err());
        assert!(err_str.contains("CRC") || err_str.contains("Corruption") || err_str.contains("korrupt"),
            "Fehler muss CRC-Mismatch beschreiben: {}", err_str);
    }

    #[tokio::test]
    async fn test_wal_replay_detects_corruption() {
        let dir = TempDir::new().expect("TempDir erstellen");
        // ... WAL erstellen, korrupten Eintrag schreiben, Replay testen
        // Implementierung abhängig von der konkreten Wal-Struktur
    }
}
```

## VALIDIERUNGS-GATE

```bash
cargo test -p memfuse-store -- wal
cargo clippy -p memfuse-store -- -D warnings
# Spezifisch den Korruptions-Test ausführen:
cargo test -p memfuse-store -- test_wal_entry_crc_corruption_detected --nocapture
```

---

---

# PROMPT #6 — HIGH-002: Locking-Mechanismus für `PersistentCheckpointStore`

**Datei:** `crates/memfuse-checkpoint/src/lib.rs`  
**Priorität:** 🟠 HIGH — Race Condition bei konkurrenten Writes  
**Estimierter Aufwand:** 30-45 Minuten  
**Abhängigkeiten:** PROMPT #4 muss abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

**Befund (HIGH-002):** `PersistentCheckpointStore` hat keinen Locking-Mechanismus. Bei konkurrenten Schreibzugriffen (z.B. mehrere Tokio-Tasks speichern gleichzeitig Checkpoints) kann es zu Race Conditions kommen.

**Hinweis:** PROMPT #4 hat bereits den Struct um ein `RwLock<HashMap>` für den In-Memory-Cache erweitert. Dieser PROMPT stellt sicher, dass auch die Storage-Schreiboperationen korrekt serialisiert werden.

## AUFGABE

### Schritt 1: Write-Serialisierung durch einen Mutex

Ergänze den Struct aus PROMPT #4 um einen `write_lock` Mutex, der verhindert dass zwei simultane `save_checkpoint` Operationen dieselbe Transaktion korrupt schreiben:

```rust
/// Registry für gespeicherte Checkpoints mit thread-sicherem Zustand.
///
/// # Thread-Safety
/// - `checkpoints`: RwLock für schnelle parallele Lesezugriffe
/// - `write_lock`: Mutex serialisiert alle Storage-Schreiboperationen
///
/// # Locking-Reihenfolge (Deadlock-Vermeidung)
/// 1. Zuerst `write_lock` acquiren
/// 2. Dann `checkpoints.write()` acquiren
/// NIEMALS in umgekehrter Reihenfolge!
pub struct PersistentCheckpointStore {
    storage: Arc<dyn memfuse_core::StorageEngine>,
    checkpoints: parking_lot::RwLock<std::collections::HashMap<u64, CheckpointMeta>>,
    /// Serialisiert Storage-Schreiboperationen. Verhindert Lost-Update-Anomalien.
    write_lock: parking_lot::Mutex<()>,
    namespace: String,
}
```

### Schritt 2: `save_checkpoint` mit Write-Lock

```rust
async fn save_checkpoint(&self, meta: CheckpointMeta) -> crate::Result<()> {
    // Write-Lock acquiren: serialisiert alle parallelen Schreibvorgänge
    let _guard = self.write_lock.lock();
    
    let key = format!("{}:checkpoint:{}", self.namespace, meta.seq_no);
    let value = bincode::serialize(&meta)
        .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
    
    // HINWEIS: TxId::new(0) ist ein vereinfachter Ansatz für den Checkpoint-Store.
    // In der Produktions-Implementierung sollte der TxBuffer aus memfuse-core
    // für atomare Transaktionen verwendet werden.
    let tx_id = memfuse_core::types::TxId::new(0);
    self.storage.put(tx_id, key.as_bytes(), &value).await?;
    self.storage.commit(tx_id).await?;
    
    // In-Memory Cache nach erfolgreichem Storage-Write aktualisieren
    self.checkpoints.write().insert(meta.seq_no, meta);
    
    // _guard wird hier gedroppt — Lock freigegeben
    Ok(())
}
```

### Schritt 3: Concurrent-Access-Test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_concurrent_checkpoint_writes_no_panic() {
        // Mock-Storage erstellen (oder einen In-Memory-Store aus memfuse-store)
        // ...
        
        // 10 simultane Checkpoint-Writes — darf nicht paniken oder korrupte Daten erzeugen
        let store = Arc::new(/* PersistentCheckpointStore::new(...) */);
        let mut handles = Vec::new();
        
        for i in 0u64..10 {
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                store_clone.save_checkpoint(CheckpointMeta {
                    seq_no: i,
                    // ... weitere Felder
                }).await.expect("Checkpoint-Write darf nicht fehlschlagen");
            }));
        }
        
        for handle in handles {
            handle.await.expect("Task darf nicht paniken");
        }
    }
}
```

## VALIDIERUNGS-GATE

```bash
cargo test -p memfuse-checkpoint
cargo clippy -p memfuse-checkpoint -- -D warnings
```

---

---

# PROMPT #7 — ABSCHLUSS: Workspace-Wide Stabilisierungs-Check

**Scope:** Gesamter Workspace  
**Priorität:** 🟡 MUSS LETZTE AUSGEFÜHRT WERDEN  
**Estimierter Aufwand:** 60-90 Minuten  
**Abhängigkeiten:** PROMPT #1-6 müssen vollständig abgeschlossen sein

---

## KONTEXT FÜR DEN IMPLEMENTIERER

Dies ist der finale Stabilisierungs-Pass. Nachdem alle Blocker und Findings behoben sind, führe einen systematischen Durchlauf durch den gesamten Workspace durch.

## AUFGABE

### Schritt 1: Vollständiger Workspace Compile-Check

```bash
cargo build --workspace 2>&1
```

Erwartetes Ergebnis: `Finished ... 0 errors`. Bei verbleibenden Fehlern: Analysiere und behebe nach denselben Prinzipien wie PROMPT #1-4.

### Schritt 2: Vollständiger Clippy-Check (Zero-Warning-Invariante)

```bash
cargo clippy --all-targets --all-features -- -D warnings 2>&1
```

**Häufige verbleibende Clippy-Warnings und ihre Fixes:**

| Warning | Fix |
|---|---|
| `clippy::missing_docs` | `/// Docstring` zu jeder pub-Funktion/Struct hinzufügen |
| `clippy::unwrap_used` | `.unwrap()` → `?` oder `.unwrap_or_else(\|e\| ...)` |
| `clippy::expect_used` | `.expect("msg")` → `?` (außerhalb von Tests) |
| `clippy::pedantic::must_use` | `#[must_use]` zu Funktionen hinzufügen die wichtige Werte zurückgeben |
| `clippy::too_many_arguments` | Struct als Parameter einführen |
| `clippy::cognitive_complexity` | Funktion aufteilen |
| `clippy::similar_names` | Variablen umbenennen |

### Schritt 3: Triple-Test-Gate ausführen

```bash
# 3× hintereinander ausführen:
cargo test --workspace -- --test-threads=1 2>&1 | tail -5
cargo test --workspace -- --test-threads=1 2>&1 | tail -5
cargo test --workspace -- --test-threads=1 2>&1 | tail -5
```

Alle 3 Läufe müssen `test result: ok.` zeigen.

### Schritt 4: Zero-Panic Audit über den gesamten Workspace

```bash
# Suche nach verbotenen Patterns (außerhalb von #[cfg(test)]):
grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "#\[cfg(test)\]" | grep -v "// SAFETY:"
grep -rn "\.expect(" crates/ --include="*.rs" | grep -v "#\[cfg(test)\]"
grep -rn "panic!(" crates/ --include="*.rs" | grep -v "#\[cfg(test)\]"
grep -rn "todo!()" crates/ --include="*.rs"
grep -rn "unreachable!()" crates/ --include="*.rs" | grep -v "#\[cfg(test)\]"
```

Für jedes Ergebnis außerhalb von Tests: Entweder eliminieren oder mit einem `// SAFETY:` / `// PANIC-FREE: ` Kommentar begründen warum diese Stelle garantiert nicht paniken kann.

### Schritt 5: Blocking I/O Audit

```bash
# Suche nach blockierendem I/O in async-Kontexten:
grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "#\[cfg(test)\]"
grep -rn "std::io::Read\|std::io::Write" crates/ --include="*.rs" | grep -v "test"
```

Jedes Vorkommen von `std::fs::` in async-Code muss durch `tokio::fs::` ersetzt werden.

### Schritt 6: Module-Doc-Comment Audit

Jede `.rs`-Datei braucht am Anfang einen `//!` Kommentar (Sovereign Core Doctrine):

```bash
# Dateien ohne //! Kommentar finden:
for f in $(find crates -name "*.rs"); do
  if ! head -5 "$f" | grep -q "//!"; then
    echo "FEHLT //!: $f"
  fi
done
```

Für jede gefundene Datei: Füge am Anfang einen aussagekräftigen Modul-Kommentar hinzu:

```rust
//! [Kurze Beschreibung des Moduls].
//!
//! # Architektur
//! [Welches Trait/Struct wird hier implementiert und warum]
//!
//! # Invarianten
//! - [Wichtigste Invariante]
```

### Schritt 7: Finaler Validierungs-Bericht

Nach Abschluss aller Schritte, dokumentiere den Status:

```bash
echo "=== MEMFUSE STABILISIERUNGS-BERICHT ===" > stabilisierung.log
echo "Datum: $(date)" >> stabilisierung.log
echo "" >> stabilisierung.log

echo "--- Build Status ---" >> stabilisierung.log
cargo build --workspace 2>&1 | tail -3 >> stabilisierung.log

echo "" >> stabilisierung.log
echo "--- Clippy Status ---" >> stabilisierung.log
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5 >> stabilisierung.log

echo "" >> stabilisierung.log
echo "--- Test Status ---" >> stabilisierung.log
cargo test --workspace 2>&1 | tail -10 >> stabilisierung.log

cat stabilisierung.log
```

Das `stabilisierung.log` ist das Übergabedokument an den Lead Architect.

---

---

# Zusammenfassung: Implementierungs-Reihenfolge

```
PHASE 1 — COMPILER-BLOCKER (SERIELL, IN DIESER REIHENFOLGE):
┌─────────────────────────────────────────────────────────────┐
│ PROMPT #1: memfuse-core/traits.rs                           │
│ → #[async_trait] auf StorageEngine, TextIndex, GraphIndex   │
│ → Beseitigt ~90% aller Kompilierungsfehler                  │
└────────────────────┬────────────────────────────────────────┘
                     │ GATE: cargo check -p memfuse-core = 0 errors
                     ▼
┌──────────────────────┬──────────────────────────────────────┐
│ PROMPT #2:           │ PROMPT #3:                           │
│ memfuse-graph/csr.rs │ memfuse-text/inverted.rs             │
│ → #[async_trait] auf │ → #[async_trait] auf beide           │
│   impl GraphIndex    │   impl TextIndex Blöcke              │
│ → Lifetime-Fixes     │ → Arc<dyn StorageEngine> Fix         │
└──────────┬───────────┘──────────────┬───────────────────────┘
           │                           │ (KÖNNEN PARALLEL LAUFEN)
           └────────────┬─────────────┘
                        │ GATE: cargo check --workspace | grep E0195 = 0
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ PROMPT #4: memfuse-checkpoint/lib.rs                        │
│ → [u8] Sized-Bug Fix (Zeile 141)                            │
│ → Arc<dyn StorageEngine> korrekt nutzen                     │
│ → RwLock für In-Memory-State                                │
└────────────────────┬────────────────────────────────────────┘
                     │ GATE: cargo build --workspace = 0 errors
                     ▼

PHASE 2 — SECURITY & STABILITY (KÖNNEN PARALLEL LAUFEN):
┌──────────────────────┬──────────────────────────────────────┐
│ PROMPT #5:           │ PROMPT #6:                           │
│ memfuse-store/wal.rs │ memfuse-checkpoint/lib.rs            │
│ → CRC auf WAL-Replay │ → Mutex für Write-Serialisierung     │
│ → HIGH-001           │ → HIGH-002                           │
└──────────┬───────────┘──────────────┬───────────────────────┘
           └────────────┬─────────────┘
                        │ GATE: cargo test --workspace = ok
                        ▼

PHASE 3 — FINALISIERUNG:
┌─────────────────────────────────────────────────────────────┐
│ PROMPT #7: Workspace-Wide Stabilisierungs-Check             │
│ → Triple-Test-Gate (3× hintereinander grün)                 │
│ → cargo clippy --all-targets -- -D warnings = 0 Warnings    │
│ → Zero-Panic Audit                                          │
│ → Module-Doc-Comment Audit                                  │
│ → Stabilisierungs-Bericht erstellen                         │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementierungs-Invarianten (ABSOLUT NICHT VERHANDELBAR)

Diese Regeln gelten für ALLE Prompts und ALLE Änderungen:

1. **`#![forbid(unsafe_code)]`** bleibt in jedem Crate — ausgenommen `memfuse-index/src/distance.rs` (SIMD)
2. **Zero `.unwrap()`** außerhalb von `#[cfg(test)]` — ausnahmslos `?` oder explizite Fehler
3. **Zero `std::fs`** in async-Kontexten — ausschließlich `tokio::fs`
4. **Jede neue public API** bekommt mindestens einen `#[tokio::test]` Contract-Test
5. **`cargo clippy -- -D warnings` = 0 Warnings** ist Pflicht vor jedem Commit
6. **Backward Compatibility** — keine bestehenden Public-API-Signaturen brechen
7. **DAG-Invariante** — keine zyklischen Abhängigkeiten zwischen Crates einführen
8. **Jede `.rs`-Datei** braucht ein `//!` Modul-Doc-Comment

---

*Erstellt von: Lead Senior Rust Architekt*  
*Basierend auf: `clippy.log` (forensische Analyse), `AGENTS.md`, `Cargo.toml`*  
*Zielgruppe: Implementierungs-Agenten (Jules, Claude Code, Gemini CLI)*  
*Sovereign Core Doctrine v1.0*
