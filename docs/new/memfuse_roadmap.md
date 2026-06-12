# MemFuse — Konkrete Roadmap & Arbeitspakete
> **Basis:** Forensische Codebase-Analyse (2026-05-28/29) | 3 unabhängige Audit-Dokumente  
> **Wichtigster Fund:** DiskANN, MarkdownChunker, BloomFilter, HNSW-Persistence-Structs — **bereits implementiert**. Roadmap ist primär Reparatur + Aktivierung, nicht Neuentwicklung.

---

## Übersicht: Release-Timeline

```
Woche 1-2   │ Phase 0 │ BUILD REPARIEREN    → cargo build grün
Woche 3-5   │ Phase 1 │ SICHERHEIT          → Datenverlust-Risiken beseitigt
Woche 6-8   │ Phase 2 │ v0.1.0 RELEASE      → pip install memfuse auf PyPI
Woche 9-14  │ Phase 3 │ SCALE & INTEGRATION → DiskANN live, MCP-Provider, LangChain
Monat 5-8   │ Phase 4 │ GOLDSTANDARD        → 4-Signal-Fusion, Time-Travel, v1.0.0
```

**Gesamter geschätzter Aufwand bis v0.1.0:** ~120–150 Agenten-Stunden  
**Gesamter Aufwand bis v1.0.0 (Goldstandard):** ~400–500 Agenten-Stunden

---

## Kritischer Pfad (Dependency-Graph der Phasen)

```
WP-0.1 (dyn-Fix) ──┐
WP-0.2 (LT-Graph) ─┤
WP-0.3 (LT-Text) ──┼──→ WP-0.5 (CI grün) ──→ WP-1.1 (WAL) ──→ WP-2.1 (HNSW-Persist)
WP-0.4 ([u8]-Fix) ─┘                                         └──→ WP-2.2 (Stable Rust)
                                                                        └──→ WP-3.1 (PyPI)
WP-1.2 (Nonce) ────────────────────────────────────────────→ WP-3.1 (PyPI)
WP-2.3 (Pytest) ───────────────────────────────────────────→ WP-3.1 (PyPI)

WP-3.1 (PyPI) ──→ WP-4.1 (MCP) ──→ WP-4.2 (LangChain) ──→ WP-5.1 (4-Signal)
                └──→ WP-4.3 (DiskANN aktiv)
```

---

## Phase 0 — Build reparieren (Woche 1–2)

**Ziel:** `cargo build --all-targets` und `cargo test --workspace` ohne Fehler.  
**Keine neuen Features in dieser Phase. Nur Fixes.**

---

### WP-0.1 — StorageEngine dyn-Kompatibilität

| Feld | Wert |
|---|---|
| **ID** | WP-0.1 |
| **Priorität** | 🔴 P0 — BLOCKER |
| **Agent** | Core Guardian (Agent 01) |
| **Aufwand** | ~3h |
| **Abhängigkeiten** | keine |

**Problem:** `PersistentCheckpointStore` in `memfuse-checkpoint/src/lib.rs` verwendet `Arc<dyn StorageEngine>` an 10 Stellen. Da `StorageEngine` async-Methoden hat, ist der Trait nicht dyn-kompatibel → `E0038`.

**Betroffene Dateien:**
- `crates/memfuse-checkpoint/src/lib.rs` — 10 Verwendungen von `Arc<dyn StorageEngine>`
- `crates/memfuse-text/src/lib.rs:25` — 1 verbleibende `Arc<dyn StorageEngine>`-Stelle

**Konkrete Änderung:**
```rust
// VORHER (crates/memfuse-checkpoint/src/lib.rs):
pub struct PersistentCheckpointStore {
    storage: Arc<dyn StorageEngine>,
}
impl PersistentCheckpointStore {
    pub fn new(storage: Arc<dyn StorageEngine>) -> Self { ... }
}

// NACHHER — generisch, analog zu memfuse-db und memfuse-text:
pub struct PersistentCheckpointStore<S: StorageEngine> {
    storage: Arc<S>,
}
impl<S: StorageEngine + Send + Sync + 'static> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>) -> Self { ... }
}
```

**Acceptance Criteria:**
- [ ] `cargo build -p memfuse-checkpoint` — 0 Errors
- [ ] `cargo build -p memfuse-text` — 0 Errors bzgl. dyn StorageEngine
- [ ] Alle bestehenden Tests in `memfuse-checkpoint` weiterhin grün
- [ ] Kein neues `unsafe` eingeführt

---

### WP-0.2 — Lifetime-Mismatches: memfuse-graph

| Feld | Wert |
|---|---|
| **ID** | WP-0.2 |
| **Priorität** | 🔴 P0 — BLOCKER |
| **Agent** | Graph Engineer (Agent 11) |
| **Aufwand** | ~3h |
| **Abhängigkeiten** | keine (unabhängig von WP-0.1) |

**Problem:** `CsrGraph` implementiert `GraphIndex`-Trait aus `memfuse-core/src/traits.rs`, aber die async-Methoden haben abweichende implizite Lifetime-Parameter.

**Betroffene Dateien:**
- `crates/memfuse-graph/src/csr.rs` — Zeilen 173, 186, 200, 264, 269, 276

**Betroffene Methoden:** `add_entity()`, `add_edge()`, `traverse()`, `commit()`, `rollback()`, `stats()`

**Konkrete Änderung:**  
Die Trait-Deklaration in `memfuse-core/src/traits.rs` und die Impl in `csr.rs` müssen identische Lifetime-Bounds haben. Standard-Pattern: `#[async_trait::async_trait]` konsistent auf Trait-Definition und alle Implementierungen anwenden, oder explizite `Send + 'static`-Bounds propagieren.

```rust
// In memfuse-core/src/traits.rs — VOR Änderung prüfen ob async_trait bereits verwendet
// Falls nicht: #[async_trait::async_trait] als Attribut auf GraphIndex + alle impl-Blöcke

// crates/memfuse-graph/src/csr.rs:
#[async_trait::async_trait]
impl GraphIndex for CsrGraph {
    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>> {
        // ... bestehende Implementierung unverändert
    }
    // ... alle 6 Methoden mit gleichem Attribut
}
```

**Acceptance Criteria:**
- [ ] `cargo build -p memfuse-graph` — 0 Errors
- [ ] Alle 5 Tests in `csr.rs` (Zeilen 390, 429, 449, 477, 498) grün
- [ ] `cargo clippy -p memfuse-graph -- -D warnings` — 0 Warnings

---

### WP-0.3 — Lifetime-Mismatches: memfuse-text

| Feld | Wert |
|---|---|
| **ID** | WP-0.3 |
| **Priorität** | 🔴 P0 — BLOCKER |
| **Agent** | Text Analyst (Agent 05) |
| **Aufwand** | ~4h |
| **Abhängigkeiten** | keine |

**Problem:** `InvertedIndex` und `BM25MorphIndex` in `inverted.rs` haben Lifetime-Mismatches für je 6 Methoden.

**Betroffene Dateien:**
- `crates/memfuse-text/src/inverted.rs` — Zeilen 376, 384, 388, 392, 396, 400 (InvertedIndex)  
- `crates/memfuse-text/src/inverted.rs` — Zeilen 460, 468, 472, 476, 480, 484 (BM25MorphIndex)

**Betroffene Methoden:** `search()`, `insert()`, `delete()`, `commit()`, `rollback()`, `stats()` — je 2× für beide Structs.

**Vorgehen:** Identisch zu WP-0.2 — `#[async_trait::async_trait]` konsistent auf `TextIndex`-Trait und alle Implementierungen anwenden.

**Acceptance Criteria:**
- [ ] `cargo build -p memfuse-text` — 0 Errors
- [ ] Alle 18 Tests in memfuse-text grün (`inverted.rs` ×5, `bm25.rs` ×6, `morphology.rs` ×3, `tokenizer.rs` ×3, `PassthroughTokenizer` ×1)
- [ ] `cargo clippy -p memfuse-text -- -D warnings` — 0 Warnings

---

### WP-0.4 — `[u8]` Sized Constraint in memfuse-checkpoint

| Feld | Wert |
|---|---|
| **ID** | WP-0.4 |
| **Priorität** | 🔴 P0 — BLOCKER |
| **Agent** | Core Guardian (Agent 01) |
| **Aufwand** | ~1h |
| **Abhängigkeiten** | WP-0.1 (beide in checkpoint/src/lib.rs) |

**Problem:** Scan-Loop in `memfuse-checkpoint/src/lib.rs:141` destructured als `(_, value)` wo `value` den Typ `[u8]` hat, der zur Compile-Zeit keine bekannte Größe hat.

**Betroffene Datei:** `crates/memfuse-checkpoint/src/lib.rs` — Zeile 141

**Konkrete Änderung:**
```rust
// VORHER (broken):
for (_, value) in entries {
    let meta = bincode::deserialize::<CheckpointMeta>(&value)?;
}

// NACHHER (korrekt):
for (_, value) in entries {
    let value: Vec<u8> = value;  // explizit als Vec<u8>
    let meta = bincode::deserialize::<CheckpointMeta>(&value)?;
}
```

**Acceptance Criteria:**
- [ ] `error[E0277]` für `[u8]` Sized verschwindet
- [ ] Zusammen mit WP-0.1: `cargo build -p memfuse-checkpoint` — 0 Errors

---

### WP-0.5 — CI-Gate grün & PR-Cleanup

| Feld | Wert |
|---|---|
| **ID** | WP-0.5 |
| **Priorität** | 🔴 P0 |
| **Agent** | QA Cross-Crate (Agent 07) |
| **Aufwand** | ~4h |
| **Abhängigkeiten** | WP-0.1, WP-0.2, WP-0.3, WP-0.4 |

**Ziel:** Vollständig grüner CI-Stand, bereinigtes PR-Backlog.

**Aufgaben:**

1. **Full-Build-Verifikation:**
   ```bash
   cargo build --all-targets          # 0 Errors
   cargo test --workspace             # alle Tests grün
   cargo clippy --all-targets -- -D warnings  # 0 Warnings
   ```

2. **clippy.log gitignoren:**
   ```gitignore
   # .gitignore — Zeile hinzufügen:
   clippy.log
   ```

3. **PR-Triage (154 offene PRs):**
   - Automatisiertes Script: Prüfe jeden PR — kompiliert er? Tests grün?
   - Auto-Close: PRs die nicht mehr compilieren oder gegen main mergen
   - Ziel: < 20 offene PRs nach diesem WP
   ```bash
   # .agent/scripts/pr-triage.sh — script erstellen
   # gh pr list --limit 200 --json number,headRefName | jq ...
   # Für jeden PR: checkout → cargo check → pass/fail → label
   ```

4. **rust-version-Konsistenz:** `Cargo.toml` `rust-version = "1.89"` mit `rust-toolchain.toml` (nightly) in Einklang bringen — kommentieren und dokumentieren.

**Acceptance Criteria:**
- [ ] `cargo build --all-targets` — 0 Errors, 0 Warnings
- [ ] `cargo test --workspace` — alle Tests grün
- [ ] `clippy.log` in `.gitignore`
- [ ] Anzahl offener PRs: ≤ 20
- [ ] GitHub Actions CI-Job `quality-gate.yml` — grün

---

## Phase 1 — Sicherheit & Integrität (Woche 3–5)

**Ziel:** Datenverlust- und Sicherheitsrisiken aus dem Audit beseitigt. MemFuse ist nach dieser Phase produktionssicher.

---

### WP-1.1 — WAL-Replay CRC-Verifikation & Rollback-Integrität

| Feld | Wert |
|---|---|
| **ID** | WP-1.1 |
| **Priorität** | 🟠 P1 — HIGH |
| **Agent** | Store Engineer (Agent 02) |
| **Aufwand** | ~6h |
| **Abhängigkeiten** | WP-0.5 (Build grün) |

**Problem (SD-02-STORE-001 + HIGH-001):** Beim WAL-Replay nach einem Crash werden CRC32-Checksummen der Einträge nicht verifiziert. Korrumpierte Daten können silently in die Datenbank geladen werden. Zusätzlich: Die MemTable-Flush-Bestätigung hat eine Race-Condition bei Error-Propagation.

**Betroffene Dateien:**
- `crates/memfuse-store/src/wal.rs` — Replay-Loop (Methode `recover()` oder äquivalent)
- `crates/memfuse-store/src/lsm.rs` — MemTable-Flush-Logik

**Konkrete Änderung — WAL-Replay:**
```rust
// In wal.rs: replay() Methode
pub async fn recover(&mut self) -> Result<Vec<WalEntry>> {
    let mut entries = Vec::new();
    // ... bestehender Code zum Lesen der Einträge
    
    for raw_entry in raw_entries {
        // NEU: CRC-Verifikation VOR Akzeptanz
        let computed_crc = crc32fast::hash(&raw_entry.data);
        if computed_crc != raw_entry.checksum {
            // Je nach Konfiguration: Fatal oder Skip+Log
            match self.config.corruption_strategy {
                CorruptionStrategy::Fatal => {
                    return Err(MemFuseError::Corruption {
                        message: format!(
                            "WAL entry at offset {} failed CRC check: expected {:#x}, got {:#x}",
                            raw_entry.offset, raw_entry.checksum, computed_crc
                        )
                    });
                }
                CorruptionStrategy::SkipAndLog => {
                    tracing::warn!(
                        "Skipping corrupted WAL entry at offset {}",
                        raw_entry.offset
                    );
                    continue;
                }
            }
        }
        entries.push(WalEntry::from_raw(raw_entry)?);
    }
    Ok(entries)
}
```

**Konkrete Änderung — Rollback-Integrität:**
```rust
// In lsm.rs: flush_memtable() — Fehler korrekt propagieren
async fn flush_memtable(&self) -> Result<()> {
    // 1. Schreibe SSTable atomisch (Write-Rename-Pattern)
    let tmp_path = self.config.path.join("tmp_flush.sst");
    let final_path = self.config.path.join(format!("{}.sst", seq_no));
    
    self.write_sstable(&tmp_path, &frozen_memtable).await?;
    tokio::fs::rename(&tmp_path, &final_path).await
        .map_err(|e| MemFuseError::Io(e))?;
    
    // 2. NUR nach erfolgreichem Rename: WAL-Checkpoint setzen
    self.wal.checkpoint(seq_no).await?;
    
    Ok(())
    // Bei ANY Fehler: tmp_path bereinigen (drop-Guard Pattern)
}
```

**Neue Tests (Pflicht):**
```rust
#[tokio::test]
async fn test_wal_replay_rejects_corrupted_entry() {
    // Schreibe WAL-Eintrag, korrumpiere 1 Byte, Recovery muss Err zurückgeben
}

#[tokio::test]  
async fn test_flush_atomicity_on_crash() {
    // Simuliere Crash nach Schreiben, vor Rename → nach Recovery: kein partielles SSTable
}
```

**Acceptance Criteria:**
- [ ] `test_wal_replay_rejects_corrupted_entry` — grün
- [ ] `test_flush_atomicity_on_crash` — grün
- [ ] `cargo test -p memfuse-store` — alle 30+ Tests grün
- [ ] Keine neuen `unwrap()`/`expect()` außerhalb `#[cfg(test)]`
- [ ] `CorruptionStrategy` als konfigurierbarer Parameter in `LsmConfig`

---

### WP-1.2 — AES-GCM Nonce-Reuse Mitigation

| Feld | Wert |
|---|---|
| **ID** | WP-1.2 |
| **Priorität** | 🟠 P1 — HIGH (Sicherheit) |
| **Agent** | Security Engineer (Agent 10) |
| **Aufwand** | ~6h |
| **Abhängigkeiten** | WP-0.5 |

**Problem (SD-09-CRYPTO-002):** AES-GCM ist bei Nonce-Wiederverwendung mit demselben Schlüssel kompromittiert (Angreifer kann Schlüssel ableiten). Der `nonce_reuse.rs`-Test existiert bereits — das Problem ist bekannt aber noch nicht behoben.

**Betroffene Dateien:**
- `crates/memfuse-crypto/src/crypto.rs` — `KeyManager::encrypt_block()`
- `crates/memfuse-crypto/tests/nonce_reuse.rs` — bestehende Tests

**Lösung — Persistenter Nonce-Counter:**
```rust
// crypto.rs — Nonce-Generierung hardened:
pub struct KeyManager {
    key: aes_gcm::Key<Aes256Gcm>,
    nonce_counter: Arc<AtomicU64>,  // NEU: persistenter Counter
    nonce_path: PathBuf,            // NEU: Counter wird auf Disk persisitert
}

impl KeyManager {
    pub async fn encrypt_block(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        // Atomisch incrementieren (monoton wachsend → keine Wiederholung)
        let counter = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        
        // Nonce = 4 Byte random (bei Init) + 8 Byte Counter
        // Das gibt 2^64 eindeutige Nonces pro Schlüssel
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[0..4].copy_from_slice(&self.nonce_prefix);
        nonce_bytes[4..12].copy_from_slice(&counter.to_le_bytes());
        
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Counter periodisch auf Disk flushen (alle 1000 Ops oder bei Flush)
        if counter % 1000 == 0 {
            self.persist_counter(counter).await?;
        }
        
        // ... encryption
    }
    
    pub async fn load_or_create(key: &[u8], path: &Path) -> Result<Self> {
        // Counter aus Disk laden + Sicherheitspuffer (+ 1000)
        // Das schützt gegen Counter-Rollback nach Crash
        let persisted = load_counter(path).await.unwrap_or(0);
        let safe_start = persisted + 1000; // Anti-Rollback-Puffer
        // ...
    }
}
```

**Alternative (einfacher, moderner):** AES-GCM-SIV — misuse-resistant, kein Nonce-Management nötig. Kostens: ~15% Performance-Overhead. Entscheidung im SPEC dokumentieren.

**Neue Tests:**
```rust
#[tokio::test]
async fn test_no_nonce_reuse_across_1000_encryptions() {
    // Sammle 1000 Nonces → keine Duplikate (HashSet)
}

#[tokio::test]
async fn test_counter_survives_restart() {
    // Verschlüssele N Blöcke, simuliere Crash, lade neu → Counter > N
}
```

**Acceptance Criteria:**
- [ ] `nonce_reuse.rs` Tests — alle grün
- [ ] `test_counter_survives_restart` — grün
- [ ] Counter-Persistenz via `tokio::fs` implementiert
- [ ] Entscheidung AES-GCM vs. AES-GCM-SIV in ADR-001 dokumentiert
- [ ] `crates/memfuse-crypto/src/crypto.rs` — 0 `unwrap()` außerhalb Tests

---

### WP-1.3 — Checkpoint-Store Locking

| Feld | Wert |
|---|---|
| **ID** | WP-1.3 |
| **Priorität** | 🟠 P1 — HIGH |
| **Agent** | Checkpoint Lead (Agent 12) |
| **Aufwand** | ~3h |
| **Abhängigkeiten** | WP-0.1, WP-0.4 (checkpoint crate baut) |

**Problem (HIGH-002 / BL-01-DB-001):** `CheckpointRegistry` und `PersistentCheckpointStore` haben keinen Thread-Safety-Locking-Mechanismus. Parallele Writes können den Checkpoint-Pointer korrumpieren.

**Betroffene Datei:** `crates/memfuse-checkpoint/src/lib.rs`

**Konkrete Änderung:**
```rust
// VORHER (unsafe bei parallelem Zugriff):
pub struct CheckpointRegistry {
    checkpoints: HashMap<String, CheckpointMeta>,
}

// NACHHER — Pattern aus memfuse-index und memfuse-text übernehmen:
pub struct CheckpointRegistry {
    inner: parking_lot::RwLock<CheckpointRegistryInner>,
}

struct CheckpointRegistryInner {
    checkpoints: HashMap<String, CheckpointMeta>,
    pinned: HashSet<TxId>,
}

impl CheckpointRegistry {
    pub fn create(&self, name: String, seq_no: u64) -> Result<CheckpointMeta> {
        let mut inner = self.inner.write();  // exklusiv
        // ... bestehende Logik
    }
    
    pub fn get(&self, name: &str) -> Option<CheckpointMeta> {
        let inner = self.inner.read();  // shared
        inner.checkpoints.get(name).cloned()
    }
}
```

**Neue Tests:**
```rust
#[tokio::test]
async fn test_concurrent_checkpoint_creation() {
    // 10 parallele tokio::spawn → alle erstellen Checkpoints → kein Panic, kein Datenverlust
}
```

**Acceptance Criteria:**
- [ ] `parking_lot::RwLock` für alle mutable State in `CheckpointRegistry`
- [ ] `test_concurrent_checkpoint_creation` — grün (Liri-Test: kein Deadlock in 30s)
- [ ] `cargo test -p memfuse-checkpoint` — alle Tests grün

---

### WP-1.4 — SIMD-Dimensionsvalidierung & SQ8-State Persistenz

| Feld | Wert |
|---|---|
| **ID** | WP-1.4 |
| **Priorität** | 🟡 P2 |
| **Agent** | Index Master (Agent 03) |
| **Aufwand** | ~4h |
| **Abhängigkeiten** | WP-0.5 |

**Probleme (SD-03-INDEX-001, MED-001):**
1. SIMD-Distanzberechnung in `distance.rs` überprüft nicht ob Input-Vektoren die gleiche Dimension haben → UB bei SIMD mit falscher Länge
2. `ScalarQuantizer` (Min/Max pro Dimension) wird nicht persistiert → Nach Neustart: neuer Quantizer mit anderen Skalierungsfaktoren → inkonsistente Scores

**Betroffene Dateien:**
- `crates/memfuse-index/src/distance.rs` — alle SIMD-Funktionen
- `crates/memfuse-index/src/quantize.rs` — `ScalarQuantizer::fit()` und Persistence

**Konkrete Änderung — Dimensionsvalidierung:**
```rust
// distance.rs — Jede öffentliche Distanzfunktion:
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(MemFuseError::DimensionMismatch {
            expected: a.len(),
            got: b.len(),
        });
    }
    // ... SIMD-Code (unverändert)
}
```

**Konkrete Änderung — SQ8-Persistenz:**
```rust
// quantize.rs — Quantizer-State serialisierbar machen:
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScalarQuantizerState {
    pub min_vals: Vec<f32>,   // per-dimension min
    pub max_vals: Vec<f32>,   // per-dimension max
    pub dimension: usize,
    pub version: u32,
}

impl ScalarQuantizer {
    pub fn to_state(&self) -> ScalarQuantizerState { ... }
    pub fn from_state(state: ScalarQuantizerState) -> Self { ... }
}
// → In memfuse-store speichern unter Key: "sq8:quantizer:{namespace}"
```

**Acceptance Criteria:**
- [ ] `distance::cosine_similarity(&[1.0f32; 128], &[1.0f32; 256])` → `Err(DimensionMismatch)`
- [ ] `ScalarQuantizerState` via `bincode` round-trip verlustfrei
- [ ] `test_sq8_survives_restart` — gleiche Scores vor und nach Reload
- [ ] Keine neuen `unsafe`-Blöcke eingeführt

---

## Phase 2 — Release-Vorbereitung (Woche 6–8)

**Ziel:** MemFuse ist auf Stable Rust, hat persistente Indizes, getestete Python-Bindings → `v0.1.0` erscheint auf PyPI und crates.io.

---

### WP-2.1 — HNSW-Persistenz aktivieren

| Feld | Wert |
|---|---|
| **ID** | WP-2.1 |
| **Priorität** | 🔴 P1 — Release-kritisch |
| **Agent** | Index Master (Agent 03) |
| **Aufwand** | ~8h |
| **Abhängigkeiten** | WP-1.1 (WAL stabil), WP-1.4 (SQ8-State) |

**Kontext:** `HnswHeader`, `NodeRecord`, `MmapIndex` in `crates/memfuse-index/src/persistence.rs` sind **bereits implementiert**. Das WP aktiviert und drähtet die Persistenz-Structs in den Load/Save-Pfad ein.

**Betroffene Dateien:**
- `crates/memfuse-index/src/hnsw.rs` — `HnswIndex::save()` und `HnswIndex::load()` Methoden
- `crates/memfuse-index/src/persistence.rs` — bestehende Structs
- `crates/memfuse-db/src/collection.rs` — Collection öffnet gespeicherten Index

**Konkrete Änderung — Save/Load:**
```rust
// hnsw.rs — zwei neue Methoden:
impl HnswIndex {
    /// Serialisiert den Graph auf Disk (atomisches Write-Rename)
    pub async fn save(&self, store: &impl StorageEngine) -> Result<()> {
        let core = self.core.read();
        
        // Header schreiben
        let header = HnswHeader {
            version: 1,
            node_count: core.nodes.len() as u64,
            dimension: core.config.dimension,
            m: core.config.m,
            metric: core.config.metric,
        };
        
        // Nodes serialisieren (bincode)
        let mut node_records = Vec::with_capacity(core.nodes.len());
        for (id, node) in &core.nodes {
            node_records.push(NodeRecord {
                id: *id,
                layer_connections: node.connections.clone(),
                vector: node.vector.clone(),
            });
        }
        
        let serialized = bincode::serialize(&(header, node_records))?;
        store.put(TxId::new(0), b"hnsw:v1:graph", &serialized).await?;
        store.put(TxId::new(0), b"hnsw:v1:sq8_state", 
                  &bincode::serialize(&self.quantizer.to_state())?).await?;
        store.commit(TxId::new(0)).await?;
        
        Ok(())
    }
    
    /// Lädt den Graph aus Disk (Cold-Start)
    pub async fn load(store: &impl StorageEngine, config: HnswConfig) -> Result<Self> {
        match store.get(b"hnsw:v1:graph").await? {
            None => Ok(HnswIndex::new(config)),  // Leere DB → neuer Index
            Some(bytes) => {
                let (header, records): (HnswHeader, Vec<NodeRecord>) = 
                    bincode::deserialize(&bytes)?;
                // Header-Validierung: dimension, version check
                if header.dimension != config.dimension {
                    return Err(MemFuseError::DimensionMismatch { ... });
                }
                // Graph rekonstruieren aus NodeRecords
                // ...
                Ok(index)
            }
        }
    }
}
```

**Inkrementeller Checkpoint (Performance):**
```rust
// Nach je N Inserts: nur Delta persistieren, nicht kompletten Graph
// Implementierung: "dirty nodes" BitSet → nur geänderte NodeRecords schreiben
```

**Neue Tests:**
```rust
#[tokio::test]
async fn test_hnsw_roundtrip_10k_vectors() {
    // Insert 10k Vektoren → save → neues HnswIndex::load() → gleiche Recall-Rate
}

#[tokio::test]
async fn test_hnsw_cold_start_latency() {
    // 100k Vektoren: load() muss in < 2s abgeschlossen sein
}
```

**Acceptance Criteria:**
- [ ] `test_hnsw_roundtrip_10k_vectors` — gleicher Recall (±2%) vor/nach Load
- [ ] `test_hnsw_cold_start_latency` — < 2s für 100k Vektoren
- [ ] Cold-Start eines leeren Pfads → neuer leerer Index (kein Fehler)
- [ ] Dimension-Mismatch beim Load → `MemFuseError::DimensionMismatch`

---

### WP-2.2 — Nightly → Stable Rust Migration

| Feld | Wert |
|---|---|
| **ID** | WP-2.2 |
| **Priorität** | 🔴 P1 — Release-kritisch |
| **Agent** | Index Master (Agent 03) + Core Guardian (Agent 01) |
| **Aufwand** | ~6h |
| **Abhängigkeiten** | WP-0.5 |

**Problem:** `portable-simd` erfordert nightly Rust. Das blockiert Adoption in Enterprise- und Produktionsumgebungen. Nightly ist kein akzeptables Requirement für eine als Dependency verwendete Bibliothek.

**Betroffene Datei:** `crates/memfuse-index/src/distance.rs`

**Migrationsstrategie — Feature-Gate:**
```rust
// distance.rs — Conditional Compilation:

/// Cosine-Ähnlichkeit mit automatischem SIMD-Fallback
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    // Dimensionscheck (WP-1.4)
    if a.len() != b.len() {
        return Err(MemFuseError::DimensionMismatch { expected: a.len(), got: b.len() });
    }
    
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        // SAFETY: AVX2 verfügbar (via target_feature check), Längen validiert
        unsafe { return cosine_avx2(a, b); }
    }
    
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    {
        // SAFETY: NEON verfügbar, Längen validiert
        unsafe { return cosine_neon(a, b); }
    }
    
    // Scalar-Fallback (stable, immer verfügbar)
    Ok(cosine_scalar(a, b))
}

// Implementierungen:
fn cosine_scalar(a: &[f32], b: &[f32]) -> f32 {
    // Reine Rust-Implementierung ohne unsafe
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b + f32::EPSILON)
}

#[cfg(target_arch = "x86_64")]
unsafe fn cosine_avx2(a: &[f32], b: &[f32]) -> f32 {
    // std::arch::x86_64 intrinsics (stable Rust!)
    use std::arch::x86_64::*;
    // ... AVX2-Implementierung
}
```

**Cargo.toml Änderung:**
```toml
# rust-toolchain.toml:
[toolchain]
channel = "stable"  # Downgrade von "nightly"

# Cargo.toml — portable-simd entfernen
# (kein Dependency mehr nötig — std::arch ist stable)
```

**Performance-Ziel:** Stable-Rust-SIMD via `std::arch` erreicht ~90% der `portable-simd`-Performance bei AVX2. Scalar-Fallback ist ~3–5× langsamer, aber korrekt.

**Acceptance Criteria:**
- [ ] `rust-toolchain.toml` zeigt `channel = "stable"`
- [ ] `cargo build --all-targets` auf stable Rust ohne Fehler
- [ ] SIMD-Distanz-Test: Ergebnis von `cosine_avx2` und `cosine_scalar` für gleichen Input — Differenz < 1e-5
- [ ] Benchmark: AVX2-Path ≥ 80% der nightly-portable-simd-Performance
- [ ] `portable-simd` aus Workspace-Dependencies entfernt

---

### WP-2.3 — Python Exception-Hierarchie & pytest-Smoke-Tests

| Feld | Wert |
|---|---|
| **ID** | WP-2.3 |
| **Priorität** | 🟠 P1 |
| **Agent** | Python Bridge (Agent 06) |
| **Aufwand** | ~6h |
| **Abhängigkeiten** | WP-2.1 (HNSW-Persistenz, für sinnvolle Tests) |

**Problem:** `memfuse-py` hat **0 Tests** — für ein PyPI-Release unakzeptabel. Alle Rust-Errors kollabieren auf `PyRuntimeError` ohne Typ-Information.

**Betroffene Datei:** `crates/memfuse-py/src/lib.rs`

**Exception-Hierarchie:**
```python
# Zielbild Python-seitig:
memfuse.MemFuseError           (Basis)
├── memfuse.DimensionError     (falsche Vektor-Dimension)
├── memfuse.NotFoundError      (Collection / Dokument nicht gefunden)
├── memfuse.CorruptionError    (WAL korrumpiert)
├── memfuse.CryptoError        (Entschlüsselungsfehler)
└── memfuse.IoError            (Dateisystem-Fehler)
```

```rust
// lib.rs — Exception-Registration:
pyo3::create_exception!(memfuse, MemFuseError, pyo3::exceptions::PyException);
pyo3::create_exception!(memfuse, DimensionError, MemFuseError);
pyo3::create_exception!(memfuse, NotFoundError, MemFuseError);
pyo3::create_exception!(memfuse, CorruptionError, MemFuseError);

// Error-Mapping:
fn map_error(err: memfuse_core::MemFuseError) -> PyErr {
    match err {
        MemFuseError::DimensionMismatch { .. } => DimensionError::new_err(err.to_string()),
        MemFuseError::NotFound { .. } => NotFoundError::new_err(err.to_string()),
        MemFuseError::Corruption { .. } => CorruptionError::new_err(err.to_string()),
        _ => MemFuseError::new_err(err.to_string()),
    }
}
```

**pytest Smoke-Tests (`crates/memfuse-py/tests/test_smoke.py`):**
```python
import pytest
import numpy as np
import memfuse

@pytest.fixture
def db(tmp_path):
    return memfuse.open(str(tmp_path / "test.db"), dimension=128)

def test_open_creates_db(tmp_path):
    db = memfuse.open(str(tmp_path / "test.db"), dimension=128)
    assert db is not None

def test_collection_insert_and_search(db):
    col = db.collection("test")
    v = np.random.rand(128).astype(np.float32)
    col.insert("doc1", v, {"key": "value"})
    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"

def test_hybrid_search(db):
    col = db.collection("hybrid")
    v = np.random.rand(128).astype(np.float32)
    col.insert("doc1", v, {"text": "hello world"})
    results = col.hybrid_search("hello", v, k=1)
    assert len(results) >= 1

def test_dimension_mismatch_raises(db):
    col = db.collection("test")
    with pytest.raises(memfuse.DimensionError):
        col.insert("x", np.zeros(64, dtype=np.float32), {})  # Falsche Dimension

def test_persistence_survives_reopen(tmp_path):
    path = str(tmp_path / "persist.db")
    db1 = memfuse.open(path, dimension=128)
    col = db1.collection("c")
    v = np.random.rand(128).astype(np.float32)
    col.insert("doc1", v, {})
    del db1
    
    db2 = memfuse.open(path, dimension=128)  # Cold-Start
    results = db2.collection("c").search(v, k=1)
    assert results[0].id == "doc1"  # Daten überlebt Neustart
```

**Acceptance Criteria:**
- [ ] Alle 5+ pytest-Tests grün: `pytest crates/memfuse-py/tests/`
- [ ] `memfuse.DimensionError`, `NotFoundError`, `CorruptionError` in Python catchbar
- [ ] `test_persistence_survives_reopen` grün (benötigt WP-2.1)
- [ ] Python Type-Stubs `memfuse.pyi` erstellt (Basis-Version)

---

### WP-2.4 — maturin Setup & PyPI-Release

| Feld | Wert |
|---|---|
| **ID** | WP-2.4 |
| **Priorität** | 🔴 P1 — Release-Meilenstein |
| **Agent** | Python Bridge (Agent 06) |
| **Aufwand** | ~6h |
| **Abhängigkeiten** | WP-2.1, WP-2.2, WP-2.3 (stabile Python-Bindings) |

**Ziel:** `pip install memfuse` auf PyPI — das ist der Kern-Versprechen des Projekts.

**Neue Datei `pyproject.toml`:**
```toml
[build-system]
requires = ["maturin>=1.5"]
build-backend = "maturin"

[project]
name = "memfuse"
version = "0.1.0"
description = "The embedded edge-AI vector database. Pure Rust, zero C-deps, pip install ready."
license = {text = "MIT OR Apache-2.0"}
requires-python = ">=3.9"
dependencies = ["numpy>=1.21"]

[tool.maturin]
features = ["pyo3/extension-module"]
module-name = "memfuse._core"
```

**GitHub Actions Matrix (`.github/workflows/release.yml`):**
```yaml
name: PyPI Release

on:
  push:
    tags: ['v*']

jobs:
  build-wheels:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest   target: x86_64
          - os: ubuntu-latest   target: aarch64   # Linux ARM
          - os: macos-latest    target: x86_64
          - os: macos-latest    target: aarch64   # Apple Silicon
          - os: windows-latest  target: x86_64
    
    steps:
      - uses: actions/checkout@v4
      - uses: PyO3/maturin-action@v1
        with:
          target: ${{ matrix.target }}
          args: --release --out dist -m crates/memfuse-py/Cargo.toml
      - uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.os }}-${{ matrix.target }}
          path: dist/

  publish-pypi:
    needs: build-wheels
    runs-on: ubuntu-latest
    steps:
      - uses: pypa/gh-action-pypi-publish@release/v1
```

**Acceptance Criteria:**
- [ ] `maturin develop` in lokalem Venv — kein Fehler
- [ ] `python -c "import memfuse; db = memfuse.open('./tmp', dimension=128)"` — läuft
- [ ] GitHub Actions Release-Workflow — alle 5 Plattformen bauen erfolgreich
- [ ] v0.1.0 auf PyPI veröffentlicht: `pip install memfuse==0.1.0` funktioniert
- [ ] v0.1.0 auf crates.io veröffentlicht: `cargo add memfuse-db` funktioniert

---

## Phase 3 — Scale & Integration (Woche 9–14)

**Ziel:** DiskANN aktiviert (10M+ Vektoren), MCP-Provider live, LangChain-Integration. Performance-Benchmarks öffentlich.

---

### WP-3.1 — DiskANN Out-of-Core Aktivierung

| Feld | Wert |
|---|---|
| **ID** | WP-3.1 |
| **Priorität** | 🟡 P2 |
| **Agent** | Index Master (Agent 03) |
| **Aufwand** | ~10h |
| **Abhängigkeiten** | WP-2.1 (HNSW-Persistenz als Basis-Pattern) |

**Kontext:** `DiskAnnIndex` und `DiskAnnConfig` in `crates/memfuse-index/src/diskann.rs` sind **bereits implementiert** (4 Integration-Tests existieren). Dieses WP verbindet DiskANN mit dem Collection-API und schreibt Tests für realistische Workloads.

**Betroffene Dateien:**
- `crates/memfuse-db/src/collection.rs` — `IndexBackend` Enum (HNSW vs. DiskANN)
- `crates/memfuse-index/src/diskann.rs` — existiert, Bugs fixen
- `crates/memfuse-index/tests/recall.rs` — Recall-Benchmark

**Collection-API-Erweiterung:**
```rust
// collection.rs:
pub struct CollectionConfig {
    pub dimension: usize,
    pub index_backend: IndexBackend,  // NEU
    pub distance_metric: DistanceMetric,
    pub encryption_key: Option<Vec<u8>>,
}

pub enum IndexBackend {
    /// Standard: bis ~5M Vektoren, vollständig in RAM
    Hnsw(HnswConfig),
    /// Out-of-Core: für > 5M Vektoren oder RAM-begrenzte Umgebungen
    DiskAnn(DiskAnnConfig),
}
```

**Python-API:**
```python
# Automatische Backend-Auswahl:
col = db.collection("large", index_backend="diskann")
# oder:
col = db.collection("small")  # default: hnsw (auto)
```

**Neue Tests:**
```rust
#[tokio::test]
async fn test_diskann_1m_vectors_recall() {
    // 1M zufällige Vektoren inserieren, 1000 Queries → Recall@10 > 90%
}

#[tokio::test]
async fn test_diskann_exceeds_ram_limit() {
    // Simuliere 2GB RAM-Limit via cgroups/ulimit → DiskANN läuft trotzdem
}
```

**Acceptance Criteria:**
- [ ] `col = db.collection("x", index_backend="diskann")` in Python funktioniert
- [ ] `test_diskann_1m_vectors_recall` — Recall@10 > 90%
- [ ] DiskANN-Index überlebt Neustart (Persistence via WP-2.1-Pattern)
- [ ] Öffentlicher Benchmark: DiskANN-Performance dokumentiert

---

### WP-3.2 — MCP-Provider

| Feld | Wert |
|---|---|
| **ID** | WP-3.2 |
| **Priorität** | 🟡 P2 — Strategisch kritisch für Adoption |
| **Agent** | Python Bridge (Agent 06) |
| **Aufwand** | ~12h |
| **Abhängigkeiten** | WP-2.4 (PyPI-Release), WP-2.1 (Persistenz) |

**Kontext:** MCP (Model Context Protocol) ist der Standardweg, mit dem Claude Code, Cursor, Continue.dev und OpenAI Agents SDK externe Tools einbinden. Ein MemFuse-MCP-Server macht MemFuse direkt aus jedem LLM-Agenten nutzbar ohne Integrationsaufwand.

**Neue Datei `crates/memfuse-py/src/mcp_server.py`** oder besser als Rust-basierter Server:

**MCP-Tool-Definitionen:**
```json
{
  "tools": [
    {
      "name": "memory_store",
      "description": "Stores a document with embedding in MemFuse memory",
      "inputSchema": {
        "type": "object",
        "properties": {
          "id": {"type": "string"},
          "text": {"type": "string"},
          "embedding": {"type": "array", "items": {"type": "number"}},
          "metadata": {"type": "object"}
        },
        "required": ["id", "text", "embedding"]
      }
    },
    {
      "name": "memory_search",
      "description": "Searches memory by vector similarity",
      "inputSchema": {
        "type": "object", 
        "properties": {
          "embedding": {"type": "array", "items": {"type": "number"}},
          "k": {"type": "integer", "default": 5},
          "filter": {"type": "object"}
        },
        "required": ["embedding"]
      }
    },
    {
      "name": "memory_hybrid_search",
      "description": "Searches memory by text AND vector (BM25 + HNSW via RRF)",
      "inputSchema": {
        "type": "object",
        "properties": {
          "text": {"type": "string"},
          "embedding": {"type": "array", "items": {"type": "number"}},
          "k": {"type": "integer", "default": 5}
        },
        "required": ["text", "embedding"]
      }
    },
    {
      "name": "memory_delete",
      "description": "Deletes a document from memory by ID",
      "inputSchema": {
        "type": "object",
        "properties": {"id": {"type": "string"}},
        "required": ["id"]
      }
    }
  ]
}
```

**Python API:**
```python
import memfuse

db = memfuse.open("./agent_memory", dimension=1536)
# Startet MCP-kompatiblen Server auf stdio oder HTTP:
memfuse.serve_mcp(db, collection="memories", transport="stdio")
# oder:
memfuse.serve_mcp(db, collection="memories", host="localhost", port=3333)
```

**`mcp.json` im Repo-Root aktualisieren:**
```json
{
  "mcpServers": {
    "memfuse": {
      "command": "python",
      "args": ["-m", "memfuse.mcp_server", "--db", "./agent_memory", "--dimension", "1536"]
    }
  }
}
```

**Acceptance Criteria:**
- [ ] `python -m memfuse.mcp_server --help` läuft ohne Fehler
- [ ] `memory_store` Tool: Insert funktioniert via MCP-Protokoll
- [ ] `memory_hybrid_search` Tool: Gibt korrekte Ergebnisse zurück
- [ ] Claude Code: `mcp.json` konfiguriert → MemFuse als Tool verfügbar
- [ ] Dokumentation: "Use MemFuse with Claude Code in 3 steps" in README

---

### WP-3.3 — LangChain & LlamaIndex Adapter

| Feld | Wert |
|---|---|
| **ID** | WP-3.3 |
| **Priorität** | 🟡 P2 |
| **Agent** | Python Bridge (Agent 06) |
| **Aufwand** | ~8h |
| **Abhängigkeiten** | WP-2.4 (PyPI), WP-2.3 (stabile Python-API) |

**LangChain VectorStore-Adapter** (`memfuse/langchain.py`):
```python
from langchain_core.vectorstores import VectorStore
from langchain_core.documents import Document
import memfuse, numpy as np

class MemFuseVectorStore(VectorStore):
    def __init__(self, path: str, dimension: int, collection: str = "default",
                 embedding_function=None):
        self._db = memfuse.open(path, dimension=dimension)
        self._col = self._db.collection(collection)
        self._embed = embedding_function
    
    def add_documents(self, documents: list[Document], **kwargs) -> list[str]:
        ids = []
        for doc in documents:
            embedding = np.array(self._embed.embed_query(doc.page_content), dtype=np.float32)
            doc_id = doc.metadata.get("id", str(uuid4()))
            self._col.insert(doc_id, embedding, doc.metadata | {"text": doc.page_content})
            ids.append(doc_id)
        return ids
    
    def similarity_search(self, query: str, k: int = 4, **kwargs) -> list[Document]:
        embedding = np.array(self._embed.embed_query(query), dtype=np.float32)
        results = self._col.hybrid_search(query, embedding, k=k)
        return [Document(page_content=r.metadata.get("text", ""), metadata=r.metadata)
                for r in results]
    
    @classmethod
    def from_texts(cls, texts, embedding, metadatas=None, path="./memfuse_db", 
                   dimension=1536, **kwargs):
        store = cls(path, dimension, embedding_function=embedding)
        docs = [Document(page_content=t, metadata=m or {}) 
                for t, m in zip(texts, metadatas or [{}]*len(texts))]
        store.add_documents(docs)
        return store
```

**Acceptance Criteria:**
- [ ] `MemFuseVectorStore` implementiert vollständiges `VectorStore`-Interface
- [ ] `langchain`-Test-Suite: 5+ Tests in `tests/test_langchain.py`
- [ ] Notebook-Beispiel: "RAG mit MemFuse + OpenAI Embeddings in 20 Zeilen"
- [ ] LlamaIndex: `MemFuseVectorStoreIndex` analog implementiert

---

### WP-3.4 — Öffentliche Benchmarks & Performance-Seite

| Feld | Wert |
|---|---|
| **ID** | WP-3.4 |
| **Priorität** | 🟡 P2 |
| **Agent** | QA Cross-Crate (Agent 07) |
| **Aufwand** | ~10h |
| **Abhängigkeiten** | WP-3.1 (DiskANN), WP-2.4 (PyPI) |

**Benchmark-Suite (`benches/`):**

```rust
// benches/insert_bench.rs — Criterion:
fn bench_insert_1k(c: &mut Criterion) {
    c.bench_function("insert_1000_vectors_dim1536", |b| {
        b.iter(|| { /* 1000 inserts in temp db */ })
    });
}

fn bench_search_10k(c: &mut Criterion) {
    // Setup: 10k vorhandene Vektoren
    c.bench_function("vector_search_k10_in_10k", |b| {
        b.iter(|| col.search(&query_vector, 10))
    });
}
```

**Python-Benchmark-Script:**
```python
# benches/compare_chroma.py
# Vergleich: MemFuse vs. ChromaDB vs. FAISS für:
# - Insert-Throughput (vectors/sec)
# - Search-Latenz P50/P95/P99 (ms)  
# - Memory-Footprint (MB per 10k vectors)
# - Recall@10 (%)
```

**Ziel-Tabelle (in `docs/BENCHMARKS.md` veröffentlichen):**

| Metrik | MemFuse | ChromaDB | FAISS |
|---|---|---|---|
| Insert 10k (dim=1536) | ≤ 3s | ~4s | ~2s |
| Search P50 (10k) | ≤ 2ms | ~5ms | ~1ms |
| Hybrid Search P50 | ≤ 5ms | N/A | N/A |
| RAM/10k Vektoren | ≤ 60MB | ~90MB | ~60MB |
| Recall@10 | ≥ 95% | ~90% | ~95% |

**Acceptance Criteria:**
- [ ] Criterion-Benchmarks in `benches/` für Insert, VecSearch, HybridSearch, Compaction
- [ ] Python-Benchmark-Script läuft gegen MemFuse + ChromaDB + FAISS
- [ ] `docs/BENCHMARKS.md` mit Ergebnissen auf echten Hardware-Specs (CPU, RAM dokumentiert)
- [ ] GitHub Actions: Benchmarks laufen automatisch bei Release-Tag

---

## Phase 4 — Goldstandard (Monat 5–8)

**Ziel:** Vollständiges Goldstandard-Feature-Set, v1.0.0.

---

### WP-4.1 — 4-Signal-Fusion API

| Feld | Wert |
|---|---|
| **ID** | WP-4.1 |
| **Priorität** | 🔵 P3 |
| **Agent** | Collection Architect (Agent 04) + Graph Engineer (Agent 11) |
| **Aufwand** | ~20h |
| **Abhängigkeiten** | WP-0.2 (Graph-Crate baut), WP-2.1, WP-3.1 |

**Ziel:** Kombinierte Suche über alle 4 Signale: Vektor-Ähnlichkeit + BM25-Keyword + Graph-Relation + Temporal-Kontext. Bereits vorbereitet durch `FusionWeights`-Typ in `memfuse-core`.

```python
# Python-API:
results = col.search_4signal(
    text="project status update",
    vector=query_vector,
    graph_seed="entity:project_memfuse",
    time_range=(1748000000, 1748100000),  # Unix-Timestamps
    k=10,
    weights=memfuse.FusionWeights(bm25=0.3, vector=0.4, graph=0.2, temporal=0.1)
)
```

**Acceptance Criteria:**
- [ ] `FusionWeights` in Python-API exponiert
- [ ] 4-Signal-Search: Ergebnisse enthalten `signal_scores: {bm25, vector, graph, temporal}`
- [ ] Graph-Seed als optionaler Parameter (kein Error wenn Graph leer)
- [ ] Performance: 4-Signal-Search P50 ≤ 20ms bei 10k Vektoren

---

### WP-4.2 — Time-Travel Queries & Checkpoint-API

| Feld | Wert |
|---|---|
| **ID** | WP-4.2 |
| **Priorität** | 🔵 P3 |
| **Agent** | Checkpoint Lead (Agent 12) |
| **Aufwand** | ~16h |
| **Abhängigkeiten** | WP-1.3 (Checkpoint-Locking), WP-2.1 (HNSW-Persistenz) |

```python
# Checkpoint setzen:
checkpoint_id = db.checkpoint("before_update_2026_05")

# Daten verändern...
col.insert("new_doc", v, {})

# Time-Travel: DB-Zustand zu altem Checkpoint:
with db.at_checkpoint(checkpoint_id) as past_db:
    old_results = past_db.collection("memories").search(v, k=5)
    # old_results enthält Daten VOR "before_update_2026_05"
```

**Acceptance Criteria:**
- [ ] `db.checkpoint(name)` → persistiert in `memfuse-checkpoint`
- [ ] `db.at_checkpoint(id)` → Read-only View auf historischen Zustand
- [ ] `db.list_checkpoints()` → `List[CheckpointInfo]`
- [ ] Test: Insert → Checkpoint → Insert → `at_checkpoint` sieht nur ersten Insert

---

### WP-4.3 — Multi-Agent Namespaces

| Feld | Wert |
|---|---|
| **ID** | WP-4.3 |
| **Priorität** | 🔵 P3 |
| **Agent** | Collection Architect (Agent 04) |
| **Aufwand** | ~10h |
| **Abhängigkeiten** | WP-2.4 (PyPI) |

```python
# Hierarchische Namespaces für Agenten-Isolation:
db = memfuse.open("./shared_memory", dimension=1536)

# Agent A hat eigenen Namespace:
agent_a = db.namespace("agent:alice")
col_a = agent_a.collection("memories")

# Agent B hat eigenen Namespace — kann nicht auf A zugreifen:
agent_b = db.namespace("agent:bob")
col_b = agent_b.collection("memories")

# Shared Namespace für gemeinsame Daten:
shared = db.namespace("shared")
```

**Acceptance Criteria:**
- [ ] Namespace-Isolation: `agent_a` kann `agent_b`-Collections nicht lesen
- [ ] `db.list_namespaces()` → alle aktiven Namespaces
- [ ] Namespace-Erstellung ist idempotent
- [ ] Cross-Namespace-Search mit expliziter Berechtigung möglich

---

### WP-4.4 — SSTable-Kompression (LZ4)

| Feld | Wert |
|---|---|
| **ID** | WP-4.4 |
| **Priorität** | 🔵 P3 |
| **Agent** | Store Engineer (Agent 02) |
| **Aufwand** | ~8h |
| **Abhängigkeiten** | WP-1.1 |

**Ziel:** SSTable-Blöcke mit LZ4 komprimieren → ~2–3× Disk-Reduktion für typische Embedding-Daten.

```toml
# Cargo.toml — neue Dependency:
lz4_flex = { version = "0.11", optional = true }

[features]
compression = ["lz4_flex"]
```

**Acceptance Criteria:**
- [ ] Feature-Flag `compression` aktiviert LZ4 optional
- [ ] Komprimierungsratio ≥ 1.5× für `f32`-Vektoren dokumentiert
- [ ] Backward-Compatibility: unkomprimierte SSTables werden weiterhin gelesen
- [ ] Kein Performance-Regression bei Search-Latenz

---

### WP-4.5 — v1.0.0 — Goldstandard Release

| Feld | Wert |
|---|---|
| **ID** | WP-4.5 |
| **Priorität** | 🔵 P3 |
| **Agent** | Alle + Human Ownership |
| **Aufwand** | ~20h (Doku, Tests, Announcement) |
| **Abhängigkeiten** | WP-4.1, WP-4.2, WP-4.3, WP-3.4 |

**Checkliste v1.0.0:**
- [ ] Alle Goldstandard-Features implementiert (WP-4.1, 4.2, 4.3)
- [ ] Öffentliche Benchmark-Seite (WP-3.4) — MemFuse gewinnt vs. ChromaDB in Embedded-Kategorie
- [ ] `docs.rs`-Dokumentation vollständig für alle public APIs
- [ ] `CHANGELOG.md` vollständig gepflegt
- [ ] `SECURITY.md` veröffentlicht (Responsible Disclosure)
- [ ] Versioning-Policy dokumentiert (SemVer, Backcompat-Garantien)
- [ ] Hacker News / Reddit Announcement vorbereitet
- [ ] Blogpost: "Why we built MemFuse: the SQLite of vector databases"

---

## Zusammenfassung: Work-Package-Übersicht

| ID | Name | Phase | Priorität | Aufwand | Agent |
|---|---|---|---|---|---|
| WP-0.1 | StorageEngine dyn-Fix | 0 | 🔴 P0 | 3h | Agent 01 |
| WP-0.2 | Lifetime-Fixes Graph | 0 | 🔴 P0 | 3h | Agent 11 |
| WP-0.3 | Lifetime-Fixes Text | 0 | 🔴 P0 | 4h | Agent 05 |
| WP-0.4 | [u8] Sized Fix | 0 | 🔴 P0 | 1h | Agent 01 |
| WP-0.5 | CI grün + PR-Cleanup | 0 | 🔴 P0 | 4h | Agent 07 |
| WP-1.1 | WAL CRC-Verifikation | 1 | 🟠 P1 | 6h | Agent 02 |
| WP-1.2 | Nonce-Reuse Crypto | 1 | 🟠 P1 | 6h | Agent 10 |
| WP-1.3 | Checkpoint-Locking | 1 | 🟠 P1 | 3h | Agent 12 |
| WP-1.4 | SIMD-Validation + SQ8 | 1 | 🟡 P2 | 4h | Agent 03 |
| WP-2.1 | HNSW-Persistenz aktiv | 2 | 🔴 P1 | 8h | Agent 03 |
| WP-2.2 | Stable Rust Migration | 2 | 🔴 P1 | 6h | Agent 03 |
| WP-2.3 | Python Tests + Exc. | 2 | 🟠 P1 | 6h | Agent 06 |
| WP-2.4 | PyPI Release | 2 | 🔴 P1 | 6h | Agent 06 |
| WP-3.1 | DiskANN aktivieren | 3 | 🟡 P2 | 10h | Agent 03 |
| WP-3.2 | MCP-Provider | 3 | 🟡 P2 | 12h | Agent 06 |
| WP-3.3 | LangChain-Adapter | 3 | 🟡 P2 | 8h | Agent 06 |
| WP-3.4 | Benchmarks öffentlich | 3 | 🟡 P2 | 10h | Agent 07 |
| WP-4.1 | 4-Signal-Fusion | 4 | 🔵 P3 | 20h | Agent 04+11 |
| WP-4.2 | Time-Travel Queries | 4 | 🔵 P3 | 16h | Agent 12 |
| WP-4.3 | Multi-Agent Namespaces | 4 | 🔵 P3 | 10h | Agent 04 |
| WP-4.4 | SSTable LZ4-Kompression | 4 | 🔵 P3 | 8h | Agent 02 |
| WP-4.5 | v1.0.0 Release | 4 | 🔵 P3 | 20h | Alle |

**Gesamt-Aufwand Phase 0–2 (bis v0.1.0):** ~51h  
**Gesamt-Aufwand Phase 0–3 (bis v0.3.0):** ~91h  
**Gesamt-Aufwand Phase 0–4 (bis v1.0.0):** ~165h

---

## Invarianten — Verbindlich

Diese Regeln gelten für jedes WP, ohne Ausnahme:

```
1.  #![forbid(unsafe_code)] in jedem Crate — Ausnahme: distance.rs mit // SAFETY: Beweis
2.  Zero .unwrap() / .expect() außerhalb #[cfg(test)]
3.  Nur tokio::fs (kein std::fs) in async-Kontexten
4.  cargo clippy --all-targets -- -D warnings → 0 Warnings nach jedem Commit
5.  Neues public fn → mindestens 1 #[tokio::test]
6.  DAG-Invariante: kein neuer Crate darf den Layer verletzen
7.  Bestehende Python-API-Signaturen dürfen nicht brechen (ab v0.1.0)
8.  Jedes WP braucht eine SPEC-{WP-ID}-{Name}.md vor Implementierungsbeginn
9.  Triple-Test-Gate: 3× hintereinander grün = DONE
10. WAL-Writes immer mit Sync-to-Disk abschließen (tokio::fs::File::sync_all)
```

---

*Roadmap erstellt auf Basis von: memfuse-vollanalyse.md, memfuse_goldstandard_report.md, memfuse_product_spec.md*  
*Stand: 2026-05-29 | Nächste Revision nach Phase-0-Abschluss*
