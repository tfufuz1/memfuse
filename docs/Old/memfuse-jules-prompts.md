# MemFuse → SME-RAG Engine: Google Jules Prompt-Sequenz

> Diese Prompts werden **nacheinander** in Google Jules ausgeführt.  
> Jeder Prompt ist in sich abgeschlossen. Jules arbeitet immer auf dem Stand des vorherigen Commits.  
> Repository: `https://github.com/tfufuz1/memfuse`

---

## Übersicht der Sequenz

| # | Aufgabe | Scope |
|---|---|---|
| 1 | Scope-Bereinigung & Legacy-Entfernung | `Cargo.toml`, Crate-Struktur |
| 2 | CVE-Patches & Dependency-Updates | `Cargo.toml`, `memfuse-store` |
| 3 | Zero-Panic Härten — Core & Store | `memfuse-core`, `memfuse-store` |
| 4 | Zero-Panic Härten — Index & Crypto | `memfuse-index`, `memfuse-crypto` |
| 5 | Graph-Persistenz aktivieren (USP-Enabler) | `memfuse-graph`, `memfuse-db` |
| 6 | Deutsche Morphologie ausbauen | `memfuse-text` |
| 7 | Python-Bindings stabilisieren | `memfuse-py` |
| 8 | Dokumenten-Ingestor (PDF/DOCX/TXT) | `memfuse-py` (Python) |
| 9 | LangChain-Integration | `memfuse-py` (Python) |
| 10 | README & Dokumentation für KMU-Zielgruppe | Alle `.md`-Dateien |

---

## Prompt 1 — Scope-Bereinigung: Legacy-Crates aus Workspace entfernen

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Scope-Bereinigung — Legacy-Crates deaktivieren

Das Projekt wird von einer militärisch orientierten Agenten-Engine zu einer
eingebetteten RAG-Engine für mittelständische Unternehmen umgebaut.
Folgende Crates gehören NICHT zum neuen Produktkern und müssen aus dem
aktiven Workspace entfernt werden, ohne die Dateien zu löschen.

### 1. Workspace-Mitglieder entfernen (`Cargo.toml` im Root)

Im `[workspace]`-Block die `members`-Liste so anpassen, dass folgende Crates
NICHT mehr aufgelistet sind (sie bleiben als Ordner erhalten, sind aber inaktiv):
- `crates/memfuse-cluster`
- `crates/memfuse-sandbox`
- `crates/memfuse-saos-agent`
- `crates/memfuse-embed`

Die verbleibenden aktiven Members sollen exakt diese sein:
```toml
members = [
    "crates/memfuse-core",
    "crates/memfuse-store",
    "crates/memfuse-index",
    "crates/memfuse-db",
    "crates/memfuse-text",
    "crates/memfuse-checkpoint",
    "crates/memfuse-crypto",
    "crates/memfuse-graph",
    "crates/memfuse-py",
]
```

`memfuse-graph` und `memfuse-py` werden hiermit reaktiviert.

### 2. Kommentar-Block in `Cargo.toml` aktualisieren

Den bestehenden `# 🧊 Frozen Zone`-Kommentarblock ersetzen durch:
```toml
# ── Archived Zone ──────────────────────────────────────────────────────────────
# Diese Crates sind nicht Teil des aktiven Builds.
# Nicht löschen — für spätere Reaktivierung aufbewahrt.
# memfuse-cluster  → Raft/gRPC (für zukünftige Multi-Node-Setups)
# memfuse-sandbox  → WASM-Sandboxing (für Plugin-System)
# memfuse-saos-agent → Agenten-Orchestrierung (für späteres Agent-Framework)
# memfuse-embed    → ONNX-Embeddings (wird via Feature-Flag optional)
```

### 3. Veraltete workspace.dependencies auskommentiert lassen

Alle bereits auskommentierten Zeilen für `openraft`, `tonic`, `prost`, `ort`
bleiben auskommentiert — keine Änderungen dort.

### 4. Neue workspace.dependency hinzufügen

Im `[workspace.dependencies]`-Block folgende Zeile ergänzen:
```toml
memfuse-graph = { path = "crates/memfuse-graph" }
memfuse-py    = { path = "crates/memfuse-py" }
```

### 5. Verifikation

Nach den Änderungen muss `cargo check --workspace` ohne Fehler durchlaufen.
Führe `cargo check --workspace 2>&1 | tail -5` aus und stelle sicher,
dass die Ausgabe `error[...]`-frei ist (Warnings sind akzeptabel).

### 6. DECISIONS.md aktualisieren

Füge am Ende von `DECISIONS.md` folgenden ADR-Eintrag ein:

```markdown
## ADR-008: Scope-Schnitt — KMU-RAG Neuausrichtung (2024)

**Status**: Beschlossen  
**Kontext**: Das Projekt wird von einer Militär/Agenten-Engine zur SME-RAG-Engine umgebaut.  
**Entscheidung**: `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent` und
`memfuse-embed` werden aus dem aktiven Workspace entfernt (Archived Zone).
`memfuse-graph` und `memfuse-py` werden reaktiviert.  
**Konsequenz**: Der Build fokussiert auf 9 Crates. Archived Crates bleiben im Repo.
```
```

---

## Prompt 2 — CVE-Patches & Dependency-Updates

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Sicherheits-Updates — CVEs beheben und Dependencies modernisieren

Es gibt zwei aktive Security Advisories, die für einen Unternehmenseinsatz
behoben werden müssen. Arbeite die folgenden Punkte ab:

### 1. RUSTSEC-2026-0186: `memmap2` ersetzen

Das Crate `memmap2 0.9.x` hat eine unsound pointer offset Vulnerability.
Ersetze es durch `memmap2 = "0.9.5"` (oder die aktuell neueste sichere
Patch-Version — prüfe `https://crates.io/crates/memmap2`).

In `Cargo.toml` (workspace):
```toml
# Alt:
memmap2 = "0.9"
# Neu:
memmap2 = "0.9.5"  # RUSTSEC-2026-0186 behoben
```

### 2. RUSTSEC-2026-0002: `lru` ersetzen durch `quick_cache`

Das Crate `lru 0.12.x` hat eine unsound `IterMut`-Implementierung.
Ersetze es vollständig durch `quick_cache`.

In `Cargo.toml` (workspace) ergänzen:
```toml
quick_cache = "0.6"
```

In `crates/memfuse-store/Cargo.toml`:
- Entferne die Zeile mit `lru`
- Füge hinzu: `quick_cache = { workspace = true }`

In `crates/memfuse-store/src/lsm.rs`:
Suche alle `use lru::` bzw. `LruCache`-Vorkommen und ersetze sie durch
die äquivalente `quick_cache::EvictClose`- oder `quick_cache::Cache`-API.

Die `quick_cache::Cache`-API ist wie folgt:
```rust
use quick_cache::sync::Cache;
let cache: Cache<K, V> = Cache::new(capacity);
cache.insert(key, value);
let val = cache.get(&key);
```

Ersetze alle `LruCache::new(cap)` durch `Cache::new(cap)`,
alle `cache.put(k, v)` durch `cache.insert(k, v)`,
alle `cache.get(&k)` bleiben identisch.

### 3. `parking_lot` Nutzung in `memfuse-db` sicherstellen

In `crates/memfuse-db/Cargo.toml` prüfen ob `parking_lot` als Dependency
vorhanden ist. Falls nicht, hinzufügen:
```toml
parking_lot = { workspace = true }
```

In `crates/memfuse-db/src/collection.rs` und `crates/memfuse-db/src/lib.rs`:
Alle `std::sync::RwLock` durch `parking_lot::RwLock` ersetzen.
Alle `std::sync::Mutex` durch `parking_lot::Mutex` ersetzen.

`parking_lot`-Locks haben kein Poison-Konzept, d.h. alle `.unwrap()`
nach `.read()` und `.write()` können durch direkte Verwendung ohne
`unwrap()` ersetzt werden:
```rust
// Alt (std):
let guard = self.inner.read().unwrap();
// Neu (parking_lot):
let guard = self.inner.read();
```

### 4. Verifikation

- `cargo check --workspace` muss fehlerfrei durchlaufen
- `cargo test -p memfuse-store 2>&1 | tail -10` ausführen
- Ausgabe: Keine `error`-Zeilen, Tests sollen durchlaufen
```

---

## Prompt 3 — Zero-Panic Härten: memfuse-core & memfuse-store

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Zero-Panic Policy durchsetzen in `memfuse-core` und `memfuse-store`

Die CONSTITUTION.md verbietet `.unwrap()` und `.expect()` in Produktionscode.
Aktuell gibt es 16+ Verstöße. Deine Aufgabe ist die vollständige Bereinigung
in den beiden Foundation-Crates.

### Regel
Jedes `.unwrap()` und `.expect("...")` im Produktionscode (NICHT in `#[cfg(test)]`-
Blöcken und NICHT in `tests/`-Unterordnern) muss durch einen der folgenden
Mechanismen ersetzt werden:

**Option A** — Fehler propagieren (bevorzugt):
```rust
// Alt:
let value = some_result.unwrap();
// Neu:
let value = some_result.map_err(|e| MemFuseError::Internal(e.to_string()))?;
```

**Option B** — Explizite Prüfung mit aussagekräftigem Fehler:
```rust
// Alt:
let value = some_option.expect("value must exist");
// Neu:
let value = some_option.ok_or_else(|| MemFuseError::Internal(
    "value must exist at this point".into()
))?;
```

**Option C** — Bei `OnceLock`/Initialisierung die bereits vorhandene
`get_runtime()`-Muster verwenden (ist schon korrekt in `memfuse-py`).

### 1. `crates/memfuse-core/src/` — alle `.rs`-Dateien

Gehe durch:
- `src/lib.rs`
- `src/error.rs`
- `src/traits.rs`
- `src/types.rs`
- `src/types/budget.rs`
- `src/types/domain.rs`
- `src/types/filter.rs`
- `src/types/saos.rs`
- `src/tx_buffer.rs`
- `src/snapshot.rs`
- `src/ipc/mod.rs`

Ersetze alle `.unwrap()` und `.expect()` außerhalb von Testblöcken.

### 2. `crates/memfuse-store/src/` — alle `.rs`-Dateien

Gehe durch:
- `src/lsm.rs`
- `src/wal.rs`
- `src/sstable.rs`
- `src/memtable.rs`
- `src/compaction.rs`
- `src/checkpoint.rs`
- `src/mmap.rs`

Besondere Vorsicht bei Lock-Operationen (nach Prompt 2 bereits `parking_lot`
— dort entfällt `.unwrap()` von selbst).

### 3. FIND-STO-001 beheben: Phantom-Daten nach Compaction

In `crates/memfuse-store/src/compaction.rs`:

Suche die Funktion, die SSTables zusammenführt (wahrscheinlich `compact()`
oder `run_compaction()`). Dort werden Tombstones (Einträge mit
`SeqNo`-Bit-63 = 1) unter Umständen zu früh verworfen.

Die Regel lautet: Ein Tombstone darf NUR dann verworfen werden, wenn die
Compaction ALLE SSTables der betreffenden Collection einschließt
(Full-Compaction). Bei Partial-Compaction müssen Tombstones erhalten bleiben.

Implementiere folgende Logik:
```rust
// In der Compaction-Schleife beim Schreiben der Output-SSTable:
if entry.is_tombstone() {
    // Tombstone nur verwerfen bei Full-Compaction (alle Tiers eingeschlossen)
    if is_full_compaction {
        continue; // Tombstone nicht in Output schreiben → bereinigt
    } else {
        // Partial-Compaction: Tombstone MUSS erhalten bleiben
        output_builder.add(entry)?;
    }
} else {
    output_builder.add(entry)?;
}
```

Füge einen `is_full_compaction: bool`-Parameter zur Compaction-Funktion hinzu,
falls noch nicht vorhanden.

### 4. Verifikation

```bash
cargo test -p memfuse-core 2>&1 | tail -5
cargo test -p memfuse-store 2>&1 | tail -5
grep -rn "\.unwrap()\|\.expect(" crates/memfuse-core/src/ crates/memfuse-store/src/ \
  | grep -v "#\[cfg(test)\]" | grep -v "tests/"
```

Die letzte Zeile darf KEINE Treffer ausgeben.
```

---

## Prompt 4 — Zero-Panic Härten: memfuse-index & memfuse-crypto

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Zero-Panic Policy durchsetzen in `memfuse-index` und `memfuse-crypto`

Gleiche Regeln wie in Prompt 3: Alle `.unwrap()` und `.expect()` außerhalb
von Testblöcken durch `?`-Operator und `MemFuseError` ersetzen.

### 1. `crates/memfuse-index/src/`

Gehe durch:
- `src/hnsw.rs` — HNSW-Vektorindex (wahrscheinlichster Ort für `unwrap()`)
- `src/diskann.rs`
- `src/distance.rs`
- `src/persistence.rs`
- `src/quantize.rs`
- `src/lib.rs`

Besondere Aufmerksamkeit in `hnsw.rs`: Vektordistanz-Berechnungen und
Layer-Zugriffe neigen zu `unwrap()`. Statt `vec[idx].unwrap()` immer
`.get(idx).ok_or_else(|| MemFuseError::Index("out of bounds".into()))?`
verwenden.

### 2. `crates/memfuse-crypto/src/`

Gehe durch:
- `src/crypto.rs`
- `src/wal_crypto.rs`
- `src/anti_tamper.rs`
- `src/lib.rs`

Bei AES-GCM-SIV Operationen: Statt `encrypt(...).unwrap()` immer:
```rust
let ciphertext = cipher
    .encrypt(nonce, plaintext)
    .map_err(|e| MemFuseError::Crypto(format!("AES-GCM-SIV encrypt failed: {e}")))?;
```

### 3. `crates/memfuse-db/src/` vollständig härten

Gehe durch:
- `src/lib.rs`
- `src/collection.rs`
- `src/context.rs`
- `src/filter.rs`
- `src/fusion.rs`
- `src/namespace.rs`
- `src/transaction.rs`
- `src/chunker.rs`
- `src/reaper.rs`

In `src/fusion.rs` gibt es bereits ein `unwrap_or` in der Sortierung:
```rust
// Diese Zeile ist bereits vorhanden und korrekt:
b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
```
Das ist akzeptabel — `partial_cmp` für `f32` kann nur bei `NaN` `None`
zurückgeben, und `Equal` als Fallback ist semantisch korrekt.
Alle anderen `unwrap()` müssen weg.

### 4. FIND-DB-002 beheben: drop_collection hinterlässt Datenleichen

In `crates/memfuse-db/src/lib.rs` oder `collection.rs`:

Suche die `drop_collection()`- oder `delete_collection()`-Funktion.
Stelle sicher, dass beim Löschen einer Collection der gesamte
LSM-Prefix `__col:{name}:\x00` durch einen Storage-Level-Befehl
bereinigt wird.

Implementiere falls noch nicht vorhanden:
```rust
pub async fn drop_collection(&self, name: &str) -> Result<()> {
    let prefix = format!("__col:{}:\x00", name);
    // Alle Keys mit diesem Prefix löschen
    self.storage.delete_prefix(prefix.as_bytes()).await?;
    // HNSW-Index aus der In-Memory-Map entfernen
    self.collections.write().remove(name);
    Ok(())
}
```

Falls `delete_prefix()` noch nicht im `StorageEngine`-Trait existiert,
füge es in `crates/memfuse-core/src/traits.rs` hinzu:
```rust
async fn delete_prefix(&self, prefix: &[u8]) -> Result<()>;
```
Und implementiere es in `crates/memfuse-store/src/lsm.rs` als Iteration
über alle Keys mit dem gegebenen Prefix und anschließendem `delete()`.

### 5. Verifikation

```bash
cargo test -p memfuse-index 2>&1 | tail -5
cargo test -p memfuse-crypto 2>&1 | tail -5
cargo test -p memfuse-db 2>&1 | tail -5
grep -rn "\.unwrap()\|\.expect(" \
  crates/memfuse-index/src/ \
  crates/memfuse-crypto/src/ \
  crates/memfuse-db/src/ \
  | grep -v "#\[cfg(test)\]" | grep -v "tests/"
```

Keine Treffer außer dem akzeptierten `partial_cmp().unwrap_or(...)` in `fusion.rs`.
```

---

## Prompt 5 — Graph-Persistenz aktivieren (USP-Enabler)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: `memfuse-graph` reaktivieren und CSR-Graph persistent machen

Dies ist der wichtigste einzelne Feature-Schritt. Aktuell verliert der
CSR-Graph alle Daten bei einem Neustart (FIND-GRA-001). Ohne persistenten
Graph ist das "4-Signal"-Versprechen nicht erfüllbar.

### Hintergrund

`crates/memfuse-graph/src/csr.rs` implementiert einen vollständigen
In-Memory CSR-Graph mit:
- `GraphInner` als innerer Zustand
- `staged_entities`, `staged_edges` für Transaktions-Staging
- `committed_staged` für committete aber nicht kompaktierte Kanten
- `SCORE_DECAY = 0.7` und `MAX_TRAVERSAL_HOPS = 3`

Der Graph implementiert den `GraphIndex`-Trait aus `memfuse-core`.
Er hat bisher KEINE Persistenz-Logik — alles lebt nur im RAM.

### 1. `crates/memfuse-graph/Cargo.toml` aktualisieren

Füge folgende Dependencies hinzu:
```toml
[dependencies]
memfuse-core = { workspace = true }
memfuse-store = { workspace = true }
parking_lot = { workspace = true }
serde = { workspace = true }
bincode = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
tracing = { workspace = true }
```

### 2. Persistenz-Layer in `crates/memfuse-graph/src/csr.rs` implementieren

Füge dem `CsrGraph`-Struct ein optionales Storage-Handle hinzu:
```rust
use memfuse_store::LsmStorage;
use memfuse_core::StorageEngine;
use std::sync::Arc;

pub struct CsrGraph {
    inner: RwLock<GraphInner>,
    /// Optional persistent storage backend.
    /// None → pure in-memory (test mode)
    /// Some → CSR is persisted under namespace "__graph:"
    storage: Option<Arc<LsmStorage>>,
}
```

Füge folgende Konstanten hinzu:
```rust
/// LSM-Key-Prefix für alle Graph-Daten.
const GRAPH_NAMESPACE: &str = "__graph:";
const GRAPH_ENTITIES_PREFIX: &str = "__graph:entity:";
const GRAPH_EDGES_PREFIX: &str = "__graph:edge:";
```

### 3. `save_entity()` und `save_edge()` implementieren

```rust
impl CsrGraph {
    /// Persists a single entity to the LSM store.
    async fn persist_entity(&self, entity: &Entity, tx_id: TxId) -> Result<()> {
        let Some(storage) = &self.storage else { return Ok(()); };
        let key = format!("{}{}",
            GRAPH_ENTITIES_PREFIX,
            entity.id.as_str()
        );
        let value = bincode::serialize(entity)
            .map_err(|e| MemFuseError::Internal(format!("graph serialize: {e}")))?;
        storage.put(key.as_bytes(), &value, tx_id).await
    }

    /// Persists a single edge to the LSM store.
    async fn persist_edge(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        weight: f32,
        tx_id: TxId,
    ) -> Result<()> {
        let Some(storage) = &self.storage else { return Ok(()); };
        let key = format!("{}{}:{}",
            GRAPH_EDGES_PREFIX,
            from_id.as_str(),
            to_id.as_str()
        );
        let value = bincode::serialize(&weight)
            .map_err(|e| MemFuseError::Internal(format!("graph edge serialize: {e}")))?;
        storage.put(key.as_bytes(), &value, tx_id).await
    }
}
```

### 4. `load_from_storage()` beim Start implementieren

```rust
impl CsrGraph {
    /// Loads all persisted graph data from LSM storage on startup.
    /// Called once during `CsrGraph::open()`.
    pub async fn load_from_storage(&self) -> Result<()> {
        let Some(storage) = &self.storage else { return Ok(()); };

        // Load entities
        let entity_keys = storage
            .scan_prefix(GRAPH_ENTITIES_PREFIX.as_bytes())
            .await?;
        for (_, raw_value) in entity_keys {
            let entity: Entity = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph deserialize entity: {e}")))?;
            let mut inner = self.inner.write();
            inner.insert_entity_internal(entity);
        }

        // Load edges
        let edge_keys = storage
            .scan_prefix(GRAPH_EDGES_PREFIX.as_bytes())
            .await?;
        for (raw_key, raw_value) in edge_keys {
            let weight: f32 = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph deserialize edge: {e}")))?;
            // Key-Format: "__graph:edge:{from_id}:{to_id}"
            let key_str = String::from_utf8_lossy(&raw_key);
            let parts: Vec<&str> = key_str
                .strip_prefix(GRAPH_EDGES_PREFIX)
                .unwrap_or("")
                .splitn(2, ':')
                .collect();
            if parts.len() == 2 {
                let from_id = EntityId::from(parts[0]);
                let to_id = EntityId::from(parts[1]);
                let mut inner = self.inner.write();
                inner.add_edge_internal(from_id, to_id, weight);
            }
        }

        tracing::info!("Graph loaded from storage");
        Ok(())
    }
}
```

### 5. `scan_prefix()` dem `StorageEngine`-Trait hinzufügen

Falls noch nicht vorhanden, in `crates/memfuse-core/src/traits.rs`:
```rust
/// Scans all key-value pairs with keys starting with `prefix`.
async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
```

Implementiere es in `crates/memfuse-store/src/lsm.rs` durch Iteration
über MemTable und SSTables und Filterung nach Prefix.

### 6. `GraphIndex`-Trait-Methoden anpassen

Im `impl GraphIndex for CsrGraph`-Block: Die `add_entity()` und
`add_edge()`-Methoden nach dem In-Memory-Update auch die
`persist_entity()` / `persist_edge()`-Funktionen aufrufen.
Da der Trait async ist, geht das direkt.

### 7. `memfuse-graph` in `memfuse-db` integrieren

In `crates/memfuse-db/Cargo.toml`:
```toml
memfuse-graph = { workspace = true }
```

In `crates/memfuse-db/src/lib.rs` bzw. `collection.rs`:
- Importiere `CsrGraph`
- Füge dem `Collection`-Struct ein Feld hinzu:
  ```rust
  pub(crate) graph: Arc<CsrGraph>,
  ```
- Initialisiere den Graph beim Öffnen einer Collection mit dem
  gleichen `LsmStorage`-Handle
- Rufe `graph.load_from_storage().await?` beim Collection-Open auf

### 8. Verifikation

```bash
cargo test -p memfuse-graph 2>&1 | tail -10
cargo test -p memfuse-db 2>&1 | tail -10
```

Schreibe außerdem einen neuen Integrationstest in
`crates/memfuse-graph/tests/persistence_test.rs`:
```rust
#[tokio::test]
async fn test_graph_survives_restart() {
    // 1. Graph öffnen, Entity und Edge einfügen, schließen
    // 2. Graph erneut öffnen (gleicher Storage-Pfad)
    // 3. Entity und Edge müssen noch vorhanden sein
}
```
```

---

## Prompt 6 — Deutsche Morphologie ausbauen (DACH-Differenzierung)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Deutsche Morphologie in `memfuse-text` für KMU-Fachvokabular ausbauen

Die existierende `GermanCompoundSplitter`-Implementierung in
`crates/memfuse-text/src/morphology.rs` hat ein kleines Wörterbuch mit
~19 Einträgen. Für den KMU-Einsatz (Maschinenbau, Handel, Logistik, HR,
Finanzen) muss das deutlich ausgebaut werden.

### 1. Wörterbuch in `GermanCompoundSplitter` erweitern

Ersetze die bestehende `dictionary`-Konstante in der `decompose()`-Methode
durch ein deutlich umfangreicheres Set. Teile das Wörterbuch in thematische
Gruppen auf und definiere es als `const`-Array auf Modul-Ebene:

```rust
/// KMU-Fachvokabular für deutschen Compound-Splitter.
/// Geordnet nach Häufigkeit (häufigste zuerst für Performance).
const GERMAN_KMU_DICTIONARY: &[&str] = &[
    // ── Allgemeine Geschäftsbegriffe ──────────────────────────────────────
    "auftrags", "auftrag", "angebots", "angebot", "rechnung", "lieferung",
    "bestellung", "kunden", "kunde", "lieferanten", "lieferant",
    "vertrags", "vertrag", "zahlungs", "zahlung", "abrechnung",
    "kosten", "preis", "rabatt", "skonto", "marge",
    "umsatz", "gewinn", "verlust", "budget", "planung",
    // ── HR & Personal ────────────────────────────────────────────────────
    "mitarbeiter", "personal", "urlaubs", "urlaub", "kranken",
    "lohn", "gehalts", "gehalt", "arbeits", "arbeit", "stelle",
    "bewerbungs", "bewerbung", "schulungs", "schulung", "weiter",
    "bildungs", "bildung", "zeugnis", "zeugnis",
    // ── Logistik & Lager ─────────────────────────────────────────────────
    "lager", "bestands", "bestand", "transport", "versand",
    "liefer", "empfangs", "empfang", "rücksendung", "rücksende",
    "fracht", "zoll", "import", "export", "logistik",
    // ── Maschinenbau & Produktion ─────────────────────────────────────────
    "fertigungs", "fertigung", "produktions", "produktion", "qualitäts",
    "qualität", "wartungs", "wartung", "reparatur", "instand",
    "maschinen", "maschine", "anlagen", "anlage", "steuerung",
    "prüfungs", "prüfung", "mess", "sensor", "prozess",
    // ── IT & Digital ─────────────────────────────────────────────────────
    "daten", "bank", "system", "software", "hardware",
    "netzwerk", "zugriffs", "zugriff", "sicherheits", "sicherheit",
    "verschlüsselung", "backup", "server", "cloud",
    // ── Recht & Compliance ───────────────────────────────────────────────
    "datenschutz", "dsgvo", "compliance", "richtlinie", "gesetz",
    "vorschrift", "genehmigung", "zertifizierung", "norm", "iso",
    "haftung", "gewähr", "leistungs", "leistung",
    // ── Finanzen ─────────────────────────────────────────────────────────
    "finanz", "steuer", "buchhaltung", "bilanz", "liquidität",
    "investition", "kredit", "zinsen", "tilgung", "cashflow",
    // ── Basis (aus alter Implementierung) ────────────────────────────────
    "bundes", "verfassungs", "gericht", "entwurf",
    "speicher", "vektor", "suche", "verwaltung",
    "bericht", "schutz", "rechte",
];
```

Passe die `decompose()`-Methode an, um dieses Modul-Level-Dictionary zu
verwenden statt der lokalen Variable.

### 2. Umlaut-Normalisierung hinzufügen

Füge eine neue Funktion hinzu und integriere sie in den Tokenizer-Flow:

```rust
/// Normalisiert deutsche Umlaute für robusten Abgleich.
/// "Änderung" und "aenderung" werden zum gleichen Token.
pub fn normalize_umlauts(input: &str) -> String {
    input
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
}
```

Integriere diese Funktion im `Tokenizer` (`src/tokenizer.rs`), sodass
beim Indexieren und bei der Suche Umlaute normalisiert werden.

### 3. `normalize_umlauts` auch für Suchanfragen verwenden

In `crates/memfuse-text/src/bm25.rs`: Die Query-Tokens vor dem Lookup
durch `normalize_umlauts()` schicken, damit "Änderung" und "aenderung"
beide die gleichen Dokumente finden.

### 4. Neue Unit-Tests schreiben

In `crates/memfuse-text/src/morphology.rs`, im `#[cfg(test)]`-Block,
folgende Tests ergänzen:

```rust
#[test]
fn test_kmu_compounds() {
    let splitter = GermanCompoundSplitter::new();

    // Logistik
    let result = splitter.decompose("lagerbestand");
    assert!(result.len() > 1, "Lagerbestand sollte gesplittet werden");

    // HR
    let result = splitter.decompose("urlaubsantrag");
    assert!(result.len() > 1, "Urlaubsantrag sollte gesplittet werden");

    // Produktion
    let result = splitter.decompose("fertigungssteuerung");
    assert!(result.len() > 1, "Fertigungssteuerung sollte gesplittet werden");
}

#[test]
fn test_umlaut_normalization() {
    assert_eq!(normalize_umlauts("Änderung"), "aenderung");
    assert_eq!(normalize_umlauts("Überprüfung"), "ueberpruefung");
    assert_eq!(normalize_umlauts("Straße"), "strasse");
}

#[test]
fn test_bm25_finds_umlaut_variants() {
    // Indexiert "Änderung", sucht nach "Aenderung" — muss treffen
    // (End-to-End-Test mit InvertedIndex)
}
```

### 5. Verifikation

```bash
cargo test -p memfuse-text 2>&1 | tail -10
```

Alle neuen Tests müssen grün sein.
```

---

## Prompt 7 — Python-Bindings stabilisieren

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: `memfuse-py` stabilisieren — Build, Tests, PyPI-Vorbereitung

Die Python-Bindings in `crates/memfuse-py/` sind aktuell nicht im
Workspace-Build und haben 0 funktionierende Tests.

### 1. `crates/memfuse-py/Cargo.toml` in Workspace-Build integrieren

Stelle sicher, dass alle notwendigen Dependencies korrekt referenziert sind:
```toml
[dependencies]
memfuse-db    = { workspace = true }
memfuse-graph = { workspace = true }
memfuse-core  = { workspace = true }
pyo3          = { version = "0.21", features = ["extension-module", "abi3-py39"] }
numpy         = "0.21"
pythonize     = "0.21"
tokio         = { workspace = true }
serde_json    = { workspace = true }

[lib]
name = "memfuse"
crate-type = ["cdylib"]
```

### 2. `crates/memfuse-py/src/lib.rs` — fehlende Methoden ergänzen

Folgende Python-Methoden müssen exponiert sein (prüfe und ergänze falls fehlend):

```rust
/// Python-Klasse: MemFuseDB
#[pyclass]
pub struct PyMemFuse { ... }

#[pymethods]
impl PyMemFuse {
    /// Öffnet oder erstellt eine Datenbank am gegebenen Pfad.
    #[staticmethod]
    fn open(path: &str, dimension: Option<usize>) -> PyResult<Self> { ... }

    /// Erstellt oder öffnet eine Collection.
    fn collection(&self, name: &str) -> PyResult<PyCollection> { ... }

    /// Listet alle Collections auf.
    fn list_collections(&self) -> PyResult<Vec<String>> { ... }

    /// Löscht eine Collection vollständig.
    fn drop_collection(&self, name: &str) -> PyResult<()> { ... }
}

/// Python-Klasse: Collection
#[pyclass]
pub struct PyCollection { ... }

#[pymethods]
impl PyCollection {
    /// Fügt ein Dokument mit Embedding und optionalen Metadaten ein.
    fn insert(
        &self,
        id: &str,
        embedding: Vec<f32>,
        metadata: Option<&PyAny>,
    ) -> PyResult<()> { ... }

    /// 4-Signal Hybrid-Suche.
    fn hybrid_search(
        &self,
        query: &str,
        embedding: Vec<f32>,
        k: Option<usize>,
    ) -> PyResult<Vec<PySearchResult>> { ... }

    /// Reine Vektor-Suche.
    fn vector_search(
        &self,
        embedding: Vec<f32>,
        k: Option<usize>,
    ) -> PyResult<Vec<PySearchResult>> { ... }

    /// Reine Text-Suche (BM25).
    fn text_search(
        &self,
        query: &str,
        k: Option<usize>,
    ) -> PyResult<Vec<PySearchResult>> { ... }

    /// Holt ein Dokument by ID.
    fn get(&self, id: &str) -> PyResult<Option<PySearchResult>> { ... }

    /// Löscht ein Dokument by ID.
    fn delete(&self, id: &str) -> PyResult<()> { ... }

    /// Gibt die Anzahl der Dokumente in der Collection zurück.
    fn count(&self) -> PyResult<usize> { ... }
}
```

### 3. `crates/memfuse-py/python/memfuse/__init__.py` sauber aufräumen

```python
"""
MemFuse — Eingebettete Hybrid-Search-Engine für LLM-Anwendungen.
"""
from ._memfuse import PyMemFuse as MemFuse, PyCollection as Collection
from .mcp import create_mcp_server

__version__ = "0.1.0-alpha"
__all__ = ["MemFuse", "Collection", "create_mcp_server"]


def open(path: str, dimension: int = 1536) -> MemFuse:
    """Öffnet oder erstellt eine MemFuse-Datenbank.

    Args:
        path: Dateisystempfad für die Datenbank.
        dimension: Vektor-Dimensionen (Standard: 1536 für OpenAI ada-002).

    Returns:
        MemFuse: Geöffnete Datenbankinstanz.
    """
    return MemFuse.open(path, dimension)
```

### 4. Vollständige pytest-Test-Suite schreiben

Ersetze `crates/memfuse-py/tests/test_bindings.py` mit einem vollständigen
Test-File:

```python
"""Integration tests for memfuse Python bindings."""
import pytest
import tempfile
import os
import numpy as np
import memfuse


@pytest.fixture
def db(tmp_path):
    """Öffnet eine temporäre MemFuse-Datenbank."""
    return memfuse.open(str(tmp_path / "test_db"), dimension=4)


@pytest.fixture
def col(db):
    """Erstellt eine Test-Collection."""
    return db.collection("test")


def make_vec(seed: float, dim: int = 4) -> list:
    return [seed + i * 0.01 for i in range(dim)]


class TestBasicOperations:
    def test_open_creates_db(self, tmp_path):
        db = memfuse.open(str(tmp_path / "db"), dimension=4)
        assert db is not None

    def test_insert_and_get(self, col):
        col.insert("doc1", make_vec(0.1), {"text": "Hallo Welt"})
        result = col.get("doc1")
        assert result is not None
        assert result.id == "doc1"

    def test_delete(self, col):
        col.insert("doc1", make_vec(0.1), {"text": "Test"})
        col.delete("doc1")
        result = col.get("doc1")
        assert result is None

    def test_count(self, col):
        assert col.count() == 0
        col.insert("doc1", make_vec(0.1), {"text": "A"})
        col.insert("doc2", make_vec(0.2), {"text": "B"})
        assert col.count() == 2


class TestSearch:
    def test_vector_search_returns_results(self, col):
        col.insert("doc1", make_vec(0.1), {"text": "Maschinenbau"})
        col.insert("doc2", make_vec(0.5), {"text": "Logistik"})
        results = col.vector_search(make_vec(0.1), k=2)
        assert len(results) > 0
        assert results[0].id == "doc1"  # Nächster Vektor zuerst

    def test_text_search_returns_results(self, col):
        col.insert("doc1", make_vec(0.1), {"text": "Urlaubsantrag genehmigt"})
        col.insert("doc2", make_vec(0.2), {"text": "Gehaltsabrechnung Februar"})
        results = col.text_search("Urlaub", k=5)
        assert any(r.id == "doc1" for r in results)

    def test_hybrid_search_returns_results(self, col):
        col.insert("doc1", make_vec(0.1), {"text": "Fertigungssteuerung Anlage"})
        results = col.hybrid_search("Fertigung", make_vec(0.1), k=3)
        assert len(results) > 0

    def test_search_empty_collection(self, col):
        results = col.vector_search(make_vec(0.1), k=5)
        assert results == []


class TestCollections:
    def test_list_collections(self, db):
        db.collection("hr")
        db.collection("finanzen")
        collections = db.list_collections()
        assert "hr" in collections
        assert "finanzen" in collections

    def test_drop_collection(self, db):
        col = db.collection("temp")
        col.insert("doc1", make_vec(0.1), {"text": "Test"})
        db.drop_collection("temp")
        collections = db.list_collections()
        assert "temp" not in collections


class TestPersistence:
    def test_data_survives_reopen(self, tmp_path):
        path = str(tmp_path / "persist_db")
        db1 = memfuse.open(path, dimension=4)
        col1 = db1.collection("data")
        col1.insert("doc1", make_vec(0.1), {"text": "Persistenz-Test"})
        del db1  # Schließen

        db2 = memfuse.open(path, dimension=4)
        col2 = db2.collection("data")
        result = col2.get("doc1")
        assert result is not None
        assert result.id == "doc1"
```

### 5. `pyproject.toml` für PyPI-Vorbereitung aktualisieren

In `crates/memfuse-py/pyproject.toml`:
```toml
[project]
name = "memfuse"
version = "0.1.0a1"
description = "Eingebettete Hybrid-Search-Engine für LLM-Anwendungen (RAG)"
readme = "../../README.md"
license = { text = "MIT OR Apache-2.0" }
keywords = ["rag", "vector-database", "hybrid-search", "llm", "embeddings"]
classifiers = [
    "Development Status :: 3 - Alpha",
    "Intended Audience :: Developers",
    "Topic :: Scientific/Engineering :: Artificial Intelligence",
    "Programming Language :: Python :: 3",
    "Programming Language :: Rust",
]
requires-python = ">=3.9"
dependencies = []

[project.optional-dependencies]
langchain = ["langchain-core>=0.2"]
ingest = ["pypdf2>=3.0", "python-docx>=1.0", "openpyxl>=3.1"]
all = ["memfuse[langchain,ingest]"]
```

### 6. Verifikation (soweit ohne Build-Umgebung möglich)

```bash
cargo check -p memfuse-py 2>&1 | tail -10
```

Alle Rust-Fehler müssen behoben sein. Python-Tests laufen nach
`maturin develop` via `pytest crates/memfuse-py/tests/`.
```

---

## Prompt 8 — Dokumenten-Ingestor für KMU-Formate

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Python-Ingestor für Unternehmens-Dokumentenformate erstellen

KMU haben ihre Dokumente als PDF, Word (DOCX), Excel (XLSX), TXT und in
Ordnerstrukturen. Ohne eine einfache Ingestion-API ist MemFuse für sie
nicht nutzbar.

### 1. Neue Datei erstellen: `crates/memfuse-py/python/memfuse/ingest.py`

```python
"""
memfuse.ingest — Dokumenten-Ingestor für KMU-Formate.

Unterstützte Formate: PDF, DOCX, XLSX, TXT, Markdown.
"""
from __future__ import annotations

import os
import hashlib
from pathlib import Path
from typing import Optional, Iterator
from dataclasses import dataclass

from . import MemFuse, Collection


@dataclass
class Document:
    """Repräsentiert ein zu indizierendes Dokument."""
    id: str
    text: str
    source: str          # Ursprünglicher Dateipfad
    page: Optional[int]  # Seitennummer (bei PDF)
    metadata: dict


class IngestError(Exception):
    """Fehler beim Dokumenten-Einlesen."""
    pass


def _doc_id(path: str, page: Optional[int] = None) -> str:
    """Erstellt eine reproduzierbare ID aus Pfad + Seite."""
    base = f"{path}#{page}" if page is not None else path
    return hashlib.sha256(base.encode()).hexdigest()[:16]


def _extract_txt(path: Path) -> Iterator[Document]:
    """Liest TXT- und Markdown-Dateien."""
    text = path.read_text(encoding="utf-8", errors="replace").strip()
    if text:
        yield Document(
            id=_doc_id(str(path)),
            text=text,
            source=str(path),
            page=None,
            metadata={"filename": path.name, "format": "txt"},
        )


def _extract_pdf(path: Path) -> Iterator[Document]:
    """Liest PDF-Dateien, eine Seite = ein Dokument."""
    try:
        import pypdf
    except ImportError:
        raise IngestError(
            "PDF-Unterstützung erfordert: pip install 'memfuse[ingest]'"
        )
    reader = pypdf.PdfReader(str(path))
    for page_num, page in enumerate(reader.pages, start=1):
        text = page.extract_text() or ""
        text = text.strip()
        if len(text) > 50:  # Leere Seiten überspringen
            yield Document(
                id=_doc_id(str(path), page_num),
                text=text,
                source=str(path),
                page=page_num,
                metadata={
                    "filename": path.name,
                    "format": "pdf",
                    "page": page_num,
                    "total_pages": len(reader.pages),
                },
            )


def _extract_docx(path: Path) -> Iterator[Document]:
    """Liest Word-Dokumente (DOCX)."""
    try:
        import docx
    except ImportError:
        raise IngestError(
            "DOCX-Unterstützung erfordert: pip install 'memfuse[ingest]'"
        )
    doc = docx.Document(str(path))
    paragraphs = [p.text.strip() for p in doc.paragraphs if p.text.strip()]
    text = "\n\n".join(paragraphs)
    if text:
        yield Document(
            id=_doc_id(str(path)),
            text=text,
            source=str(path),
            page=None,
            metadata={"filename": path.name, "format": "docx"},
        )


def _extract_xlsx(path: Path) -> Iterator[Document]:
    """Liest Excel-Dateien, ein Tabellenblatt = ein Dokument."""
    try:
        import openpyxl
    except ImportError:
        raise IngestError(
            "XLSX-Unterstützung erfordert: pip install 'memfuse[ingest]'"
        )
    wb = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
    for sheet_name in wb.sheetnames:
        ws = wb[sheet_name]
        rows = []
        for row in ws.iter_rows(values_only=True):
            cells = [str(c) for c in row if c is not None]
            if cells:
                rows.append(" | ".join(cells))
        text = "\n".join(rows).strip()
        if text:
            yield Document(
                id=_doc_id(str(path), sheet_name),
                text=text,
                source=str(path),
                page=None,
                metadata={
                    "filename": path.name,
                    "format": "xlsx",
                    "sheet": sheet_name,
                },
            )


EXTRACTORS = {
    ".txt": _extract_txt,
    ".md": _extract_txt,
    ".pdf": _extract_pdf,
    ".docx": _extract_docx,
    ".xlsx": _extract_xlsx,
    ".xls": _extract_xlsx,
}


class DocumentIngestor:
    """
    Fügt Unternehmensdokumente in eine MemFuse-Collection ein.

    Beispiel:
        >>> import memfuse
        >>> db = memfuse.open("./firma_db")
        >>> col = db.collection("dokumente")
        >>> ingestor = DocumentIngestor(col, embed_fn=openai_embed)
        >>> ingestor.ingest_folder("./unterlagen/")
        >>> print(f"{ingestor.stats['inserted']} Dokumente indiziert")
    """

    def __init__(
        self,
        collection: Collection,
        embed_fn,  # Callable[[str], list[float]]
        chunk_size: int = 512,
        chunk_overlap: int = 64,
    ):
        """
        Args:
            collection: MemFuse Collection-Instanz.
            embed_fn: Funktion, die Text → Embedding-Vektor umwandelt.
                      Signatur: (text: str) -> list[float]
            chunk_size: Maximale Zeichen pro Chunk.
            chunk_overlap: Überlappung zwischen Chunks (für Kontext-Kontinuität).
        """
        self.collection = collection
        self.embed_fn = embed_fn
        self.chunk_size = chunk_size
        self.chunk_overlap = chunk_overlap
        self.stats = {"inserted": 0, "skipped": 0, "errors": 0}

    def _chunk_text(self, text: str, doc_id: str) -> list[tuple[str, str]]:
        """Teilt langen Text in überlappende Chunks."""
        if len(text) <= self.chunk_size:
            return [(doc_id, text)]

        chunks = []
        start = 0
        chunk_num = 0
        while start < len(text):
            end = min(start + self.chunk_size, len(text))
            chunk_text = text[start:end]
            chunks.append((f"{doc_id}#chunk{chunk_num}", chunk_text))
            chunk_num += 1
            start += self.chunk_size - self.chunk_overlap
        return chunks

    def ingest_file(self, path: str | Path) -> int:
        """
        Indiziert eine einzelne Datei.
        Returns: Anzahl eingefügter Chunks.
        """
        path = Path(path)
        suffix = path.suffix.lower()
        extractor = EXTRACTORS.get(suffix)

        if extractor is None:
            self.stats["skipped"] += 1
            return 0

        inserted = 0
        try:
            for doc in extractor(path):
                for chunk_id, chunk_text in self._chunk_text(doc.text, doc.id):
                    embedding = self.embed_fn(chunk_text)
                    metadata = {**doc.metadata, "text": chunk_text, "source": doc.source}
                    if doc.page:
                        metadata["page"] = doc.page
                    self.collection.insert(chunk_id, embedding, metadata)
                    inserted += 1
            self.stats["inserted"] += inserted
        except Exception as e:
            self.stats["errors"] += 1
            raise IngestError(f"Fehler bei {path}: {e}") from e

        return inserted

    def ingest_folder(
        self,
        folder: str | Path,
        recursive: bool = True,
        extensions: Optional[list[str]] = None,
    ) -> dict:
        """
        Indiziert alle unterstützten Dateien in einem Ordner.

        Args:
            folder: Pfad zum Ordner.
            recursive: Unterordner ebenfalls durchsuchen.
            extensions: Nur diese Endungen verarbeiten (z.B. [".pdf", ".docx"]).
                       None = alle unterstützten Formate.

        Returns:
            Stats-Dictionary: {"inserted": int, "skipped": int, "errors": int}
        """
        folder = Path(folder)
        allowed = set(extensions or EXTRACTORS.keys())
        pattern = "**/*" if recursive else "*"

        for file_path in sorted(folder.glob(pattern)):
            if file_path.is_file() and file_path.suffix.lower() in allowed:
                try:
                    self.ingest_file(file_path)
                except IngestError as e:
                    print(f"⚠️  {e}")

        return dict(self.stats)
```

### 2. `__init__.py` um Ingestor erweitern

In `crates/memfuse-py/python/memfuse/__init__.py` ergänzen:
```python
from .ingest import DocumentIngestor, IngestError

__all__ = ["MemFuse", "Collection", "create_mcp_server", "DocumentIngestor", "IngestError", "open"]
```

### 3. Beispiel-Notebook erstellen

Neue Datei `examples/kmu_quickstart.py`:

```python
"""
MemFuse KMU Quickstart — Firmendokumente mit LLM durchsuchbar machen.

Voraussetzungen:
    pip install memfuse[ingest] openai

Führe aus: python examples/kmu_quickstart.py
"""
import os
import memfuse
from memfuse import DocumentIngestor

# ── 1. Datenbank öffnen ────────────────────────────────────────────────────
db = memfuse.open("./meine_firma_db", dimension=1536)
col = db.collection("dokumente")

# ── 2. Embedding-Funktion definieren (OpenAI-Beispiel) ────────────────────
def embed(text: str) -> list[float]:
    """Wandelt Text in einen Embedding-Vektor um."""
    from openai import OpenAI
    client = OpenAI(api_key=os.environ["OPENAI_API_KEY"])
    response = client.embeddings.create(
        model="text-embedding-ada-002",
        input=text,
    )
    return response.data[0].embedding

# ── 3. Dokumente einlesen ──────────────────────────────────────────────────
ingestor = DocumentIngestor(col, embed_fn=embed)
stats = ingestor.ingest_folder("./unterlagen/", recursive=True)
print(f"✅ {stats['inserted']} Chunks indiziert, {stats['errors']} Fehler")

# ── 4. Fragen stellen ──────────────────────────────────────────────────────
frage = "Was sind unsere Zahlungsbedingungen mit Lieferant Müller GmbH?"
frage_embedding = embed(frage)

results = col.hybrid_search(frage, frage_embedding, k=5)

print(f"\n📄 Top-{len(results)} relevante Stellen:\n")
for i, r in enumerate(results, 1):
    source = r.metadata.get("source", "Unbekannt")
    text_preview = r.metadata.get("text", "")[:200]
    print(f"{i}. [{r.score:.3f}] {source}")
    print(f"   {text_preview}...\n")
```

### 4. Tests für den Ingestor schreiben

Neue Datei `crates/memfuse-py/tests/test_ingest.py`:

```python
"""Tests für den DocumentIngestor."""
import pytest
import tempfile
from pathlib import Path
import memfuse
from memfuse.ingest import DocumentIngestor, _extract_txt


def dummy_embed(text: str) -> list[float]:
    """Deterministisches Dummy-Embedding für Tests."""
    return [hash(text[:10]) % 100 / 100.0] * 4


@pytest.fixture
def col(tmp_path):
    db = memfuse.open(str(tmp_path / "test"), dimension=4)
    return db.collection("ingest_test")


def test_ingest_txt_file(col, tmp_path):
    txt_file = tmp_path / "test.txt"
    txt_file.write_text("Urlaubsantrag von Mitarbeiter Schmidt genehmigt.")
    ingestor = DocumentIngestor(col, embed_fn=dummy_embed)
    count = ingestor.ingest_file(txt_file)
    assert count == 1
    results = col.text_search("Urlaubsantrag", k=5)
    assert len(results) > 0


def test_ingest_folder(col, tmp_path):
    (tmp_path / "a.txt").write_text("Dokument A: Lagerbericht Januar")
    (tmp_path / "b.txt").write_text("Dokument B: Gehaltsabrechnung März")
    (tmp_path / "ignored.xyz").write_text("Ignorierte Datei")
    ingestor = DocumentIngestor(col, embed_fn=dummy_embed)
    stats = ingestor.ingest_folder(tmp_path, recursive=False)
    assert stats["inserted"] == 2
    assert stats["skipped"] == 1


def test_ingest_skips_unknown_formats(col, tmp_path):
    (tmp_path / "test.xyz").write_text("Unbekanntes Format")
    ingestor = DocumentIngestor(col, embed_fn=dummy_embed)
    stats = ingestor.ingest_folder(tmp_path)
    assert stats["inserted"] == 0
    assert stats["skipped"] == 1
```

### 5. Verifikation

```bash
# Python-Syntax prüfen
python -m py_compile crates/memfuse-py/python/memfuse/ingest.py && echo "OK"
```
```

---

## Prompt 9 — LangChain-Integration

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: LangChain-Integration als optionales Python-Modul implementieren

LangChain ist der de-facto Standard für LLM-Anwendungen. Eine fertige
Integration reduziert die Einstiegshürde für KMU-Entwickler drastisch.

### 1. Neue Datei erstellen: `crates/memfuse-py/python/memfuse/langchain.py`

```python
"""
memfuse.langchain — LangChain-Integration für MemFuse.

Voraussetzungen: pip install 'memfuse[langchain]'

Beispiel:
    from memfuse.langchain import MemFuseVectorStore
    from langchain_openai import OpenAIEmbeddings

    embeddings = OpenAIEmbeddings()
    store = MemFuseVectorStore.from_texts(
        texts=["Text A", "Text B"],
        embedding=embeddings,
        db_path="./firma_db",
        collection_name="langchain",
    )
    retriever = store.as_retriever(search_kwargs={"k": 5})
"""
from __future__ import annotations

from typing import Any, Dict, Iterable, List, Optional, Tuple, Type

try:
    from langchain_core.documents import Document
    from langchain_core.embeddings import Embeddings
    from langchain_core.vectorstores import VectorStore
    _LANGCHAIN_AVAILABLE = True
except ImportError:
    _LANGCHAIN_AVAILABLE = False

import memfuse as _memfuse


def _require_langchain():
    if not _LANGCHAIN_AVAILABLE:
        raise ImportError(
            "LangChain-Integration erfordert: pip install 'memfuse[langchain]'"
        )


class MemFuseVectorStore:
    """
    LangChain-kompatibler VectorStore auf Basis von MemFuse.

    Implementiert die LangChain VectorStore-Schnittstelle und nutzt
    MemFuse's 4-Signal Hybrid-Suche für überlegene Retrieval-Qualität.
    """

    def __init__(
        self,
        db_path: str,
        embedding: "Embeddings",
        collection_name: str = "langchain",
        dimension: int = 1536,
    ):
        _require_langchain()
        self._db = _memfuse.open(db_path, dimension=dimension)
        self._collection = self._db.collection(collection_name)
        self._embedding = embedding

    # ── Dokumente hinzufügen ──────────────────────────────────────────────

    def add_texts(
        self,
        texts: Iterable[str],
        metadatas: Optional[List[dict]] = None,
        ids: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> List[str]:
        """Fügt Texte mit optionalen Metadaten ein."""
        texts = list(texts)
        embeddings = self._embedding.embed_documents(texts)
        metadatas = metadatas or [{} for _ in texts]

        inserted_ids = []
        for i, (text, embedding, meta) in enumerate(
            zip(texts, embeddings, metadatas)
        ):
            doc_id = ids[i] if ids else f"doc_{i}_{hash(text) % 10000}"
            full_meta = {**meta, "text": text}
            self._collection.insert(doc_id, embedding, full_meta)
            inserted_ids.append(doc_id)

        return inserted_ids

    def add_documents(
        self,
        documents: List["Document"],
        **kwargs: Any,
    ) -> List[str]:
        """Fügt LangChain-Dokumente ein."""
        texts = [doc.page_content for doc in documents]
        metadatas = [doc.metadata for doc in documents]
        return self.add_texts(texts, metadatas=metadatas, **kwargs)

    # ── Suche ─────────────────────────────────────────────────────────────

    def similarity_search(
        self,
        query: str,
        k: int = 4,
        **kwargs: Any,
    ) -> List["Document"]:
        """4-Signal Hybrid-Suche (Vektor + BM25 + Graph + Metadaten)."""
        query_embedding = self._embedding.embed_query(query)
        results = self._collection.hybrid_search(query, query_embedding, k=k)
        return [
            Document(
                page_content=r.metadata.get("text", ""),
                metadata={**r.metadata, "score": r.score, "id": r.id},
            )
            for r in results
        ]

    def similarity_search_with_score(
        self,
        query: str,
        k: int = 4,
        **kwargs: Any,
    ) -> List[Tuple["Document", float]]:
        """Suche mit Relevanz-Scores."""
        docs = self.similarity_search(query, k=k)
        return [(doc, doc.metadata.get("score", 0.0)) for doc in docs]

    def as_retriever(self, **kwargs: Any):
        """Gibt einen LangChain-Retriever zurück."""
        from langchain_core.vectorstores import VectorStoreRetriever
        return VectorStoreRetriever(vectorstore=self, **kwargs)

    # ── Factory-Methoden ──────────────────────────────────────────────────

    @classmethod
    def from_texts(
        cls,
        texts: List[str],
        embedding: "Embeddings",
        db_path: str = "./memfuse_db",
        collection_name: str = "langchain",
        dimension: int = 1536,
        metadatas: Optional[List[dict]] = None,
        **kwargs: Any,
    ) -> "MemFuseVectorStore":
        """Erstellt einen VectorStore und fügt Texte direkt ein."""
        store = cls(
            db_path=db_path,
            embedding=embedding,
            collection_name=collection_name,
            dimension=dimension,
        )
        store.add_texts(texts, metadatas=metadatas)
        return store

    @classmethod
    def from_documents(
        cls,
        documents: List["Document"],
        embedding: "Embeddings",
        db_path: str = "./memfuse_db",
        collection_name: str = "langchain",
        **kwargs: Any,
    ) -> "MemFuseVectorStore":
        """Erstellt einen VectorStore aus LangChain-Dokumenten."""
        store = cls(db_path=db_path, embedding=embedding,
                    collection_name=collection_name, **kwargs)
        store.add_documents(documents)
        return store
```

### 2. `__init__.py` aktualisieren

In `crates/memfuse-py/python/memfuse/__init__.py`:
```python
# Lazy import — nur wenn langchain installiert ist
def get_langchain_store():
    """Gibt MemFuseVectorStore zurück (erfordert pip install memfuse[langchain])."""
    from .langchain import MemFuseVectorStore
    return MemFuseVectorStore
```

### 3. LangChain-Beispiel erstellen

Neue Datei `examples/langchain_rag.py`:

```python
"""
MemFuse + LangChain RAG-Pipeline für KMU.

Erstellt einen vollständigen RAG-Assistenten, der interne Firmendokumente
versteht und Fragen auf Deutsch beantwortet.

Voraussetzungen:
    pip install memfuse[langchain,ingest] langchain-openai langchain

Verwendung:
    OPENAI_API_KEY=... python examples/langchain_rag.py
"""
import os
from pathlib import Path

from langchain_openai import ChatOpenAI, OpenAIEmbeddings
from langchain_core.prompts import ChatPromptTemplate
from langchain_core.runnables import RunnablePassthrough
from langchain_core.output_parsers import StrOutputParser

from memfuse.langchain import MemFuseVectorStore
from memfuse import DocumentIngestor
import memfuse

# ── Konfiguration ──────────────────────────────────────────────────────────
DB_PATH = "./firma_rag_db"
DOCS_FOLDER = "./unterlagen"  # Hier liegen die Firmendokumente
COLLECTION = "firma_wissen"

# ── Initialisierung ────────────────────────────────────────────────────────
embeddings = OpenAIEmbeddings(model="text-embedding-ada-002")
llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)

# ── Dokumente indizieren (nur beim ersten Start nötig) ─────────────────────
db = memfuse.open(DB_PATH)
col = db.collection(COLLECTION)

if col.count() == 0:
    print("🔄 Indiziere Dokumente...")
    ingestor = DocumentIngestor(col, embed_fn=embeddings.embed_query)
    stats = ingestor.ingest_folder(DOCS_FOLDER)
    print(f"✅ {stats['inserted']} Chunks indiziert")

# ── LangChain VectorStore ──────────────────────────────────────────────────
vector_store = MemFuseVectorStore(
    db_path=DB_PATH,
    embedding=embeddings,
    collection_name=COLLECTION,
)
retriever = vector_store.as_retriever(search_kwargs={"k": 5})

# ── RAG-Prompt ─────────────────────────────────────────────────────────────
SYSTEM_PROMPT = """Du bist ein hilfreicher Unternehmensassistent.
Beantworte Fragen ausschließlich auf Basis der bereitgestellten Kontext-Dokumente.
Antworte auf Deutsch. Wenn die Antwort im Kontext nicht zu finden ist,
sage ehrlich: "Diese Information liegt mir nicht vor."

Kontext:
{context}
"""

prompt = ChatPromptTemplate.from_messages([
    ("system", SYSTEM_PROMPT),
    ("human", "{question}"),
])

# ── RAG-Chain ──────────────────────────────────────────────────────────────
def format_docs(docs):
    return "\n\n---\n\n".join(
        f"[Quelle: {doc.metadata.get('source', 'Unbekannt')}]\n{doc.page_content}"
        for doc in docs
    )

rag_chain = (
    {"context": retriever | format_docs, "question": RunnablePassthrough()}
    | prompt
    | llm
    | StrOutputParser()
)

# ── Interaktive Session ────────────────────────────────────────────────────
print("\n🤖 Firmen-Assistent bereit. Strg+C zum Beenden.\n")
while True:
    try:
        frage = input("❓ Ihre Frage: ").strip()
        if not frage:
            continue
        antwort = rag_chain.invoke(frage)
        print(f"\n💬 {antwort}\n")
    except KeyboardInterrupt:
        print("\nAuf Wiedersehen!")
        break
```

### 4. Tests für LangChain-Integration

Neue Datei `crates/memfuse-py/tests/test_langchain.py`:

```python
"""Tests für die LangChain-Integration."""
import pytest


def test_langchain_import_error_without_langchain():
    """Ohne langchain installiert soll ein klarer Fehler kommen."""
    import sys
    # Simuliere fehlende Installation durch temporäres Verstecken
    import memfuse.langchain as lc
    # Wenn langchain nicht da ist, soll from_texts einen ImportError werfen
    # (Test nur aussagekräftig in Umgebungen ohne langchain)


def test_memfuse_vector_store_basic(tmp_path):
    """Grundlegender Test ohne echte LangChain-Embeddings."""
    pytest.importorskip("langchain_core")

    import memfuse
    from memfuse.langchain import MemFuseVectorStore

    def mock_embed(texts):
        return [[0.1, 0.2, 0.3, 0.4]] * len(texts)

    class MockEmbeddings:
        def embed_documents(self, texts): return mock_embed(texts)
        def embed_query(self, text): return [0.1, 0.2, 0.3, 0.4]

    store = MemFuseVectorStore.from_texts(
        texts=["Urlaubsantrag Müller", "Gehaltsabrechnung Schmidt"],
        embedding=MockEmbeddings(),
        db_path=str(tmp_path / "lc_db"),
        dimension=4,
    )
    results = store.similarity_search("Urlaub", k=2)
    assert len(results) > 0
    assert isinstance(results[0].page_content, str)
```

### 5. Verifikation

```bash
python -m py_compile crates/memfuse-py/python/memfuse/langchain.py && echo "OK"
python -m py_compile examples/langchain_rag.py && echo "OK"
```
```

---

## Prompt 10 — README & Dokumentation für KMU-Zielgruppe

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: README und Dokumentation für KMU-Zielgruppe komplett überarbeiten

Das aktuelle README spricht Agentenentwickler mit Militärvokabular an.
Die neue Zielgruppe sind KMU-Entwickler, die LLMs in ihre Unternehmensprozesse
integrieren wollen.

### 1. `README.md` komplett ersetzen

Ersetze den gesamten Inhalt von `README.md` mit folgendem:

---

```markdown
# MemFuse — Die lokale RAG-Engine für Unternehmens-LLMs

MemFuse macht Ihre Firmendaten für KI-Assistenten durchsuchbar —  
lokal, sicher, ohne Cloud, DSGVO-konform.

> ⚠️ **Status: Alpha (v0.1.0-alpha)** — Kern-Funktionalität läuft stabil.
> Produktionseinsatz nach eigenem Ermessen und Testabdeckung.

## Das Problem, das MemFuse löst

Ihr Unternehmen hat Wissen in PDFs, Word-Dokumenten, ERP-Exporten und E-Mails.
ChatGPT kennt dieses Wissen nicht. RAG (Retrieval-Augmented Generation) ist die
Lösung — aber bestehende Systeme sind entweder zu komplex, zu teuer oder schicken
Ihre Daten in die Cloud.

**MemFuse ist anders:**
- Läuft komplett lokal auf Ihrem Server — keine Daten verlassen Ihr Netz
- Kein Docker, kein separater Server, kein Cloud-Account
- Eingebaut: Verschlüsselung, ACID-Transaktionen, deutsche Morphologie

## Schnellstart (Python)

```bash
pip install memfuse[ingest]
```

```python
import memfuse
from memfuse import DocumentIngestor
from openai import OpenAI

# Embedding-Funktion (OpenAI-Beispiel — jedes Embedding-Modell funktioniert)
client = OpenAI()
def embed(text):
    return client.embeddings.create(
        model="text-embedding-ada-002", input=text
    ).data[0].embedding

# Datenbank öffnen und Dokumente einlesen
db = memfuse.open("./firma_db")
col = db.collection("alle_dokumente")

ingestor = DocumentIngestor(col, embed_fn=embed)
stats = ingestor.ingest_folder("./meine_unterlagen/")  # PDF, DOCX, XLSX, TXT
print(f"{stats['inserted']} Abschnitte indiziert")

# Fragen stellen
frage = "Was sind unsere Zahlungsbedingungen?"
ergebnisse = col.hybrid_search(frage, embed(frage), k=5)
for r in ergebnisse:
    print(f"[{r.score:.2f}] {r.metadata['source']}: {r.metadata['text'][:100]}...")
```

## LangChain-Integration

```python
from memfuse.langchain import MemFuseVectorStore
from langchain_openai import OpenAIEmbeddings

embeddings = OpenAIEmbeddings()
store = MemFuseVectorStore.from_documents(
    documents=meine_langchain_docs,
    embedding=embeddings,
    db_path="./firma_db",
)
retriever = store.as_retriever(search_kwargs={"k": 5})
# Ab hier: Standard LangChain RAG-Chain
```

## Warum MemFuse für Ihr Unternehmen?

| | MemFuse | ChromaDB | Qdrant | Weaviate |
|---|---|---|---|---|
| Kein Server nötig | ✅ | ✅ | ❌ | ❌ |
| 4-Signal-Suche¹ | ✅ | ❌ | ❌ | ❌ |
| DSGVO-konform (local-first) | ✅ | ✅ | ✅² | ✅² |
| Eingebaute Verschlüsselung | ✅ | ❌ | ❌ | ❌ |
| Deutsche Morphologie | ✅ | ❌ | ❌ | ❌ |
| ACID-Transaktionen | ✅ | ❌ | ✅ | ❌ |

¹ Vektor + BM25-Volltext + Wissensgraph + Metadaten-Filter  
² Selbst gehostete Version erforderlich

## Unterstützte Dokumentenformate

- **PDF** — Berichte, Verträge, Produktdokumentationen
- **DOCX** — Word-Dokumente, Handbücher, Protokolle
- **XLSX** — Preislisten, Stücklisten, Auswertungen
- **TXT / Markdown** — Wikis, Notizen, README-Dateien

## Architektur

MemFuse kombiniert vier Suchsignale über Reciprocal Rank Fusion (RRF):

```
Ihre Anfrage
    ├── Vektor-Suche (HNSW, SIMD-beschleunigt)
    ├── Volltext-Suche (BM25, deutsche Morphologie)
    ├── Wissensgraph (Entitäts-Beziehungen, CSR)
    └── Metadaten-Filter (strukturierte Felder)
              ↓
    Reciprocal Rank Fusion
              ↓
    Top-K relevante Dokument-Abschnitte
              ↓
    LLM-Kontext (LangChain / eigene Pipeline)
```

## Installation

### Python (empfohlen für LLM-Projekte)
```bash
pip install memfuse                    # Basis
pip install memfuse[ingest]            # + PDF/DOCX/XLSX-Unterstützung
pip install memfuse[langchain]         # + LangChain VectorStore
pip install memfuse[all]               # Alles
```

### Rust
```toml
[dependencies]
memfuse-db = "0.1.0-alpha"
```

## Beispiele

- [`examples/kmu_quickstart.py`](examples/kmu_quickstart.py) — Dokumente indizieren und durchsuchen
- [`examples/langchain_rag.py`](examples/langchain_rag.py) — Vollständige LangChain RAG-Pipeline

## Technische Details

Für Entwickler und technisch Interessierte:
- [Architektur](docs/ARCHITECTURE.md) — Schichtmodell und Invarianten
- [Entwickler-Guide](DEVELOPERS.md) — Build-Setup, Tests, Contribution
- [Sicherheitskonzept](SECURITY.md) — Verschlüsselung, Namespaces

## Lizenz

MIT OR Apache-2.0
```
---

### 2. `docs/ARCHITECTURE.md` — Militärvokabular entfernen

In `docs/ARCHITECTURE.md`:
- Entferne alle Referenzen auf `SAOS`, `airgap`, `sovereign` aus
  dem Fließtext (Kommentare im Code dürfen bleiben)
- Ersetze "Agent-Memory-Library" durch "RAG-Engine für Unternehmensanwendungen"
- Aktualisiere die Crate-Liste: `memfuse-graph` und `memfuse-py` sind jetzt aktiv

### 3. `GLOSSARY.md` — KMU-Begriffe ergänzen

Am Ende von `GLOSSARY.md` einen neuen Abschnitt ergänzen:

```markdown
## KMU-Domänenbegriffe

| Begriff | Definition |
|---|---|
| **RAG** | Retrieval-Augmented Generation — Technik, bei der ein LLM mit relevanten Dokumenten aus einer Datenbank angereichert wird, bevor es antwortet. |
| **Chunk** | Abschnitt eines langen Dokuments, das für die Indizierung in kleinere Teile aufgeteilt wurde. |
| **Ingestor** | Python-Komponente (`DocumentIngestor`), die Unternehmensdokumente liest und in MemFuse indiziert. |
| **Embedding** | Numerische Vektordarstellung eines Textes, die semantische Ähnlichkeit erfassbar macht. |
| **4-Signal-Fusion** | MemFuse's kombinierter Suchmodus: Vektor + BM25 + Graph + Metadaten via RRF. |
```

### 4. `docs/SOURCE_OF_TRUTH.md` aktualisieren

Füge am Anfang von `docs/SOURCE_OF_TRUTH.md` folgenden Status-Block ein:

```markdown
## Aktueller Projektstatus (nach KMU-Pivot)

**Ausrichtung**: Lokale RAG-Engine für mittelständische Unternehmen (KMU)  
**Zielgruppe**: Python-Entwickler in KMU, die LLMs auf Unternehmensdaten anwenden  
**Alleinstellungsmerkmal**: 4-Signal Hybrid-Suche (Vektor+BM25+Graph+Meta), lokal, verschlüsselt, DSGVO-konform

### Aktive Crates (9)
- `memfuse-core` — Typen, Traits, Fehler
- `memfuse-store` — LSM-Tree, WAL, SSTables
- `memfuse-index` — HNSW, SIMD
- `memfuse-text` — BM25, Deutsche Morphologie
- `memfuse-crypto` — AES-GCM-SIV
- `memfuse-checkpoint` — Checkpoint-Management
- `memfuse-graph` — CSR-Graph (persistiert) ← reaktiviert
- `memfuse-db` — 4-Signal Fusion, Collections API
- `memfuse-py` — Python-Bindings, Ingestor, LangChain ← reaktiviert

### Archived Crates (nicht im Build)
- `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent`, `memfuse-embed`
```

### 5. Verifikation

```bash
# Markdown-Syntax prüfen (optional, wenn markdownlint installiert)
# markdownlint README.md

# Stellt sicher, dass alle verlinkten Dateien existieren
grep -oP '\[.*?\]\(\K[^)]+' README.md | while read f; do
    [ -f "$f" ] || echo "⚠️  Fehlende Datei: $f"
done
```

Nach diesem Prompt ist das Repository vollständig umgebaut.
```

---

## Ausführungsreihenfolge & Hinweise für Jules

1. **Jeden Prompt einzeln ausführen** — nach jedem Prompt committet Jules die Änderungen.
2. **Prompt 5 hängt von Prompt 3 ab** (StorageEngine-Trait-Erweiterungen).
3. **Prompts 8 und 9 hängen von Prompt 7 ab** (Python-Bindings müssen laufen).
4. **Prompt 10 kann parallel zu 6–9 laufen** (nur Dokumentation, kein Code).
5. Falls Jules bei Prompt 5 die `scan_prefix()`-Implementierung nicht abschließt, kann Prompt 7 sie in Python simulieren (langsamer aber funktional).

**Gesamtaufwand geschätzt**: 8–14 Stunden Jules-Laufzeit, abhängig von Codebase-Komplexität.
