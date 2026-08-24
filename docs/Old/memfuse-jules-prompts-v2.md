# MemFuse Brain: Google Jules Prompt-Sequenz (v2 — Tauri Enterprise Pivot)

> **Aktualisiert** auf Basis der Senior-Architekt-Analyse vom 22.08.2026 und dem
> aktuellen Repo-Stand (`memfuse-sandbox` und `memfuse-saos-agent` sind bereits
> aus dem Repo entfernt — dieser Schritt ist erledigt).
>
> **Neue Ausrichtung**: Nicht mehr "Python-Bibliothek für Entwickler", sondern
> **MemFuse Brain** — eine Tauri-Desktop-App für KMU: lokal, air-gapped,
> mit Ollama als LLM-Backend. "GPT4All für Unternehmen mit professionellem Gehirn."
>
> Repository: `https://github.com/tfufuz1/memfuse`  
> Jeder Prompt wird **einzeln und nacheinander** in Google Jules ausgeführt.

---

## Was sich gegenüber der alten Prompt-Sequenz ändert

| Alt (v1) | Neu (v2) | Grund |
|---|---|---|
| Python-Bindings + LangChain als Hauptprodukt | Tauri-Desktop-App als Hauptprodukt | Zero-IT-Setup für KMU — kein `pip install` nötig |
| CVE-Patches als Prompt 2 | Entfällt — `lru`/`memmap2` nicht mehr als kritisch bestätigt, wird in Prompt 2 nur re-verifiziert | Analyse fand andere, dringendere Bugs |
| Deutsche Morphologie als Prompt 6 | Bleibt, aber später (Prompt 8) | Der Graph-USP hat jetzt Priorität 1 |
| — | **Graph MUSS in `hybrid_search()` integriert werden** (fehlte in v1 komplett) | Analyse zeigt: Graph wird aktuell nirgends abgefragt — das ist schlimmer als reine Nicht-Persistenz |
| — | **README-Bug beheben** (`create_collection` existiert nicht) | Jeder Rust-Nutzer scheitert sofort |
| — | **Checkpoint-Test-Fix** als Sofortmaßnahme | `cargo test` ist aktuell rot |
| — | **Tauri-Shell, Ollama-Bridge, Ingestion-Pipeline** | Neue Kernarchitektur laut Strategie |
| PyPI/LangChain als Vertriebsweg | Native Installer (Windows/macOS/Linux) als Hauptvertrieb, Python/Rust-Crates als Nebenkanal | Zielgruppe sind Sachbearbeiter, keine Entwickler |

---

## Übersicht der neuen Sequenz

| # | Aufgabe | Priorität | Abhängigkeit |
|---|---|---|---|
| 1 | Sofort-Fixes: RwLock, Checkpoint-Test, README-Bug | Kritisch | — |
| 2 | `delete_prefix()` + `scan_prefix()` im StorageEngine-Trait | Kritisch | Prompt 1 |
| 3 | Graph-Persistenz implementieren (FIND-GRA-001) | Kritisch | Prompt 2 |
| 4 | Graph in `hybrid_search()` integrieren — echtes 3-Signal-RRF | Kritisch | Prompt 3 |
| 5 | FIND-STO-001 (Compaction-Tombstones) + FIND-DB-002 (drop_collection) | Hoch | Prompt 2 |
| 6 | `memfuse-tauri` Crate aufsetzen (Grundgerüst) | Hoch | Prompt 4 |
| 7 | Ingestion-Pipeline (PDF/DOCX/MD/E-Mail) | Hoch | Prompt 6 |
| 8 | Deutsche Morphologie ausbauen (KMU-Vokabular) | Mittel | — (parallel möglich) |
| 9 | Ollama-Bridge (Chat mit RAG-Streaming) | Hoch | Prompt 7 |
| 10 | Tauri-Commands (IPC-Schicht) + Frontend-Grundgerüst | Hoch | Prompt 9 |
| 11 | Echter MCP-Server (axum/SSE statt JSON-Stub) | Mittel | Prompt 4 |
| 12 | Dokumentation & README für Enterprise-Zielgruppe | Niedrig | Alle vorherigen |

---

## Prompt 1 — Sofort-Fixes: RwLock-Panic-Risiko, Checkpoint-Test, README-Bug

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Kritische Sofort-Fixes (Show-Stopper beheben)

Eine Code-Analyse hat drei kritische Probleme identifiziert, die vor jeder
Weiterentwicklung behoben werden müssen.

### 1. `std::sync::RwLock` durch `parking_lot::RwLock` ersetzen (Panic-Risiko)

Betroffene Stellen (alle bestätigt):

**`crates/memfuse-db/src/collection.rs`**:
- Zeile 59: `pub(crate) embedder: std::sync::RwLock<Option<Arc<TextEmbedder>>>,`
- Zeile 73-74: `std::sync::RwLock::new(self.embedder.read().unwrap()...)`
- Zeile 106: `embedder: std::sync::RwLock::new(None),`
- Zeile 115: `let mut guard = self.embedder.write().unwrap();`
- Zeile 125: `let mut guard = self.embedder.write().unwrap();`
- Zeile 285: `let embedder_guard = self.embedder.read().unwrap();`
- Zeile 318: `let embedder_guard = self.embedder.read().unwrap();`
- Zeile 821: `let embedder_guard = self.embedder.read().unwrap();`

**`crates/memfuse-db/src/lib.rs`**:
- Zeile 140: `embedder: std::sync::RwLock<Option<Arc<TextEmbedder>>>,`
- Zeile 172: `embedder: std::sync::RwLock::new(None),`
- Zeile 351: `if let Some(emb) = self.embedder.read().unwrap().as_ref() {`
- Zeile 754, 760, 768, 781, 786: `let mut guard = ....embedder.write().unwrap();`

**Vorgehen:**
1. In `crates/memfuse-db/Cargo.toml` sicherstellen, dass `parking_lot` als
   Dependency vorhanden ist: `parking_lot = { workspace = true }`
2. In `collection.rs` und `lib.rs`: `use std::sync::RwLock;` bzw. den
   vollqualifizierten Typ `std::sync::RwLock<...>` durch `parking_lot::RwLock<...>`
   ersetzen.
3. `parking_lot::RwLock` hat kein Poison-Konzept — `.read()` und `.write()`
   geben den Guard direkt zurück (keine `Result`, kein `.unwrap()` nötig):
   ```rust
   // Alt:
   let guard = self.embedder.read().unwrap();
   // Neu:
   let guard = self.embedder.read();
   ```
4. Konstruktoren anpassen: `std::sync::RwLock::new(x)` → `parking_lot::RwLock::new(x)`

### 2. Checkpoint-Test API-Mismatch reparieren

**Datei**: `crates/memfuse-checkpoint/tests/concurrency.rs`, Zeile 76

Aktueller fehlerhafter Aufruf:
```rust
m.create_checkpoint("same_name", "coll", i as u64, serde_json::json!({}))
```

Prüfe die tatsächliche Signatur von `create_checkpoint()` in
`crates/memfuse-checkpoint/src/lib.rs` (oder `manager.rs`). Sie erwartet
vermutlich einen zusätzlichen `TxId`-Parameter. Passe den Testaufruf an die
echte Signatur an — füge den fehlenden Parameter mit einem sinnvollen
Testwert hinzu (z.B. `i as u64` für die TxId, falls die Signatur lautet
`create_checkpoint(name, collection, tx_id, seq_no, metadata)` oder ähnlich).

Prüfe dabei ALLE Aufrufe von `create_checkpoint()` im gesamten Testverzeichnis
`crates/memfuse-checkpoint/tests/` auf die gleiche Inkonsistenz und korrigiere
sie einheitlich.

### 3. README API-Mismatch korrigieren

**Datei**: `README.md`

Suche nach:
```rust
let col = db.create_collection("agents", 1536).await?;
```

Diese Methode existiert nicht im Code. Die tatsächliche API ist:
```rust
let col = db.collection("agents").await?;
```

Ersetze ALLE Vorkommen von `create_collection(` im README durch die korrekte
`collection(`-Methode. Prüfe dabei, ob die Dimension über `MemFuseConfig`
beim Öffnen der Datenbank gesetzt wird, und passe das Beispiel entsprechend an
(z.B. `MemFuse::open_with_config(path, MemFuseConfig { dimension: 1536, .. })`).
Schaue in `crates/memfuse-db/src/lib.rs` nach der echten `MemFuse::open()`- bzw.
`MemFuseConfig`-Signatur und verwende exakt diese im README-Beispiel.

### 4. Verifikation

```bash
cargo build --workspace 2>&1 | tail -20
cargo test -p memfuse-checkpoint 2>&1 | tail -20
cargo test -p memfuse-db 2>&1 | tail -20
grep -rn "std::sync::RwLock" crates/memfuse-db/src/
```

Die letzte Zeile darf KEINE Treffer mehr ausgeben. Alle Tests müssen grün sein.
```

---

## Prompt 2 — StorageEngine-Trait erweitern: `delete_prefix()` und `scan_prefix()`

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Fehlende Trait-Methoden ergänzen (Grundlage für Graph-Persistenz und drop_collection-Fix)

Zwei nachfolgende Bugfixes (Graph-Persistenz, `drop_collection`) benötigen
Methoden, die im `StorageEngine`-Trait aktuell fehlen.

### 1. `delete_prefix()` zum Trait hinzufügen

**Datei**: `crates/memfuse-core/src/traits.rs`

Füge im `StorageEngine`-Trait folgende Methode hinzu:
```rust
/// Löscht alle Key-Value-Paare, deren Key mit `prefix` beginnt.
/// Wird u.a. für `drop_collection()` und Graph-Cleanup benötigt.
async fn delete_prefix(&self, tx: TxId, prefix: &[u8]) -> Result<u64>;
// Rückgabewert: Anzahl gelöschter Keys
```

Prüfe die exakte Signatur-Konvention der bestehenden Methoden im Trait
(z.B. ob `tx: TxId` als erster Parameter Konvention ist, wie bei `delete()`
oder `put()`) und übernimm diese Konvention exakt.

### 2. `scan_prefix()` zum Trait hinzufügen (falls noch nicht vorhanden)

Prüfe zuerst, ob `scan_prefix()` oder `scan_prefix_at()` bereits im Trait
existiert (die Analyse erwähnt `scan_prefix_at()` als MVCC-Methode). Falls
eine MVCC-fähige Variante bereits existiert, verwende diese als Vorlage und
stelle sicher, dass eine einfache Nicht-MVCC-Variante ebenfalls verfügbar ist:

```rust
/// Scannt alle Key-Value-Paare mit dem gegebenen Prefix (aktuelle Sicht,
/// keine MVCC-Snapshot-Isolation).
async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
```

### 3. Implementierung in `LsmStorage`

**Datei**: `crates/memfuse-store/src/lsm.rs`

Implementiere `delete_prefix()`:
```rust
async fn delete_prefix(&self, tx: TxId, prefix: &[u8]) -> Result<u64> {
    let matching_keys = self.scan_prefix(prefix).await?;
    let mut deleted = 0u64;
    for (key, _) in matching_keys {
        self.delete(tx, &key).await?;
        deleted += 1;
    }
    Ok(deleted)
}
```

Falls `scan_prefix()` noch nicht existiert, implementiere sie durch Iteration
über die aktive MemTable, alle immutable MemTables und alle SSTables
(newest-first, wie im bestehenden Read-Path dokumentiert), gefiltert nach
Prefix und unter Ausschluss von Tombstones.

Orientiere dich am bestehenden `get()`-Read-Path in derselben Datei für die
korrekte Iterationsreihenfolge über die Speicherschichten.

### 4. Default-Implementierung für andere StorageEngine-Implementierungen

Falls es weitere `impl StorageEngine for ...`-Blöcke im Workspace gibt
(z.B. Test-Mocks), stelle sicher, dass auch diese `delete_prefix()`
implementieren — notfalls über eine naive Fallback-Implementierung
(Iteration + Einzel-`delete()`-Aufrufe wie oben).

### 5. Verifikation

```bash
cargo build --workspace 2>&1 | tail -20
cargo test -p memfuse-core -p memfuse-store 2>&1 | tail -20
```

Schreibe zusätzlich einen Test in `crates/memfuse-store/tests/` (oder
passendem bestehenden Testfile):
```rust
#[tokio::test]
async fn test_delete_prefix_removes_all_matching_keys() {
    // 1. Mehrere Keys mit gemeinsamem Prefix "test:" einfügen
    // 2. delete_prefix("test:") aufrufen
    // 3. Prüfen: alle "test:*"-Keys sind weg, andere Keys bleiben unberührt
}
```
```

---

## Prompt 3 — Graph-Persistenz implementieren (FIND-GRA-001)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: CSR-Graph persistent machen — der zentrale USP-Fix

Dies ist der wichtigste einzelne Schritt der gesamten Neuausrichtung. Laut
Code-Analyse steht wörtlich in `crates/memfuse-graph/src/csr.rs` Zeile 331:

```rust
// CSR Graph currently does not persist state across restarts or support physical rollback
```

Der Graph lebt ausschließlich im RAM. Diese Aufgabe behebt das vollständig,
unter Nutzung des in Prompt 2 hinzugefügten `scan_prefix()`.

### 1. `crates/memfuse-graph/Cargo.toml` — Dependencies prüfen/ergänzen

```toml
[dependencies]
memfuse-core = { workspace = true }
parking_lot = { workspace = true }
serde = { workspace = true }
bincode = { workspace = true }
async-trait = { workspace = true }
tracing = { workspace = true }
```

Falls `memfuse-store` als konkreter Typ gebraucht wird (statt generisch über
den `StorageEngine`-Trait), diese Dependency ebenfalls ergänzen. Bevorzuge
aber generische Nutzung über `&dyn StorageEngine` bzw. `S: StorageEngine`,
um die Layer-Trennung (Graph kennt Storage nur über Trait) einzuhalten.

### 2. `CsrGraph` um Persistenz-Fähigkeit erweitern

**Datei**: `crates/memfuse-graph/src/csr.rs`

Füge folgende Konstanten hinzu:
```rust
/// LSM-Key-Prefix für alle Graph-Entities.
const GRAPH_ENTITY_PREFIX: &[u8] = b"__graph:entity:";
/// LSM-Key-Prefix für alle Graph-Edges.
const GRAPH_EDGE_PREFIX: &[u8] = b"__graph:edge:";
```

Implementiere zwei neue öffentliche Methoden auf `CsrGraph`:

```rust
impl CsrGraph {
    /// Persistiert eine einzelne Entity in den übergebenen Storage.
    pub async fn persist_entity<S: StorageEngine>(
        &self,
        storage: &S,
        tx: TxId,
        entity: &Entity,
    ) -> Result<()> {
        let key = [GRAPH_ENTITY_PREFIX, entity.id.as_bytes()].concat();
        let value = bincode::serialize(entity)
            .map_err(|e| MemFuseError::Internal(format!("graph entity serialize: {e}")))?;
        storage.put(tx, &key, &value).await
    }

    /// Persistiert eine einzelne Edge in den übergebenen Storage.
    pub async fn persist_edge<S: StorageEngine>(
        &self,
        storage: &S,
        tx: TxId,
        from: &EntityId,
        to: &EntityId,
        weight: f32,
    ) -> Result<()> {
        let key = [
            GRAPH_EDGE_PREFIX,
            from.as_bytes(),
            b":",
            to.as_bytes(),
        ].concat();
        let value = bincode::serialize(&weight)
            .map_err(|e| MemFuseError::Internal(format!("graph edge serialize: {e}")))?;
        storage.put(tx, &key, &value).await
    }

    /// Lädt den kompletten Graph-Zustand aus dem Storage (beim Startup).
    pub async fn load_from_storage<S: StorageEngine>(storage: &S) -> Result<Self> {
        let graph = Self::new(); // bestehender Konstruktor

        let entity_entries = storage.scan_prefix(GRAPH_ENTITY_PREFIX).await?;
        for (_, raw_value) in entity_entries {
            let entity: Entity = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph entity deserialize: {e}")))?;
            graph.insert_entity_direct(entity)?; // interne Methode, kein TxId nötig beim Bulk-Load
        }

        let edge_entries = storage.scan_prefix(GRAPH_EDGE_PREFIX).await?;
        for (raw_key, raw_value) in edge_entries {
            let weight: f32 = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph edge deserialize: {e}")))?;
            let key_str = String::from_utf8_lossy(&raw_key);
            let rest = key_str.strip_prefix("__graph:edge:").unwrap_or("");
            if let Some((from_str, to_str)) = rest.split_once(':') {
                let from_id = EntityId::from(from_str);
                let to_id = EntityId::from(to_str);
                graph.insert_edge_direct(from_id, to_id, weight)?;
            }
        }

        tracing::info!(
            entities = graph.entity_count(),
            "Graph aus Storage geladen"
        );
        Ok(graph)
    }
}
```

**Wichtig**: Falls `insert_entity_direct()` / `insert_edge_direct()` als
interne Bulk-Load-Methoden (ohne Transaktions-Staging) noch nicht existieren,
implementiere sie als Varianten der bestehenden `add_entity()`/`add_edge()`-
Trait-Methoden, die direkt in die CSR-Struktur schreiben statt über
`staged_entities`/`staged_edges` zu gehen — analog zur bestehenden internen
Struktur in `GraphInner`.

### 3. `commit()` im `GraphIndex`-Trait-Impl um Persistenz erweitern

Finde die bestehende `commit()`-Methode im `impl GraphIndex for CsrGraph`-Block
(die Datei referenziert bereits `rollback()` bei Zeile ~323 — `commit()` sollte
in der Nähe sein). Erweitere sie so, dass beim Commit einer Transaktion die
gestagten Entities und Edges NICHT nur in den In-Memory-CSR übernommen,
sondern zusätzlich über `persist_entity()`/`persist_edge()` persistiert werden.

Falls `CsrGraph` aktuell keinen Storage-Handle hält, füge dem Struct ein
optionales Feld hinzu:
```rust
pub struct CsrGraph {
    inner: RwLock<GraphInner>,
    /// Optionaler Persistenz-Handle. None = reiner In-Memory-Modus (z.B. Tests).
    storage: Option<Arc<dyn StorageEngine>>,
}
```

Und einen neuen Konstruktor:
```rust
pub fn with_storage(storage: Arc<dyn StorageEngine>) -> Self {
    Self {
        inner: RwLock::new(GraphInner::new()),
        storage: Some(storage),
    }
}
```

Passe `commit()` an: Wenn `self.storage.is_some()`, rufe für jede gestagte
Entity/Edge `persist_entity()`/`persist_edge()` mit dem gehaltenen Storage auf.

### 4. `CsrGraph` in `memfuse-db` verdrahten

**Datei**: `crates/memfuse-db/src/lib.rs` (oder `collection.rs`, je nachdem wo
die Collection-Initialisierung passiert)

1. `crates/memfuse-db/Cargo.toml`: `memfuse-graph = { workspace = true }` ergänzen
2. Beim Öffnen einer `Collection`/`MemFuse`-Instanz:
   - `CsrGraph::with_storage(storage.clone())` erzeugen statt `CsrGraph::new()`
   - Direkt danach `CsrGraph::load_from_storage(&storage).await?` aufrufen,
     um den persistierten Zustand zu laden
   - Das Ergebnis als `graph_index`-Feld im `Collection`-Struct ablegen
     (Feld ggf. neu hinzufügen, falls es aktuell keinen Graph-Bezug in
     `Collection` gibt)

### 5. Integrationstest für Persistenz über Neustart

Neue Datei `crates/memfuse-graph/tests/persistence_test.rs`:

```rust
use memfuse_graph::CsrGraph;
use memfuse_store::LsmStorage;
use memfuse_core::{Entity, EntityId, GraphIndex};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_graph_survives_restart() {
    let dir = tempdir().unwrap();
    let storage_path = dir.path().to_path_buf();

    // Phase 1: Graph öffnen, Daten einfügen, committen
    {
        let storage = Arc::new(LsmStorage::open(&storage_path).await.unwrap());
        let graph = CsrGraph::with_storage(storage.clone());
        let tx = storage.begin_tx().await.unwrap();
        graph.add_entity(tx, Entity {
            id: EntityId::from("kunde_mueller"),
            entity_type: "Kunde".into(),
            attributes: Default::default(),
        }).await.unwrap();
        graph.commit(tx).await.unwrap();
    }

    // Phase 2: Neuer Storage/Graph-Handle auf gleichem Pfad → muss Daten wiederfinden
    {
        let storage = Arc::new(LsmStorage::open(&storage_path).await.unwrap());
        let loaded_graph = CsrGraph::load_from_storage(storage.as_ref()).await.unwrap();
        assert_eq!(loaded_graph.entity_count(), 1);
    }
}
```

Passe die Test-API an die tatsächlich existierenden Signaturen von
`LsmStorage::open()`, `begin_tx()` etc. an — orientiere dich an bestehenden
Tests in `crates/memfuse-store/tests/` für die korrekten Aufrufmuster.

### 6. Verifikation

```bash
cargo test -p memfuse-graph 2>&1 | tail -20
cargo test -p memfuse-db 2>&1 | tail -20
grep -n "does not persist state" crates/memfuse-graph/src/csr.rs
```

Die letzte Zeile sollte NICHT mehr den alten Kommentar mit dieser Aussage
finden (er muss durch die neue Implementierung ersetzt bzw. der Kommentar
aktualisiert worden sein).
```

---

## Prompt 4 — Graph in `hybrid_search()` integrieren (echtes 3-Signal-RRF)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Den Graph tatsächlich in die Suche einbeziehen

Die Code-Analyse deckt auf, dass selbst wenn der Graph persistiert wäre,
er aktuell NIRGENDS in der Suche verwendet wird:

**Datei**: `crates/memfuse-db/src/collection.rs`, Zeile ~932-962

```rust
pub async fn hybrid_search(...) {
    ...
    Ok(crate::fusion::reciprocal_rank_fusion(
        vec![vector_results, text_results],  // Graph fehlt komplett!
        k,
    ))
}
```

Das ist der eigentliche Kern des "4-Signal"-Versprechens — ohne diesen Fix
bleibt es bei 2 Signalen, egal wie gut die Graph-Persistenz aus Prompt 3 ist.

### 1. Graph-Traversal-Ergebnisse als drittes Signal einbinden

**Datei**: `crates/memfuse-db/src/collection.rs`

Finde die `hybrid_search()`-Methode. Erweitere die Signatur (falls sinnvoll,
mit sensiblen Defaults für Abwärtskompatibilität) um einen optionalen
Anker-Entity-Parameter, über den der Graph-Traversal gestartet wird:

```rust
pub async fn hybrid_search(
    &self,
    query: &str,
    query_vector: &[f32],
    k: usize,
    anchor_entities: Option<&[EntityId]>,  // NEU: optionale Graph-Anker
) -> Result<Vec<SearchResult>> {
    let vector_results = self.vector_search(query_vector, k).await?;
    let text_results = self.text_search(query, k).await?;

    // NEU: Graph-Signal
    let graph_results = if let Some(anchors) = anchor_entities {
        self.graph_index
            .multi_traverse(anchors, MAX_TRAVERSAL_HOPS)
            .await?
    } else {
        // Fallback: Entities aus Text-Treffern als implizite Anker ableiten,
        // falls die Collection Entity-Extraktion unterstützt
        Vec::new()
    };

    let all_signal_sets = if graph_results.is_empty() {
        vec![vector_results, text_results]
    } else {
        vec![vector_results, text_results, graph_results]
    };

    Ok(crate::fusion::reciprocal_rank_fusion(all_signal_sets, k))
}
```

**Wichtig**: Prüfe die exakte Signatur der bestehenden `traverse()`-Methode
im `GraphIndex`-Trait (`crates/memfuse-core/src/traits.rs`) und passe
`multi_traverse()` entsprechend an, oder nutze eine bestehende Methode,
falls `traverse()` bereits mehrere Anker unterstützt.

### 2. Graph-Traversal-Ergebnisse in `SearchResult`-Format konvertieren

Die Fusion-Funktion `reciprocal_rank_fusion()` erwartet vermutlich einen
einheitlichen `SearchResult`-Typ mit `id` und `score`-Feldern für jedes
Signal-Set. Stelle sicher, dass die vom Graph zurückgegebenen Entities
(mit ihrem Score-Decay, siehe `SCORE_DECAY = 0.7` in `csr.rs`) korrekt
in diesen Typ konvertiert werden — die Entity-ID muss dabei der Dokument-ID
entsprechen, unter der das zugehörige Dokument in der Collection gespeichert
ist (Konvention prüfen: sind Entity-IDs identisch mit Dokument-IDs, oder
gibt es eine Mapping-Tabelle?).

Falls es aktuell keine direkte 1:1-Beziehung zwischen Entity-IDs und
Dokument-IDs gibt, dokumentiere dies explizit in einem Kommentar und
implementiere eine sinnvolle Fallback-Strategie (z.B. Entity-Metadaten
enthalten eine `source_doc_id`).

### 3. Test: 3-Signal-Fusion funktioniert end-to-end

Neue Datei oder Ergänzung in `crates/memfuse-db/tests/`:

```rust
#[tokio::test]
async fn test_hybrid_search_includes_graph_signal() {
    // 1. Collection öffnen
    // 2. Ein Dokument einfügen (Vektor + Text)
    // 3. Eine zugehörige Entity + Edge im Graph anlegen (gleiche ID)
    // 4. hybrid_search() MIT anchor_entities aufrufen
    // 5. Prüfen: Das über den Graph erreichbare Dokument taucht im Ergebnis auf,
    //    auch wenn Vektor- und Text-Score niedrig wären
}
```

### 4. README-Ehrlichkeit sicherstellen

**Datei**: `README.md`

Falls im README "4-Signal" oder "3-Signal RRF" beworben wird, stelle sicher,
dass die Formulierung erst NACH diesem Fix als "3-Signal: Vektor + BM25 +
Wissensgraph" (Metadaten-Filter ist ein 4. optionales Signal, kein RRF-Input)
korrekt beschrieben wird — und nicht mehr Features verspricht, als der Code
tatsächlich liefert.

### 5. Verifikation

```bash
cargo test -p memfuse-db 2>&1 | tail -30
grep -n "vec!\[vector_results, text_results\]" crates/memfuse-db/src/collection.rs
```

Die letzte Zeile sollte KEINEN Treffer mehr finden (der alte 2-Signal-Aufruf
muss durch die neue 3-Signal-fähige Logik ersetzt sein).
```

---

## Prompt 5 — FIND-STO-001 (Compaction-Tombstones) + FIND-DB-002 (drop_collection)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Zwei bestätigte Datenkorrektheits-Bugs beheben

### Bug 1: FIND-STO-001 — Compaction-Tombstone Phantom-Daten

**Datei**: `crates/memfuse-store/src/compaction.rs`

Problem: Tombstones werden bei PARTIELLER Compaction gelöscht, obwohl
ältere SSTables (die nicht Teil dieser Compaction sind) noch den originalen,
"gelöschten" Wert enthalten können. Nach der nächsten Lesezugriff auf eine
solche ältere SSTable "materialisiert" sich das eigentlich gelöschte
Dokument wieder.

Finde die Compaction-Schleife, die SSTables zusammenführt. Implementiere:

```rust
// Beim Schreiben der Output-SSTable während der Compaction:
if entry.is_tombstone() {
    if is_full_compaction {
        // Full-Compaction schließt ALLE SSTables der Collection ein →
        // Tombstone kann jetzt sicher verworfen werden, kein älterer
        // Wert kann mehr "durchsickern".
        continue;
    } else {
        // Partial-Compaction: Es könnten noch ältere SSTables außerhalb
        // dieser Compaction-Runde existieren, die den Original-Wert
        // enthalten. Tombstone MUSS erhalten bleiben, sonst "erscheint"
        // der gelöschte Wert beim nächsten Read wieder.
        output_builder.add(entry)?;
    }
} else {
    output_builder.add(entry)?;
}
```

Füge der Compaction-Funktion einen `is_full_compaction: bool`-Parameter
hinzu, falls dieser noch nicht existiert. `is_full_compaction` ist `true`,
wenn die Compaction-Runde ALLE existierenden SSTable-Tiers der betroffenen
Collection einschließt (also keine älteren, nicht beteiligten SSTables
mehr existieren können).

### Bug 2: FIND-DB-002 — `drop_collection()` hinterlässt Datenleichen

**Datei**: `crates/memfuse-db/src/lib.rs`

Aktueller fehlerhafter Code:
```rust
// drop_collection() löscht NUR den Index-Key:
self.storage.delete(tx, &col_idx_key).await?;
// Alle __col:<name>:* Keys bleiben für immer in der DB!
```

Fix unter Nutzung der in Prompt 2 hinzugefügten `delete_prefix()`-Methode:

```rust
pub async fn drop_collection(&self, name: &str) -> Result<()> {
    let tx = self.storage.begin_tx().await?;

    // 1. Alle Daten-Keys der Collection löschen (Prefix-basiert)
    let col_data_prefix = format!("__col:{}:", name);
    self.storage.delete_prefix(tx, col_data_prefix.as_bytes()).await?;

    // 2. Alle Graph-Daten dieser Collection löschen (falls Graph pro
    //    Collection isoliert ist — prüfe das tatsächliche Namespace-Schema)
    // Falls Graph collection-übergreifend ist, diesen Schritt auslassen
    // und stattdessen entity-spezifisches Cleanup nachdenken.

    // 3. Den Index-Key selbst löschen (bisheriges Verhalten)
    let col_idx_key = /* bestehende Konstruktion des Index-Keys */;
    self.storage.delete(tx, &col_idx_key).await?;

    // 4. In-Memory-Referenz entfernen (HNSW-Index, Collection-Handle etc.)
    self.collections.write().remove(name);  // parking_lot RwLock aus Prompt 1

    self.storage.commit(tx).await?;
    Ok(())
}
```

Passe den Code an die tatsächliche Struktur von `MemFuse`/`lib.rs` an —
insbesondere wie `col_idx_key` bisher konstruiert wird und wie die
In-Memory-Collection-Registry (vermutlich ein `HashMap<String, ...>`
hinter einem Lock) heißt.

### 3. Tests

Ergänze in `crates/memfuse-db/tests/` (oder passendem bestehenden File):

```rust
#[tokio::test]
async fn test_drop_collection_removes_all_data() {
    // 1. Collection erstellen, mehrere Dokumente einfügen
    // 2. drop_collection() aufrufen
    // 3. storage.scan_prefix("__col:<name>:") muss LEER sein
    // 4. list_collections() darf den Namen nicht mehr enthalten
}

#[tokio::test]
async fn test_partial_compaction_preserves_tombstones() {
    // 1. Dokument einfügen, mehrere SSTable-Flushes erzwingen (mehrere
    //    Schreibvorgänge über die Flush-Schwelle)
    // 2. Dokument löschen (erzeugt Tombstone in neuer SSTable)
    // 3. NUR die neueste SSTable-Gruppe partiell kompaktieren
    //    (is_full_compaction = false)
    // 4. Dokument abfragen → muss weiterhin als gelöscht erscheinen
    //    (Tombstone darf nicht verloren gegangen sein)
}
```

### 4. Verifikation

```bash
cargo test -p memfuse-store -p memfuse-db 2>&1 | tail -30
```
```

---

## Prompt 6 — `memfuse-tauri` Crate: Grundgerüst

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Neues Crate `memfuse-tauri` als Grundgerüst für die Desktop-App

Die strategische Neuausrichtung sieht MemFuse als **Tauri-Desktop-Applikation**
vor ("MemFuse Brain") statt als reine Bibliothek. Dieser Prompt legt das
Grundgerüst an — noch OHNE vollständige UI, aber mit lauffähiger Tauri-Shell
und Backend-Anbindung.

### 1. Neues Crate anlegen: `crates/memfuse-tauri/`

```toml
# crates/memfuse-tauri/Cargo.toml
[package]
name = "memfuse-tauri"
version.workspace = true
edition.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
memfuse-db = { workspace = true }
memfuse-graph = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[lib]
name = "memfuse_tauri_lib"
crate-type = ["staticlib", "cdylib", "rlib"]
```

### 2. In Root-`Cargo.toml` als optionales Workspace-Mitglied ergänzen

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
    "crates/memfuse-tauri",
]
```

(`memfuse-py` bleibt vorerst außen vor — die neue Priorität ist die
Desktop-App, nicht die Python-Bibliothek. Falls `memfuse-py` bereits
Teil des Workspace ist, unverändert lassen.)

### 3. Tauri-App-Struktur anlegen

```
crates/memfuse-tauri/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── state.rs       # App-State: gehaltene MemFuse-Instanz
│   └── commands/
│       └── mod.rs     # Platzhalter für Tauri-Commands (Prompt 10)
└── icons/              # Platzhalter-Icons (können generisch sein)
```

**`src/lib.rs`**:
```rust
mod state;
pub mod commands;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Commands werden in Prompt 10 ergänzt
        ])
        .run(tauri::generate_context!())
        .expect("error while running memfuse-brain application");
}
```

**`src/main.rs`**:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    memfuse_tauri_lib::run();
}
```

**`src/state.rs`**:
```rust
use memfuse_db::MemFuse;
use std::sync::Arc;
use parking_lot::RwLock;

/// Globaler App-Zustand: hält die aktuell geöffnete lokale Datenbank.
pub struct AppState {
    pub db: RwLock<Option<Arc<MemFuse>>>,
    pub db_path: RwLock<Option<std::path::PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: RwLock::new(None),
            db_path: RwLock::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

**`tauri.conf.json`** (Minimal-Konfiguration):
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "MemFuse Brain",
  "version": "0.1.0",
  "identifier": "com.memfuse.brain",
  "app": {
    "windows": [
      {
        "title": "MemFuse Brain — Ihr lokaler Unternehmens-Assistent",
        "width": 1200,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

Erstelle Platzhalter-Icon-Dateien (können minimale/generische PNGs sein,
sofern für den Build notwendig — falls `tauri-build` ohne echte Icons
bei `cargo build` (nicht `tauri build`) nicht bricht, reicht ein Kommentar,
dass die Icons vor dem finalen App-Bundle ersetzt werden müssen).

### 4. `build.rs`

```rust
fn main() {
    tauri_build::build()
}
```

### 5. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
```

Ein vollständiger `cargo build -p memfuse-tauri` benötigt ggf. System-
Dependencies (WebView2 auf Windows, WebKitGTK auf Linux), die in der
Jules-Umgebung eventuell nicht vorhanden sind — `cargo check` sollte
aber unabhängig davon durchlaufen. Falls `cargo check` an fehlenden
System-Libraries scheitert, dokumentiere dies explizit im PR-Kommentar,
da dies eine reine Umgebungsfrage ist und keinen Code-Fehler darstellt.
```

---

## Prompt 7 — Ingestion-Pipeline (PDF/DOCX/Markdown/E-Mail)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: `IngestionPipeline` implementieren — nutzt den bestehenden MarkdownChunker

Die Code-Analyse bestätigt: `crates/memfuse-db/src/chunker.rs` und `context.rs`
bilden bereits einen "Kern einer produktionsreifen RAG-Pipeline — nur noch
nicht an eine UI verdrahtet." Diese Aufgabe baut die fehlende
Ingestion-Schicht darüber.

### 1. Neues Modul: `crates/memfuse-tauri/src/ingestion/mod.rs`

Lege die Ingestion-Pipeline im Tauri-Crate an (nicht in `memfuse-db`, da sie
UI-nahe Dependencies wie PDF/DOCX-Parser einführt, die den Core-Datenbank-
Kern nicht aufblähen sollen).

```
crates/memfuse-tauri/src/ingestion/
├── mod.rs
├── pdf.rs
├── docx.rs
├── email.rs
└── pipeline.rs
```

### 2. `Cargo.toml` — neue Dependencies

```toml
[dependencies]
# ... bestehende ...
pdf-extract = "0.7"
docx-rs = "0.4"
mailparse = "0.15"
```

### 3. `src/ingestion/pdf.rs`

```rust
use memfuse_core::Result;
use std::path::Path;

/// Extrahiert reinen Text aus einer PDF-Datei.
pub fn extract_pdf_text(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path)
        .map_err(|e| memfuse_core::MemFuseError::Internal(
            format!("PDF-Extraktion fehlgeschlagen für {:?}: {e}", path)
        ))
}
```

### 4. `src/ingestion/docx.rs`

```rust
use memfuse_core::Result;
use std::path::Path;

/// Extrahiert reinen Text aus einer DOCX-Datei.
pub fn extract_docx_text(path: &Path) -> Result<String> {
    use docx_rs::*;
    let bytes = std::fs::read(path)
        .map_err(|e| memfuse_core::MemFuseError::Internal(
            format!("DOCX lesen fehlgeschlagen für {:?}: {e}", path)
        ))?;
    // docx-rs Parsing-Logik: Paragraphen extrahieren und zu Fließtext
    // zusammenfügen. Passe an die tatsächliche docx-rs API-Version an,
    // die beim `cargo add` aufgelöst wird — die Read-API kann je nach
    // Version leicht variieren.
    todo!("docx-rs Text-Extraktion — Implementierung gemäß installierter API-Version")
}
```

(Hinweis für Jules: Prüfe beim Hinzufügen der Dependency die exakte API von
`docx-rs`, da sich Methodennamen zwischen Versionen unterscheiden können,
und implementiere die Extraktion entsprechend vollständig, nicht als `todo!`.)

### 5. `src/ingestion/email.rs`

```rust
use memfuse_core::Result;
use std::path::Path;

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei.
pub struct EmailContent {
    pub subject: String,
    pub from: String,
    pub body: String,
}

pub fn extract_email(path: &Path) -> Result<EmailContent> {
    let raw = std::fs::read(path)
        .map_err(|e| memfuse_core::MemFuseError::Internal(
            format!("E-Mail lesen fehlgeschlagen für {:?}: {e}", path)
        ))?;
    let parsed = mailparse::parse_mail(&raw)
        .map_err(|e| memfuse_core::MemFuseError::Internal(
            format!("E-Mail parsen fehlgeschlagen: {e}")
        ))?;

    let subject = parsed.headers.get_first_value("Subject").unwrap_or_default();
    let from = parsed.headers.get_first_value("From").unwrap_or_default();
    let body = parsed.get_body().unwrap_or_default();

    Ok(EmailContent { subject, from, body })
}
```

### 6. `src/ingestion/pipeline.rs` — Orchestrierung

```rust
use memfuse_core::Result;
use memfuse_db::{Collection, MemFuse};
use std::path::Path;
use std::sync::Arc;

/// Ergebnis eines Ingestion-Vorgangs.
#[derive(Debug, serde::Serialize)]
pub struct IngestReport {
    pub file_path: String,
    pub chunks_created: usize,
    pub errors: Vec<String>,
}

/// Trait für eine Embedding-Funktion — abstrahiert vom konkreten
/// Backend (Ollama, OpenAI, lokales ONNX-Modell etc.).
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

pub struct IngestionPipeline {
    embedder: Arc<dyn EmbeddingProvider>,
}

impl IngestionPipeline {
    pub fn new(embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { embedder }
    }

    /// Liest eine Datei, erkennt das Format anhand der Endung, chunked den
    /// Text mit dem bestehenden MarkdownChunker und speichert die Chunks
    /// mit Embeddings in der übergebenen Collection.
    pub async fn ingest_file(
        &self,
        path: &Path,
        collection: &Collection,
    ) -> Result<IngestReport> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let raw_text = match extension.as_str() {
            "pdf" => crate::ingestion::pdf::extract_pdf_text(path)?,
            "docx" => crate::ingestion::docx::extract_docx_text(path)?,
            "md" | "markdown" | "txt" => std::fs::read_to_string(path)
                .map_err(|e| memfuse_core::MemFuseError::Internal(
                    format!("Datei lesen fehlgeschlagen: {e}")
                ))?,
            "eml" => {
                let email = crate::ingestion::email::extract_email(path)?;
                format!("Betreff: {}\nVon: {}\n\n{}", email.subject, email.from, email.body)
            }
            other => {
                return Ok(IngestReport {
                    file_path: path.display().to_string(),
                    chunks_created: 0,
                    errors: vec![format!("Nicht unterstütztes Format: .{other}")],
                });
            }
        };

        // Nutzt den bereits bestehenden MarkdownChunker aus memfuse-db
        let chunker = memfuse_db::chunker::MarkdownChunker::default();
        let chunks = chunker.chunk(&raw_text);

        let mut created = 0;
        let mut errors = Vec::new();

        for chunk in chunks {
            match self.embedder.embed(&chunk.text).await {
                Ok(embedding) => {
                    let doc_id = format!(
                        "{}#{}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        created
                    );
                    let metadata = serde_json::json!({
                        "text": chunk.text,
                        "source": path.display().to_string(),
                        "heading_path": chunk.heading_path,  // aus bestehendem Chunker
                    });
                    if let Err(e) = collection.insert(&doc_id, &embedding, Some(metadata)).await {
                        errors.push(format!("Insert fehlgeschlagen: {e}"));
                    } else {
                        created += 1;
                    }
                }
                Err(e) => errors.push(format!("Embedding fehlgeschlagen: {e}")),
            }
        }

        Ok(IngestReport {
            file_path: path.display().to_string(),
            chunks_created: created,
            errors,
        })
    }

    /// Indiziert alle unterstützten Dateien in einem Ordner (rekursiv).
    pub async fn ingest_folder(
        &self,
        folder: &Path,
        collection: &Collection,
    ) -> Result<Vec<IngestReport>> {
        let mut reports = Vec::new();
        let supported = ["pdf", "docx", "md", "markdown", "txt", "eml"];

        for entry in walkdir::WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if supported.contains(&ext.as_str()) {
                let report = self.ingest_file(entry.path(), collection).await?;
                reports.push(report);
            }
        }

        Ok(reports)
    }
}
```

Ergänze `walkdir = "2"` in `Cargo.toml`.

**Wichtig**: Prüfe die tatsächliche API von `memfuse_db::chunker::MarkdownChunker`
(Methodennamen, Feldnamen von `chunk.text`/`chunk.heading_path`) und passe
den obigen Code an die reale Signatur an — der Code hier ist ein Zielentwurf,
kein exaktes API-Zitat.

### 7. Tests

```rust
// crates/memfuse-tauri/tests/ingestion_test.rs
#[tokio::test]
async fn test_ingest_markdown_file() {
    // Dummy-EmbeddingProvider, der einen konstanten Vektor zurückgibt
    // Markdown-Testdatei erstellen, ingest_file() aufrufen
    // Prüfen: chunks_created > 0, errors leer
}
```

### 8. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
cargo test -p memfuse-tauri 2>&1 | tail -30
```
```

---

## Prompt 8 — Deutsche Morphologie ausbauen (KMU-Vokabular)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Deutsche Morphologie in `memfuse-text` für KMU-Fachvokabular erweitern

Die Analyse bestätigt: `memfuse-text` hat bereits einen Tokenizer mit
"Morphologie-Erweiterung (DACH-Sprachunterstützung!)" — das ist die
sauberste Implementierung im Repository. Diese Aufgabe baut das für den
KMU-Kontext gezielt aus, OHNE die bestehende saubere Architektur zu stören.

### 1. Bestehende Morphologie-Implementierung analysieren

Lies zuerst `crates/memfuse-text/src/morphology.rs` und den zugehörigen
Tokenizer vollständig, um die bestehende Struktur (Trait-Namen, Methoden-
Signaturen) zu verstehen, bevor Änderungen vorgenommen werden. Halte dich
exakt an die bestehenden Konventionen.

### 2. KMU-Fachvokabular-Wörterbuch ergänzen

Erweitere das bestehende Compound-Splitting-Wörterbuch um branchenspezifische
Begriffe aus folgenden Domänen (als zusätzliches, klar kommentiertes
Datensegment, nicht als Ersatz der bestehenden Einträge):

```rust
/// KMU-Fachvokabular — ergänzt das Basis-Wörterbuch für Unternehmenskontexte.
const KMU_DOMAIN_VOCABULARY: &[&str] = &[
    // Geschäftsprozesse
    "auftrags", "angebots", "rechnungs", "lieferungs", "bestellungs",
    "kunden", "lieferanten", "vertrags", "zahlungs",
    // HR
    "mitarbeiter", "personal", "urlaubs", "gehalts", "arbeits",
    "bewerbungs", "schulungs",
    // Logistik
    "lager", "bestands", "transport", "versand", "liefer", "fracht",
    // Produktion
    "fertigungs", "produktions", "qualitäts", "wartungs", "maschinen",
    "prüfungs", "prozess",
    // Compliance & Recht
    "datenschutz", "compliance", "richtlinie", "genehmigungs",
    "zertifizierungs", "haftungs",
    // Finanzen
    "finanz", "steuer", "buchhaltungs", "bilanz", "liquiditäts",
];
```

Integriere dieses Vokabular in die bestehende Dictionary-Struktur, ohne
die Performance-Charakteristik des Splitters zu verschlechtern (Reihenfolge
nach Häufigkeit beibehalten, falls das bestehende Wörterbuch so sortiert ist).

### 3. Umlaut-Normalisierung ergänzen (falls noch nicht vorhanden)

Prüfe, ob bereits eine Umlaut-Normalisierung existiert. Falls nicht, füge
hinzu:

```rust
/// Normalisiert deutsche Umlaute für robusten Suchabgleich.
pub fn normalize_umlauts(input: &str) -> String {
    input
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
}
```

Integriere diese Funktion konsistent an der Stelle im Tokenizer-Pipeline,
an der auch das bestehende Compound-Splitting eingehängt ist — sowohl beim
Indexieren als auch bei der Query-Verarbeitung in `memfuse-text/src/bm25.rs`.

### 4. Neue Tests

```rust
#[test]
fn test_kmu_domain_compounds() {
    let splitter = GermanCompoundSplitter::new(); // exakter Typname prüfen
    let result = splitter.decompose("lagerbestandsverwaltung");
    assert!(result.len() > 1);

    let result = splitter.decompose("urlaubsantragsprozess");
    assert!(result.len() > 1);
}

#[test]
fn test_umlaut_normalization_kmu_terms() {
    assert_eq!(normalize_umlauts("Änderungsantrag"), "aenderungsantrag");
    assert_eq!(normalize_umlauts("Qualitätsprüfung"), "qualitaetspruefung");
}
```

### 5. Verifikation

```bash
cargo test -p memfuse-text 2>&1 | tail -30
```

Alle bestehenden Tests müssen weiterhin grün sein — keine Regression im
bereits sauberen Modul.
```

---

## Prompt 9 — Ollama-Bridge (Chat mit RAG-Streaming)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Ollama-Anbindung für lokalen Chat mit RAG-Kontext

MemFuse Brain nutzt Ollama als lokales LLM-Backend (kein Cloud-API-Call,
konsistent mit dem Air-Gapped-Versprechen). Diese Aufgabe implementiert die
Bridge zwischen dem bereits vorhandenen `ContextManager` (aus `memfuse-db`)
und einer lokalen Ollama-Instanz.

### 1. Dependency ergänzen

**Datei**: `crates/memfuse-tauri/Cargo.toml`
```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
futures-util = "0.3"
```

### 2. Neues Modul: `crates/memfuse-tauri/src/ollama.rs`

```rust
use futures_util::StreamExt;
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};

/// Bridge zu einer lokal laufenden Ollama-Instanz.
pub struct OllamaBridge {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatStreamChunk {
    message: Option<ChatMessageResponse>,
    done: bool,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

impl OllamaBridge {
    /// Erstellt eine neue Bridge. Standard-Port von Ollama: 11434.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn localhost() -> Self {
        Self::new("http://localhost:11434")
    }

    /// Prüft, ob Ollama erreichbar ist und listet verfügbare Modelle.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(
                format!("Ollama nicht erreichbar unter {}: {e}. Ist Ollama gestartet?", self.base_url)
            ))?;

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Antwort ungültig: {e}")))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Führt einen RAG-Chat aus: Systemkontext (aus MemFuse-Suchergebnissen)
    /// wird vor die Nutzerfrage gesetzt, Antwort wird gestreamt.
    pub async fn chat_with_rag_streaming(
        &self,
        model: &str,
        user_query: &str,
        context: &str,  // Vom ContextManager::prepare_context() erzeugt
        mut on_token: impl FnMut(String) + Send,
    ) -> Result<String> {
        let system_prompt = format!(
            "Du bist ein hilfreicher Unternehmensassistent. Beantworte Fragen \
             ausschließlich auf Basis des folgenden Kontexts aus internen \
             Firmendokumenten. Antworte auf Deutsch. Wenn die Antwort im \
             Kontext nicht zu finden ist, sage ehrlich: \
             'Diese Information liegt mir nicht vor.'\n\nKontext:\n{context}"
        );

        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage { role: "system".into(), content: system_prompt },
                ChatMessage { role: "user".into(), content: user_query.to_string() },
            ],
            stream: true,
        };

        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Chat-Anfrage fehlgeschlagen: {e}")))?;

        let mut stream = response.bytes_stream();
        let mut full_response = String::new();

        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result
                .map_err(|e| MemFuseError::Internal(format!("Stream-Fehler: {e}")))?;
            for line in bytes.split(|&b| b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(chunk) = serde_json::from_slice::<ChatStreamChunk>(line) {
                    if let Some(msg) = chunk.message {
                        on_token(msg.content.clone());
                        full_response.push_str(&msg.content);
                    }
                    if chunk.done {
                        break;
                    }
                }
            }
        }

        Ok(full_response)
    }
}
```

### 3. `EmbeddingProvider`-Implementierung für Ollama

Ergänze in `crates/memfuse-tauri/src/ollama.rs` eine Implementierung des
in Prompt 7 definierten `EmbeddingProvider`-Traits, damit Ollama auch für
Embeddings genutzt werden kann (z.B. mit `nomic-embed-text`):

```rust
#[async_trait::async_trait]
impl crate::ingestion::pipeline::EmbeddingProvider for OllamaBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            prompt: &'a str,
        }
        #[derive(Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        let url = format!("{}/api/embeddings", self.base_url);
        let request = EmbedRequest { model: "nomic-embed-text", prompt: text };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Embedding fehlgeschlagen: {e}")))?;

        let parsed: EmbedResponse = response
            .json()
            .await
            .map_err(|e| MemFuseError::Internal(format!("Ollama-Embedding-Antwort ungültig: {e}")))?;

        Ok(parsed.embedding)
    }
}
```

### 4. Test mit Mock (kein echtes Ollama in CI nötig)

```rust
// crates/memfuse-tauri/tests/ollama_test.rs
#[tokio::test]
async fn test_ollama_bridge_handles_connection_error_gracefully() {
    // Bridge auf einen garantiert nicht existierenden Port zeigen lassen
    let bridge = memfuse_tauri_lib::ollama::OllamaBridge::new("http://localhost:1");
    let result = bridge.list_models().await;
    assert!(result.is_err());
    // Fehlermeldung muss hilfreich sein (erwähnt "Ollama" und "gestartet")
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Ollama"));
}
```

### 5. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
cargo test -p memfuse-tauri 2>&1 | tail -30
```
```

---

## Prompt 10 — Tauri-Commands (IPC-Schicht) + Frontend-Grundgerüst

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Tauri-Commands implementieren — die Brücke zwischen Frontend und Backend

Diese Aufgabe verdrahtet alle bisher gebauten Komponenten (Suche, Ingestion,
Ollama-Chat) als aufrufbare Tauri-Commands, die vom Frontend (JS/TS) über
`invoke()` angesprochen werden können.

### 1. `crates/memfuse-tauri/src/commands/mod.rs`

```rust
mod search;
mod ingest;
mod chat;
mod collections;

pub use search::*;
pub use ingest::*;
pub use chat::*;
pub use collections::*;
```

### 2. `src/commands/collections.rs`

```rust
use crate::state::AppState;
use memfuse_db::{MemFuse, MemFuseConfig};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct CollectionInfo {
    pub name: String,
    pub document_count: usize,
}

/// Öffnet oder erstellt eine lokale MemFuse-Datenbank am gegebenen Pfad.
#[tauri::command]
pub async fn open_database(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let db = MemFuse::open(&path_buf)
        .await
        .map_err(|e| format!("Datenbank konnte nicht geöffnet werden: {e}"))?;

    *state.db.write() = Some(std::sync::Arc::new(db));
    *state.db_path.write() = Some(path_buf);
    Ok(())
}

#[tauri::command]
pub async fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionInfo>, String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;

    // Passe an die tatsächliche list_collections()-API von MemFuse an
    let names = db.list_collections().await.map_err(|e| e.to_string())?;
    let mut infos = Vec::new();
    for name in names {
        let col = db.collection(&name).await.map_err(|e| e.to_string())?;
        let count = col.count().await.map_err(|e| e.to_string())?;
        infos.push(CollectionInfo { name, document_count: count });
    }
    Ok(infos)
}

#[tauri::command]
pub async fn create_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    db.collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn drop_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    db.drop_collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}
```

### 3. `src/commands/ingest.rs`

```rust
use crate::state::AppState;
use crate::ingestion::pipeline::{IngestionPipeline, IngestReport};
use crate::ollama::OllamaBridge;
use tauri::State;
use std::sync::Arc;

#[tauri::command]
pub async fn ingest_file(
    state: State<'_, AppState>,
    file_path: String,
    collection_name: String,
) -> Result<IngestReport, String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    let collection = db.collection(&collection_name).await.map_err(|e| e.to_string())?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    pipeline
        .ingest_file(std::path::Path::new(&file_path), &collection)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ingest_folder(
    state: State<'_, AppState>,
    folder_path: String,
    collection_name: String,
) -> Result<Vec<IngestReport>, String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    let collection = db.collection(&collection_name).await.map_err(|e| e.to_string())?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    pipeline
        .ingest_folder(std::path::Path::new(&folder_path), &collection)
        .await
        .map_err(|e| e.to_string())
}
```

### 4. `src/commands/search.rs`

```rust
use crate::state::AppState;
use crate::ollama::OllamaBridge;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SearchResultDto {
    pub id: String,
    pub score: f32,
    pub text_preview: String,
    pub source: String,
}

#[tauri::command]
pub async fn hybrid_search(
    state: State<'_, AppState>,
    query: String,
    collection_name: String,
    k: usize,
) -> Result<Vec<SearchResultDto>, String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    let collection = db.collection(&collection_name).await.map_err(|e| e.to_string())?;

    let embedder = OllamaBridge::localhost();
    let query_vector = embedder.embed(&query).await.map_err(|e| e.to_string())?;

    let results = collection
        .hybrid_search(&query, &query_vector, k, None)  // Signatur aus Prompt 4
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|r| SearchResultDto {
            id: r.id.clone(),
            score: r.score,
            text_preview: r.metadata
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.chars().take(200).collect())
                .unwrap_or_default(),
            source: r.metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("Unbekannt")
                .to_string(),
        })
        .collect())
}
```

### 5. `src/commands/chat.rs`

```rust
use crate::state::AppState;
use crate::ollama::OllamaBridge;
use tauri::{State, Emitter};

/// Streamt Chat-Antworten als Tauri-Events an das Frontend, statt sie
/// als einzelnen Rückgabewert zu liefern.
#[tauri::command]
pub async fn chat_with_rag(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    message: String,
    collection_name: String,
    model: String,
) -> Result<String, String> {
    let db_guard = state.db.read();
    let db = db_guard.as_ref().ok_or("Keine Datenbank geöffnet")?;
    let collection = db.collection(&collection_name).await.map_err(|e| e.to_string())?;

    let embedder = OllamaBridge::localhost();
    let query_vector = embedder.embed(&message).await.map_err(|e| e.to_string())?;

    let search_results = collection
        .hybrid_search(&message, &query_vector, 5, None)
        .await
        .map_err(|e| e.to_string())?;

    // Nutzt den bestehenden ContextManager aus memfuse-db
    let context_manager = memfuse_db::context::ContextManager::default();
    let context = context_manager
        .prepare_context(search_results)
        .map_err(|e| e.to_string())?;

    let bridge = OllamaBridge::localhost();
    let app_clone = app.clone();
    let full_response = bridge
        .chat_with_rag_streaming(&model, &message, &context.to_string(), move |token| {
            let _ = app_clone.emit("chat-token", token);
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(full_response)
}

#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let bridge = OllamaBridge::localhost();
    bridge.list_models().await.map_err(|e| e.to_string())
}
```

### 6. `src/lib.rs` — Commands registrieren

Aktualisiere die `invoke_handler!`-Liste:
```rust
.invoke_handler(tauri::generate_handler![
    commands::open_database,
    commands::list_collections,
    commands::create_collection,
    commands::drop_collection,
    commands::ingest_file,
    commands::ingest_folder,
    commands::hybrid_search,
    commands::chat_with_rag,
    commands::list_ollama_models,
])
```

### 7. Minimal-Frontend (Platzhalter, kein vollständiges UI-Polish)

Erstelle `crates/memfuse-tauri/ui/index.html` als einfaches Vanilla-JS-
Grundgerüst (kein Framework-Overhead in diesem Schritt):

```html
<!DOCTYPE html>
<html lang="de">
<head>
    <meta charset="UTF-8">
    <title>MemFuse Brain</title>
    <style>
        body { font-family: system-ui, sans-serif; margin: 2rem; }
        #chat-log { border: 1px solid #ccc; padding: 1rem; min-height: 300px; margin-bottom: 1rem; }
        input, button { padding: 0.5rem; font-size: 1rem; }
    </style>
</head>
<body>
    <h1>MemFuse Brain — Ihr lokaler Unternehmens-Assistent</h1>
    <div id="chat-log"></div>
    <input id="query-input" type="text" placeholder="Stellen Sie eine Frage..." style="width: 70%;">
    <button id="send-btn">Senden</button>

    <script type="module">
        const { invoke } = window.__TAURI__.core;
        const { listen } = window.__TAURI__.event;

        const log = document.getElementById('chat-log');
        const input = document.getElementById('query-input');
        const btn = document.getElementById('send-btn');

        let currentResponseEl = null;
        listen('chat-token', (event) => {
            if (currentResponseEl) {
                currentResponseEl.textContent += event.payload;
            }
        });

        btn.addEventListener('click', async () => {
            const message = input.value;
            if (!message) return;

            log.innerHTML += `<p><strong>Sie:</strong> ${message}</p>`;
            currentResponseEl = document.createElement('p');
            currentResponseEl.innerHTML = '<strong>Assistent:</strong> ';
            log.appendChild(currentResponseEl);
            input.value = '';

            try {
                await invoke('chat_with_rag', {
                    message,
                    collectionName: 'dokumente',
                    model: 'llama3.2',
                });
            } catch (e) {
                currentResponseEl.textContent += `Fehler: ${e}`;
            }
        });
    </script>
</body>
</html>
```

Ergänze in `tauri.conf.json` den `frontendDist`-Pfad:
```json
"build": {
    "frontendDist": "ui"
}
```

### 8. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
```
```

---

## Prompt 11 — Echter MCP-Server (axum/SSE statt JSON-Stub)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: MCP-Server implementieren — `mcp.json` ist bisher nur ein Stub

Die Analyse bestätigt: "`mcp.json` ist ein JSON-Stub mit Tool-Deklarationen.
Kein einziger Code-Byte implementiert den Server." Diese Aufgabe schließt
diese Lücke mit einem echten HTTP/SSE-Server, damit MemFuse als MCP-Server
für Claude Desktop und andere MCP-Clients nutzbar wird.

### 1. Neues Crate: `crates/memfuse-mcp/`

```toml
# crates/memfuse-mcp/Cargo.toml
[package]
name = "memfuse-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
memfuse-db = { workspace = true }
axum = { version = "0.7", features = ["json"] }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
async-stream = "0.3"
```

In Root-`Cargo.toml` als Workspace-Mitglied ergänzen:
```toml
members = [
    # ... bestehende ...
    "crates/memfuse-mcp",
]
```

### 2. Bestehende `mcp.json` als Spezifikation lesen

Lies die bestehende `mcp.json` im Repo-Root (oder wo sie liegt) vollständig,
um die exakten Tool-Namen, Parameter und Beschreibungen zu übernehmen. Der
Server muss GENAU diese deklarierten Tools bereitstellen, nicht neu erfundene.

### 3. `crates/memfuse-mcp/src/lib.rs`

```rust
use axum::{
    routing::post,
    Router, Json,
    extract::State,
};
use memfuse_db::MemFuse;
use serde_json::Value;
use std::sync::Arc;

pub struct McpServerState {
    pub db: Arc<MemFuse>,
}

/// Erstellt den axum-Router mit allen MCP-JSON-RPC-Endpunkten.
pub fn create_router(state: Arc<McpServerState>) -> Router {
    Router::new()
        .route("/mcp/tools/list", axum::routing::get(list_tools))
        .route("/mcp/tools/call", post(call_tool))
        .with_state(state)
}

async fn list_tools() -> Json<Value> {
    // Tool-Liste EXAKT aus der bestehenden mcp.json übernehmen
    Json(serde_json::json!({
        "tools": [
            {
                "name": "memfuse_search",
                "description": "Hybrid-Suche über Vektor, BM25 und Wissensgraph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "collection": {"type": "string"},
                        "k": {"type": "integer", "default": 5}
                    },
                    "required": ["query", "collection"]
                }
            },
            {
                "name": "memfuse_insert",
                "description": "Fügt ein Dokument in eine Collection ein",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "text": {"type": "string"},
                        "collection": {"type": "string"}
                    },
                    "required": ["id", "text", "collection"]
                }
            },
            {
                "name": "memfuse_get",
                "description": "Holt ein Dokument per ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "collection": {"type": "string"}
                    },
                    "required": ["id", "collection"]
                }
            }
            // Weitere Tools gemäß bestehender mcp.json ergänzen
        ]
    }))
}

async fn call_tool(
    State(state): State<Arc<McpServerState>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let tool_name = request.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = request.get("arguments").cloned().unwrap_or_default();

    let result = match tool_name {
        "memfuse_search" => handle_search(&state, &args).await,
        "memfuse_insert" => handle_insert(&state, &args).await,
        "memfuse_get" => handle_get(&state, &args).await,
        other => Err(format!("Unbekanntes Tool: {other}")),
    };

    match result {
        Ok(value) => Json(serde_json::json!({ "content": [{ "type": "text", "text": value.to_string() }] })),
        Err(e) => Json(serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": e }] })),
    }
}

async fn handle_search(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let query = args.get("query").and_then(|v| v.as_str()).ok_or("query fehlt")?;
    let collection_name = args.get("collection").and_then(|v| v.as_str()).ok_or("collection fehlt")?;
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let collection = state.db.collection(collection_name).await.map_err(|e| e.to_string())?;

    // Für reine Text-Suche ohne externes Embedding-Modell: text_search() nutzen
    // Für volle Hybrid-Suche wird ein Embedding benötigt — hier vereinfacht
    // auf Text-Suche, da der MCP-Server keinen eingebetteten Embedder hat.
    let results = collection.text_search(query, k).await.map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(results).unwrap_or_default())
}

async fn handle_insert(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
    let text = args.get("text").and_then(|v| v.as_str()).ok_or("text fehlt")?;
    let collection_name = args.get("collection").and_then(|v| v.as_str()).ok_or("collection fehlt")?;

    let collection = state.db.collection(collection_name).await.map_err(|e| e.to_string())?;
    // Ohne Vektor-Embedding: Dummy-Vektor oder text-only insert, je nach
    // tatsächlicher API. Prüfe ob collection.insert() einen optionalen
    // Embedding-Parameter erlaubt oder ob memfuse-embed hierfür gebraucht wird.
    let metadata = serde_json::json!({ "text": text });
    collection.insert(id, &[], Some(metadata)).await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "status": "inserted", "id": id }))
}

async fn handle_get(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
    let collection_name = args.get("collection").and_then(|v| v.as_str()).ok_or("collection fehlt")?;

    let collection = state.db.collection(collection_name).await.map_err(|e| e.to_string())?;
    let doc = collection.get(id).await.map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(doc).unwrap_or(Value::Null))
}
```

**Wichtig**: Der `handle_insert`-Fall mit leerem Vektor `&[]` ist ein
Platzhalter — prüfe, ob `collection.insert()` tatsächlich einen leeren
Vektor akzeptiert oder ob hierfür zwingend ein echtes Embedding nötig ist.
Falls letzteres, dokumentiere dies als bekannte Einschränkung: Der reine
MCP-Server kann ohne angeschlossenes Embedding-Modell nur Text-Suche und
keine Vektor-Suche/Insert anbieten — das ist ein akzeptabler MVP-Scope.

### 4. `src/bin/memfuse-mcp-server.rs` — Standalone-Binary

```rust
use memfuse_db::MemFuse;
use memfuse_mcp::{create_router, McpServerState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .iter()
        .position(|a| a == "--db-path")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "./memfuse_data".to_string());

    let db = MemFuse::open(&db_path).await?;
    let state = Arc::new(McpServerState { db: Arc::new(db) });
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3939").await?;
    tracing::info!("MemFuse MCP-Server läuft auf http://127.0.0.1:3939");
    axum::serve(listener, app).await?;

    Ok(())
}
```

### 5. README-Korrektur

Das README verspricht aktuell:
```bash
python -m memfuse.mcp --db-path ./agent_memory
```

Dieser Befehl schlägt fehl, da es keinen Python-MCP-Server gibt. Korrigiere
das README-Beispiel auf den echten Rust-Binary-Aufruf:
```bash
cargo run --bin memfuse-mcp-server -- --db-path ./firma_daten
```

### 6. Verifikation

```bash
cargo check -p memfuse-mcp 2>&1 | tail -30
cargo build --bin memfuse-mcp-server 2>&1 | tail -30
```
```

---

## Prompt 12 — Dokumentation & README für Enterprise-Zielgruppe

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: README und Kern-Dokumentation final auf "MemFuse Brain" ausrichten

Nach Abschluss aller technischen Prompts (1-11) wird die Dokumentation final
auf die neue Positionierung ausgerichtet: **MemFuse Brain — die lokale
Enterprise-RAG-Desktop-App für KMU.**

### 1. `README.md` komplett neu strukturieren

```markdown
# MemFuse Brain

**Ihr lokaler, air-gapped Unternehmensassistent — mit professionellem Gedächtnis.**

MemFuse Brain ist eine Desktop-Applikation, die Ihre Firmendokumente
(PDF, Word, Markdown, E-Mails) durchsuchbar macht und über ein lokal
laufendes Sprachmodell (via Ollama) Fragen dazu beantwortet — komplett
offline, ohne dass ein einziges Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> getestet (LSM-Tree, HNSW, BM25). Desktop-App und Ollama-Integration
> befinden sich im Aufbau.

## Warum MemFuse Brain?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Zero-IT-Setup** — ein Installer, fertig. Kein Docker, kein Server, kein Admin
- **3-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph, fusioniert via Reciprocal Rank Fusion
- **Deutsche Morphologie** — versteht "Urlaubsantragsprozess" auch als
  "Urlaub", "Antrag", "Prozess" für bessere Trefferqualität
- **Verschlüsselt** — AES-256-GCM auf Disk, HMAC-Anti-Tamper im WAL

## Architektur

```
┌─────────────────────────────────────────┐
│  MemFuse Brain (Tauri Desktop-App)       │
│  ┌─────────────┐  ┌────────────────────┐│
│  │ Chat-UI      │  │ Dokumenten-Import  ││
│  └──────┬───────┘  └─────────┬──────────┘│
│         │                     │            │
│  ┌──────▼─────────────────────▼─────────┐ │
│  │  Ollama-Bridge (lokales LLM)         │ │
│  └──────┬─────────────────────────────  ┘ │
│         │                                  │
│  ┌──────▼─────────────────────────────┐   │
│  │  MemFuse Core (3-Signal RAG-Engine) │   │
│  │  Vektor + BM25 + Wissensgraph        │   │
│  └───────────────────────────────────┘    │
└─────────────────────────────────────────┘
         Alles lokal. Nichts verlässt den Rechner.
```

## Für Entwickler: Rust-Crates

Der Kern von MemFuse Brain ist als eigenständige, wiederverwendbare
Rust-Bibliothek verfügbar:

```toml
[dependencies]
memfuse-db = "0.1.0"
```

```rust
use memfuse_db::MemFuse;

let db = MemFuse::open("./meine_daten").await?;
let col = db.collection("dokumente").await?;

col.insert("doc-1", &embedding, Some(serde_json::json!({"text": "..."}))).await?;

let results = col.hybrid_search("meine Anfrage", &query_embedding, 5, None).await?;
```

## MCP-Server (für Claude Desktop & andere MCP-Clients)

```bash
cargo run --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Roadmap

- [x] LSM-Tree-Storage mit MVCC, WAL, Crash-Recovery
- [x] HNSW-Vektorindex mit SIMD-Beschleunigung
- [x] BM25-Volltextsuche mit deutscher Morphologie
- [x] 3-Signal-Fusion (Vektor + BM25 + Wissensgraph) — persistiert & integriert
- [x] Dokumenten-Ingestion (PDF, DOCX, Markdown, E-Mail)
- [ ] Tauri-Desktop-App (UI in aktivem Aufbau)
- [ ] Ollama-Chat-Integration mit Streaming
- [ ] MCP-Server (axum/SSE)
- [ ] Native Installer (Windows/macOS/Linux)

## Lizenz

MIT OR Apache-2.0
```

### 2. `docs/ARCHITECTURE.md` aktualisieren

Ergänze einen neuen Abschnitt am Anfang, der die Tauri-Shell und den
`memfuse-mcp`-Server als neue Architektur-Layer dokumentiert:

```markdown
## Architektur-Update: Desktop-Applikation (MemFuse Brain)

Über dem bestehenden 3-Schichten-Modell (Triebwerk/Getriebe/Fassade) liegt
nun eine vierte Schicht:

**Layer 4 — Anwendung**:
- `memfuse-tauri`: Desktop-Shell, IPC-Commands, Ingestion-Pipeline, Ollama-Bridge
- `memfuse-mcp`: Standalone MCP-Server (axum/SSE) für externe LLM-Clients

Diese Schicht kennt `memfuse-db` nur über dessen öffentliche API — keine
Layer-Verletzung nach unten.
```

### 3. `docs/SOURCE_OF_TRUTH.md` — Status-Block aktualisieren

Ersetze den bisherigen Status-Block (falls aus einer früheren Iteration
vorhanden) durch:

```markdown
## Aktueller Projektstatus (Stand: nach Enterprise-Pivot v2)

**Produkt**: MemFuse Brain — lokale, air-gapped RAG-Desktop-App für KMU  
**Kern-USP**: Echtes 3-Signal-Hybrid-RAG (Vektor+BM25+Graph), persistiert,
in `hybrid_search()` fusioniert — kein Marketing-Versprechen mehr, sondern
verifizierter Code-Zustand.

### Aktive Crates
- `memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-text`,
  `memfuse-crypto`, `memfuse-checkpoint` — Foundation (unverändert stabil)
- `memfuse-graph` — CSR-Graph, jetzt persistent (LSM-Namespace `__graph:`)
- `memfuse-db` — 3-Signal-Fusion inkl. Graph-Traversal
- `memfuse-tauri` — Desktop-App-Shell, Ingestion, Ollama-Bridge
- `memfuse-mcp` — Standalone MCP-Server

### Bekannte Einschränkungen (ehrlich dokumentiert)
- `memfuse-embed` (ONNX) ist weiterhin nicht buildbar ohne manuelle
  Feature-Flag-Aktivierung — aktuell wird Ollama als primärer Embedding-Weg
  genutzt, ONNX bleibt optionaler Zukunftsausbau.
- Der MCP-Server unterstützt aktuell nur Text-Suche ohne Embedding-Insert,
  da er keinen eingebetteten Embedder hält.
```

### 4. Verifikation

```bash
grep -n "create_collection(" README.md  # darf keine Treffer mehr finden
grep -n "python -m memfuse.mcp" README.md  # darf keine Treffer mehr finden
```
```

---

## Ausführungshinweise für Jules

1. **Reihenfolge ist bindend** bei den Abhängigkeiten: 1→2→3→4→5 ist eine
   strikte Kette (jeder Schritt baut auf dem vorherigen API-Zuwachs auf).
2. **Prompt 8** (Morphologie) kann jederzeit parallel zu 6/7/9/10 laufen,
   da es ein isoliertes Modul ohne Abhängigkeiten zu den anderen ist.
3. **Prompt 11** (MCP-Server) kann direkt nach Prompt 4 laufen, unabhängig
   von der Tauri-Schiene (6/7/9/10) — beide konsumieren nur `memfuse-db`.
4. Nach jedem Prompt sollte Jules kurz zusammenfassen, **welche Annahmen**
   getroffen wurden, wenn eine im Prompt beschriebene API-Signatur nicht
   exakt mit dem realen Code übereinstimmt — das erleichtert Review.
5. Bei `memfuse-tauri`-bezogenen Prompts (6, 7, 9, 10): Falls die Jules-
   Umgebung keine System-WebView-Libraries hat, ist `cargo check` (nicht
   `cargo build`) das richtige Erfolgskriterium — dies im PR vermerken.

**Geschätzter Gesamtaufwand**: 20–30 Stunden Jules-Laufzeit für alle 12
Prompts, da die Tauri-Integration deutlich umfangreicher ist als die
vorherige Python-Bindings-Strategie.
