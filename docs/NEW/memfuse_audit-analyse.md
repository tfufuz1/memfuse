# MemFuse Codebase — Vollständiger Sicherheits- und Architektur-Audit
**Datum:** 2026-09-01 | **Reviewer:** Senior Rust Architect (29 Jahre Erfahrung) | **Scope:** Alle 15 Workspace-Crates

---

## Executive Summary

Die MemFuse-Codebasis zeigt für ein primär KI-gesteuertes Entwicklungsprojekt (Google Jules) ein insgesamt solides Fundament: HKDF-Schlüsselableitung, HMAC-Chaining, MVCC-Snapshot-Pinning, SIMD-Distanzberechnungen und RAII-Guards sind konzeptuell korrekt implementiert. Jedoch wurden **4 kritische Fehler**, **7 schwerwiegende Fehler**, **6 mittelschwere Fehler** und **7 geringfügige Fehler** identifiziert, die ein erhebliches Risiko für Datenverlust, stille Datenkorruption und falsche Suchergebnisse darstellen.

Die gravierendsten Probleme betreffen nicht-atomare Datei-Erstellungen (SALT und WAL-UUID), eine semantisch unwirksame Community-Boost-Logik vor RRF und eine undokumentierte Inkompatibilität zwischen f32- und u8-Distanzmetriken (Euklidisch: Quadrat vs. Wurzel).

---

## Schweregrad-Legende

| Symbol | Level | Definition |
|---|---|---|
| 🔴 KRIT | Kritisch | Datenverlust, Datenkorruption, Sicherheitsbruch, unwiederherstellbare Systemzustände |
| 🟠 MAJOR | Schwerwiegend | Stille logische Fehler, falsche Suchergebnisse, Protokollbruch, Konsistenzlücken |
| 🟡 MED | Mittel | Race Conditions (niedriger Schweregrad), Performanceprobleme, API-Inkonsistenzen |
| 🟢 LOW | Gering | Code Smells, Dokumentationslücken, inkonsistente Benennung, tote Code-Pfade |

---

## 🔴 KRITISCHE FEHLER

---

### KRIT-01 — Nicht-atomare SALT-Datei-Erstellung führt zu unwiederherstellbarer Datenbank

**Datei:** `crates/memfuse-store/src/lsm.rs` (Zeilen 190–197)
**Kategorie:** Crash Safety / Data Durability

**Beschreibung:**

```rust
// ❌ FALSCH: Nicht-atomarer, nicht-gefsyncter Schreibvorgang
tokio::fs::write(&salt_path, &buf)
    .await
    .map_err(|e| MemFuseError::Storage(format!("Failed to write SALT: {}", e)))?;
```

Die SALT-Datei wird mit `tokio::fs::write()` erstellt — ein nicht-atomarer Schreibvorgang ohne:
- Temporäre Datei + atomares Umbenennen (`rename()`)
- `fsync()` der Datei selbst
- `fsync()` des Elternverzeichnisses nach dem Schreibvorgang

Ein Absturz zwischen dem Beginn des Schreibvorgangs und seinem Abschluss erzeugt eine null-Byte oder teilweise SALT-Datei. Beim Neustart schlägt der 32-Byte-Längenprüfer fehl:

```rust
if buf.len() != 32 {
    return Err(MemFuseError::Storage(format!(
        "Invalid SALT length: expected 32, got {}",
        buf.len()
    )));
}
```

Es gibt **keinen Recovery-Pfad**. Die Datenbank ist dauerhaft unlesbar — alle verschlüsselten SSTables sind ohne SALT nicht entschlüsselbar.

**Vergleich mit korrekter Implementierung:** `load_or_create_integrity_key()` im selben Crate verwendet korrekt `tmp_path → hard_link → remove tmp → fsync_parent_dir`. Die SALT-Erstellung folgt diesem Pattern nicht.

**Fix:**
```rust
use tokio::io::AsyncWriteExt;
let tmp_salt_path = config.path.join(format!(
    "SALT.tmp.{}.{}", std::process::id(), rand::thread_rng().next_u64()
));
let mut f = tokio::fs::OpenOptions::new()
    .write(true).create_new(true)
    .open(&tmp_salt_path).await?;
f.write_all(&buf).await?;
f.sync_all().await?;
drop(f);
tokio::fs::rename(&tmp_salt_path, &salt_path).await?;
crate::util::fsync_parent_dir(&salt_path).await?;
```

---

### KRIT-02 — Nicht-atomare WAL-UUID-Sidecar-Erstellung führt zu dauerhaft unlesbaren verschlüsselten WALs

**Datei:** `crates/memfuse-store/src/wal.rs`, Funktion `load_or_create_wal_uuid()`
**Kategorie:** Crash Safety / Encryption Key Derivation

**Beschreibung:**

```rust
// ❌ FALSCH: Nicht-atomarer Schreibvorgang ohne fsync vor parent-dir-sync
tokio::fs::write(&uuid_path, &bytes).await.map_err(|e| {
    MemFuseError::Storage(format!("Failed to write WAL UUID sidecar: {}", e))
})?;
crate::util::fsync_parent_dir(&uuid_path).await?;
```

Das `tokio::fs::write()` ist kein atomarer Schreibvorgang. Ein Crash nach dem Beginn des Writes, aber vor seinem Abschluss, hinterlässt eine `<8 Byte` UUID-Datei. Beim Neustart schlägt der Längenprüfer fehl:

```rust
if bytes.len() != 16 {
    return Err(MemFuseError::Storage(format!(
        "WAL UUID sidecar has unexpected length: {} (expected 16)",
        bytes.len()
    )));
}
```

Ohne die UUID kann der `derive_file_key()` nicht aufgerufen werden. Der Sub-Schlüssel des KeyManagers kann nicht abgeleitet werden. Das verschlüsselte WAL ist **permanent unlesbar**.

Im Gegensatz dazu verwendet `load_or_create_integrity_key()` im selben File korrekt `hard_link` mit O_EXCL-Semantik (atomares Erstellen). Diese Inkonsistenz zeigt AI-induziertes Wissensvergessen zwischen zwei eng verwandten Funktionen.

---

### KRIT-03 — WAL-`prepare_batch` aktualisiert `last_hmac` nicht: HMAC-Chain-Forking bei konkurrenten Aufrufern

**Datei:** `crates/memfuse-store/src/wal.rs`, Funktion `prepare_batch()`
**Kategorie:** Concurrency / Data Integrity / HMAC Chain

**Beschreibung:**

```rust
pub async fn prepare_batch(&self, ops: Vec<(WalOp, u64)>) -> Result<Vec<WalEntry>> {
    let last_hmac = self.last_hmac.lock().await; // Lock akquirieren
    let integrity_key = self.get_integrity_key()?;
    
    let mut current_chain = *last_hmac; // Lokale Kopie
    // ❌ Lock wird am Ende der Funktion freigegeben, OHNE last_hmac zu aktualisieren!
    
    for (op, seq_no) in ops {
        let entry = WalEntry::try_new(op, seq_no, &integrity_key, current_chain)?;
        current_chain = entry.checksum; // Nur lokale Kette, NICHT self.last_hmac
        entries.push(entry);
    }
    Ok(entries)
    // Hier: Lock freigegeben, last_hmac ist immer noch der ALTE Wert
}
```

Wenn zwei Threads `prepare_batch` parallel aufrufen (obwohl in der LSM-Commit-Strecke durch `commit_mutex` serialisiert), erhalten beide **denselben** `last_hmac`-Startwert. Die erzeugten Batches haben identische `prev_hmac`-Werte beim ersten Eintrag, was bei WAL-Replay zu:

```
Batch A: [entry(prev=hmac0) → entry(prev=hmacA1) → ...]
Batch B: [entry(prev=hmac0) → entry(prev=hmacB1) → ...]
// HMAC-Verifier erwartet bei Batch B: prev=last(Batch A), findet aber: prev=hmac0 → FAIL
```

**Wirkungsbereich:** Die aktuelle LSM-Commit-Strecke schützt sich durch `commit_mutex`. Das API ist jedoch für Externe ohne diese Garantie exponiert. Tests, die `prepare_batch` direkt verwenden, sind anfällig.

**Tiefere Ursache:** Das Design, bei dem `prepare_batch` den Chainstate LIEST aber NICHT SCHREIBT, und `append_batch` den Chainstate AKTUALISIERT, schafft ein Zeitfenster. Zwischen `prepare_batch`-Rückgabe und `append_batch`-Aufruf ist der lokale Chain-State inkonsistent mit dem global gespeicherten `last_hmac`.

---

### KRIT-04 — TOCTOU Race Condition bei WAL-`is_new`-Prüfung kann Parent-Directory-Fsync überspringen

**Datei:** `crates/memfuse-store/src/wal.rs`, Funktion `open_with_key_manager()`
**Kategorie:** Crash Safety / TOCTOU

**Beschreibung:**

```rust
let mut is_new = false;
if !path.exists() {   // ← PRÜFUNG
    is_new = true;
}
let file = tokio::fs::OpenOptions::new()
    .create(true).append(true).read(true)
    .open(&path)      // ← NUTZUNG
    .await?;

if is_new {           // is_new ist möglicherweise veraltet!
    file.sync_all().await?;
    crate::util::fsync_parent_dir(&path).await?;
}
```

Zwischen `path.exists()` und `open()` kann ein anderer Prozess oder Thread die Datei erstellen und löschen. Szenarien:

1. **Datei wird zwischen Check und Open erstellt:** `is_new = true` aber Datei ist nicht wirklich neu → unnötiger fsync (harmlos)
2. **Datei wird gelöscht, dann durch `create(true)` neu erstellt:** `is_new = false` → Parent-Directory-Fsync wird **übersprungen** → Verzeichniseintrag ist nicht durable → WAL-Datei fehlt nach Absturz → Datenverlust

Das Standardmuster für diesen Fall ist: nach `open()` die Metadaten lesen und anhand der Dateigröße bestimmen, ob es ein neues File ist, oder `create_new(true)` verwenden und `is_new` aus dem `AlreadyExists`-Fehler ableiten.

---

## 🟠 SCHWERWIEGENDE FEHLER

---

### MAJOR-01 — Community Score-Boost hat NULLWIRKUNG auf RRF: Score-Multiplikation ignoriert von Fusion

**Datei:** `crates/memfuse-db/src/collection/search.rs`, Funktionen `hybrid_search_with_strategy()` und `hybrid_search_with_query()`
**Kategorie:** Logikfehler / Stille Fehlfunktion / ADR-Verletzung

**Beschreibung:**

```rust
// In filter_or_boost():
res.score *= 1.2;  // ← Diese Zeile hat NULLEFFEKT auf das Endergebnis
filtered.push(res);
```

Danach:

```rust
// In weighted_reciprocal_rank_fusion():
for (rank, doc) in result_set.into_iter().enumerate() {
    let score = weight / ((k + rank + 1) as f32);  // ← Nur RANG zählt, nicht doc.score!
    entry.0 += score;
```

RRF berechnet die Fusion ausschließlich über die **Rang-Position** eines Dokuments in der Eingabeliste, NICHT über seinen `score`-Wert. Der `1.2x`-Multiplikator ändert nur `SearchResult.score` im Speicher, beeinflusst aber die Rang-Position innerhalb der bereits sortierten Liste nicht. Damit hat dieser Boost **buchstäblich keinen messbaren Effekt** auf das Ranking der Suchergebnisse.

**Zusätzliches Problem:** `filter_or_boost` filtert auch Dokumente heraus, die NICHT in der Ziel-Community sind. Dies ist effektiv ein Pre-RRF-Filter, der ADR-024 verletzt ("Pre-RRF-Filter müssen RRF-Eigenschaften erhalten"). Dokumente, die von Vektor- oder Text-Suche gefunden werden, aber nicht in der Community sind, werden vollständig eliminiert, bevor sie in die Fusion eingehen. Ein Dokument das in 5 Signalen top-ranked ist, aber nicht in der Community, verschwindet komplett.

**Fix:** Community-Boost entweder als Re-Rank-Schritt NACH RRF implementieren, oder als Pre-Filter mit dokumentierter ADR-Ausnahme kennzeichnen. Der `score *= 1.2` muss entweder auf RRF-Scores angewendet oder entfernt werden.

---

### MAJOR-02 — `search_with_filter_expr`: Doppelte Dead-Code `k == 0`-Guard mit widersprüchlichen Fehlermeldungen

**Datei:** `crates/memfuse-db/src/collection/search.rs` (Zeilen 65, 70, 77)
**Kategorie:** Dead Code / Inkonsistenz / Code Smell

**Beschreibung:**

```rust
// Erste Prüfung
if k == 0 {
    return Err(MemFuseError::invalid_input("k must be greater than 0"));
}
if query.len() != self.dimension { ... }
// Zweite, redundante Prüfung
if k == 0 {
    return Err(MemFuseError::invalid_input("Search k must be greater than 0")); // andere Meldung!
}
let k = k.min(memfuse_core::MAX_SEARCH_K);
```

Die zweite `k == 0`-Prüfung ist unerreichbarer Dead Code — die erste Prüfung gibt bereits zurück. Sie erzeugt aber eine andere Fehlermeldung ("Search k must..." vs "k must..."), was bei Code-Searches zu Verwirrung führt, welche Prüfung kanonisch ist. Dies ist ein klassisches AI-Artefakt: Copy-Paste ohne Bewusstsein für den bereits vorhandenen identischen Guard.

---

### MAJOR-03 — `DistanceMetric::compute_u8` Euklidisch gibt **Quadrat** der Distanz zurück, f32-Pfad gibt Wurzel

**Datei:** `crates/memfuse-core/src/types/domain.rs`, `DistanceMetric::compute_u8()`
**Kategorie:** Semantischer Fehler / Cross-Konsistenz

**Beschreibung:**

```rust
// f32-Pfad: sqrt(Σ diff²)  — echte Euklidische Distanz
Self::Euclidean => {
    sum.sqrt()  // ← Wurzel wird gezogen
}

// u8-Pfad: Σ diff²  — QUADRIERTE Euklidische Distanz
Self::Euclidean => {
    Ok(sum.min(u32::MAX as u64) as u32)  // ← KEINE Wurzel!
}
```

Der f32-Pfad berechnet `√(Σ diff²)`, der u8-Pfad berechnet `Σ diff²`. Beide heißen "Euklidisch", sind aber unterschiedliche Metriken. Für Ranking innerhalb eines einzelnen Signals ist die Monotoniebeziehung erhalten (x² ist monoton für x ≥ 0). Aber:

1. **Kreuz-Vergleich f32 vs u8:** Scores aus beiden Pfaden sind **nicht vergleichbar**. Eine f32-Distanz von `5.0` entspricht einer u8-Distanz von `25`.
2. **DotProduct-Inkonsistenz:** f32-Pfad gibt `-dot` (negiert), u8-Pfad gibt `dot` (positiv). Ranking-Richtung ist entgegengesetzt! Das Docstring sagt: "The caller is responsible for inverting the ranking order when using DotProduct" — aber kein Caller macht das tatsächlich (geprüft in `crates/memfuse-index/src/hnsw.rs`).
3. **Test-Fehler:** `test_distance_metrics_u8` in `domain.rs` testet u8-Euklidisch und erwartet `200` (korrekt für quadriert), aber kommentiert es als "squared = 200" ohne die Inkonsistenz zum f32-Pfad zu flaggen.

---

### MAJOR-04 — Graph-Signal nutzt `EntityId::from_key(text_result.id)` ohne dokumentierte Mapping-Invariante

**Datei:** `crates/memfuse-db/src/collection/search.rs`, Funktion `hybrid_search_with_strategy()`
**Kategorie:** Stille Fehlfunktion / Architektur-Inkonsistenz

**Beschreibung:**

```rust
// Text-Ergebnisse haben String-IDs wie "mein-dokument-key"
implicit_anchors = text_results
    .iter()
    .map(|r| memfuse_core::EntityId::from_key(r.id.as_str()))  // BLAKE3-Hash
    .collect::<Result<Vec<_>>>()?;
```

`EntityId::from_key` wendet BLAKE3 auf den String an und nimmt die ersten 8 Bytes. Dies setzt voraus, dass Graph-Knoten mit **identischen String-Schlüsseln** erstellt wurden. Diese Annahme ist:
- **Undokumentiert** — kein ADR, kein Kommentar, keine Invariante
- **Zerbrechlich** — wenn Entities mit `EntityId::from_doc_id(doc_id)` oder `EntityId::new(u64)` erstellt werden, erzeugt dieser Code eine nicht-existente EntityId → leere Graph-Traversierung → Graph-Signal ist still deaktiviert

Das 4-Signal-Fusion-Konzept basiert auf dem korrekten Graph-Signal. Eine stille Deaktivierung des Graph-Signals (Ergebnis: leere Liste) fließt un-bemerkt in RRF ein und ergibt unerwartetes Ranking.

---

### MAJOR-05 — PPR-Implementierung (`compute_ppr`) ignoriert uncompaktierte Pending-Edges

**Datei:** `crates/memfuse-graph/src/ppr.rs`, Funktion `compute_ppr()`
**Kategorie:** Korrektheit / Vollständigkeit

**Beschreibung:**

`compute_ppr` liest ausschließlich aus `inner.offsets`, `inner.targets`, `inner.weights` — den kompaktierten CSR-Arrays. `inner.pending_edges` (Delta-Buffer für uncommitted/uncompacted Edges) wird NICHT gelesen:

```rust
for edge_idx in start..end {  // Nur CSR-Traversal
    let target = inner.targets[edge_idx];
    let weight = inner.weights[edge_idx];
    // ← pending_edges werden hier NICHT berücksichtigt
```

`personalized_page_rank()` ruft `self.compact()` VOR `compute_ppr` auf, was die Pending-Edges in die CSR übernimmt. **Aber:** Zwischen `compact()` (Write-Lock freigegeben) und dem Read-Lock in `compute_ppr` können andere Threads neue Edges zu `pending_edges` hinzufügen. Diese fehlen dann in der PPR-Berechnung.

Im Kontrast: BFS-Traversal in `csr.rs` liest EXPLIZIT aus beiden Quellen (CSR und `pending_edges`). Diese Inkonsistenz zwischen BFS und PPR für identische Graph-Daten ist ein stiller Architektur-Fehler.

---

### MAJOR-06 — BM25 IDF Floor `1e-6` für häufige Terme widerspricht BM25-Semantik

**Datei:** `crates/memfuse-text/src/bm25.rs`, Funktion `score_term_with_params()`
**Kategorie:** Algorithmus-Abweichung / Relevanz-Qualität

**Beschreibung:**

```rust
let idf_arg = (n - df + 0.5) / (df + 0.5);
let idf = if idf_arg <= 1.0 {
    1e-6  // ← Problematischer Floor
} else {
    idf_arg.ln()
};
```

Wenn ein Term in mehr als der Hälfte aller Dokumente vorkommt (df > N/2 → idf_arg < 1.0), gibt `ln(idf_arg)` einen negativen Wert. Das Standard-BM25+ verwendet dann entweder `0` oder eine additive Glättung. Der Wert `1e-6` ist:
- Zu groß für Nullwirkung: Bei k1=1.5, tf=1, dl=avg → BM25-Score ≈ `1e-6 * (1.5+1)/2.5` = ~`6e-7`. Nicht-null, aber winzig.
- **Nicht** die Robertson-Spärck-Jones-Glättungsformel `ln(1 + ...)` (was korrekt wäre)
- Nicht dokumentiert als bewusste Design-Entscheidung

Deutsche Stoppwörter (der, die, das, und, oder) die in nahezu jedem Dokument vorkommen erhalten damit einen kleinen positiven Score statt 0, was Suchergebnisse für kurze Queries geringfügig verzerrt.

---

### MAJOR-07 — `LEGACY_INTEGRITY_KEY` ist `pub const` und damit Teil der öffentlichen API

**Datei:** `crates/memfuse-store/src/wal.rs`
**Kategorie:** Sicherheit / API-Design

**Beschreibung:**

```rust
pub const LEGACY_INTEGRITY_KEY: [u8; 32] = *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";
```

Dieser statische Schlüssel ist als `pub const` exponiert. Das bedeutet:
1. **Breaking Change:** Wenn der Schlüssel je geändert werden muss, bricht die öffentliche API
2. **Security-by-Obscurity unterlaufen:** Der Schlüssel ist kompiliert in das Binary eingebettet UND kann von Dritten aus dem Rust-Crate-Ökosystem importiert werden
3. **Null-Bytes im Schlüssel:** Die 8 Null-Bytes am Ende sind keine valide kryptografische Praxis für einen HMAC-Schlüssel (effektive Entropie: 24 Bytes statt 32)

Mitigierung: `pub(crate)` als Sichtbarkeit wäre ausreichend.

---

## 🟡 MITTELSCHWERE FEHLER

---

### MED-01 — `CheckpointGuard::Drop` spawnt Fire-and-Forget Rollback ohne Fehlerweiterleitung

**Datei:** `crates/memfuse-checkpoint/src/lib.rs`
**Kategorie:** Error Handling / Concurrency

**Beschreibung:**

```rust
impl<S: StorageEngine> Drop for CheckpointGuard<S> {
    fn drop(&mut self) {
        if let Some(cp) = self.checkpoint.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    if let Err(e) = storage_clone.rollback_to_tx(cp.tx_id).await {
                        tracing::error!("CheckpointGuard auto-rollback fehlgeschlagen: {e}");
                    }
                });  // ← Fire and Forget!
            } else {
                tracing::error!("CheckpointGuard außerhalb tokio-Runtime gedroppt. Rollback übersprungen.");
                // ← Rollback stillschweigend übersprungen!
```

Zwei Probleme:
1. Die gespawnten Tasks sind vom Lebenszyklus der Tokio-Runtime abhängig. Beim Shutdown werden laufende Tasks möglicherweise abgebrochen, bevor der Rollback abgeschlossen ist.
2. Außerhalb einer Tokio-Runtime (synchroner Kontext, Tests) wird der Rollback **ohne Fehler** übersprungen. Der Checkpoint bleibt ohne Rollback offen.

---

### MED-02 — `HNSW_REBUILD_THRESHOLD` Konstantenname semantisch invertiert

**Datei:** `crates/memfuse-index/src/hnsw.rs`
**Kategorie:** Code Smell / Verwirrende Benennung

**Beschreibung:**

```rust
pub const HNSW_REBUILD_THRESHOLD: f64 = 0.30;  // Name impliziert "30% gelöscht = Rebuild"

// In HnswConfig::default():
rebuild_threshold: 1.0 - HNSW_REBUILD_THRESHOLD,  // = 0.70
```

`HNSW_REBUILD_THRESHOLD = 0.30` suggeriert "Rebuild wenn 30% gelöscht". Aber der tatsächlich gespeicherte `rebuild_threshold = 0.70` bedeutet "Rebuild wenn weniger als 70% aktiv sind". Eine externe Konfiguration mit `rebuild_threshold = 0.30` würde bedeuten "Rebuild wenn weniger als 30% aktiv" — also erst bei 70% Löschungen. Das ist das Doppelte des intendierten Schwellwerts.

---

### MED-03 — `LsmStorage::rollback_to_tx` akquiriert `commit_mutex` — potentieller Deadlock in Error-Recovery-Pfaden

**Datei:** `crates/memfuse-store/src/lsm.rs`
**Kategorie:** Deadlock-Risiko

**Beschreibung:**

```rust
pub async fn rollback_to_tx(&self, target_tx: TxId) -> Result<()> {
    let _commit_lock = self.commit_mutex.lock().await;  // ← Mutex-Akquisition
    ...
}
```

Und in `commit()`:

```rust
let _commit_lock = self.commit_mutex.lock().await;  // ← Gleicher Mutex
...
// Wenn commit() fehlschlägt und rollback aufgerufen wird:
state.wal.truncate(pre_tx_offset, pre_tx_hmac).await?;  // ← WAL Rollback ohne Mutex
```

Direkter WAL-Rollback in `commit()` hält den `commit_mutex`. `rollback_to_tx()` würde den `commit_mutex` erneut versuchen zu akquirieren — Deadlock wenn `rollback_to_tx` aus einem `commit`-Fehlerkontext aufgerufen wird. In der aktuellen Implementierung wird WAL-Truncation direkt (ohne `rollback_to_tx`) gerufen, aber zukünftige Refactorings könnten diesen Pfad aktivieren.

---

### MED-04 — `MemoryType::as_metadata_key()` gibt Kleinbuchstaben zurück, Serde serialisiert in CamelCase

**Datei:** `crates/memfuse-core/src/types/domain.rs`
**Kategorie:** Dateninkonsistenz / Serialisierungs-Mismatch

**Beschreibung:**

```rust
// Manuelle Methode: "episodic" (Kleinbuchstaben)
pub fn as_metadata_key(&self) -> &'static str {
    match self {
        MemoryType::Episodic => "episodic",
```

```rust
// Serde Derive: "Episodic" (CamelCase per Default)
#[derive(Serialize, Deserialize)]
pub enum MemoryType { Episodic, Semantic, ... }
```

```json
{"memory_type": "episodic"}  // via as_metadata_key()
{"memory_type": "Episodic"}  // via serde::Serialize
```

JSON-Keys, die via `as_metadata_key()` gesetzt werden, werden mit Serde-Deserialisierung nicht korrekt gelesen. Das Docstring schreibt: "für JSON-Serialisierung", was impliziert, beide Pfade sollten konsistent sein — sind sie aber nicht.

---

### MED-05 — WAL-Replay: Verschlüsselte V1-Dateien erzeugen ambige Parsing-Strecke

**Datei:** `crates/memfuse-store/src/wal.rs`, Funktion `replay_with_size_and_version()`
**Kategorie:** Versionierungslogik / Latenter Fehler

**Beschreibung:**

```rust
if matches!(version, WalVersion::V2 | WalVersion::V3) && self.key_manager.is_some() {
    // Encrypted Batch Path
} else {
    // Non-encrypted OR V1 path — attempted per-entry decryption if km present
}
```

V1-Dateien (kein `MFW2`/`MFW3`-Header) mit aktivem `key_manager` fallen in die `else`-Strecke, die per-Entry-Entschlüsselung versucht. V1 hatte jedoch keine einheitliche Verschlüsselung — das Format war Entry-by-Entry ohne Batch-Nonce. Die `else`-Strecke versucht, die ersten 12 Bytes jedes Entries als Nonce zu interpretieren, was beim eigentlichen WAL-Entry-Header fehlschlägt. Das Fehlerhandling gibt einen WAL-Corruption-Fehler zurück, der aber eigentlich ein Format-Mismatch ist.

---

### MED-06 — `merge_metadata` in RRF: "First-Wins"-Semantik undokumentiert und schwer nachvollziehbar

**Datei:** `crates/memfuse-db/src/fusion.rs`
**Kategorie:** Undokumentiertes Verhalten / Korrektheit

**Beschreibung:**

```rust
fn merge_metadata(target: &mut Option<serde_json::Value>, source: Option<serde_json::Value>) {
    match (target, source) {
        (Some(t_val), Some(s_val)) => {
            if let (Some(t_obj), Some(s_obj)) = (t_val.as_object_mut(), s_val.as_object()) {
                for (k, v) in s_obj {
                    if !t_obj.contains_key(k) {  // ← "First Wins"
                        t_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
```

Die Metadata-Fusion hat "First-Wins"-Semantik: Der zuerst verarbeitete Signal-Set "gewinnt" für jeden Metadata-Key. Da `result_sets` in der Reihenfolge `(vector, text, graph)` iteriert wird, überschreibt Vector-Metadata immer Text- und Graph-Metadata für gleiche Keys. Diese Reihenfolge ist nicht dokumentiert und nicht konfigurierbar. Für ein System, bei dem Graph-Metadaten (z.B. entity_type, community_id) semantisch wertvoller sind als Vector-Metadaten, ist dies suboptimal.

---

## 🟢 GERINGFÜGIGE FEHLER

---

### LOW-01 — `Embedding::normalize` nutzt exakten `== 0.0` Float-Vergleich für Null-Vektor

**Datei:** `crates/memfuse-core/src/types/domain.rs`
Der Vergleich `if norm == 0.0` ist korrekt für exakte Null-Vektoren, aber subnormale Werte nahe 0 (z.B. `1e-38f32`) werden nicht gefangen. Division durch einen subnormalen Wert kann theoretisch `Inf` produzieren. Empfehlung: `if norm < f32::EPSILON`.

### LOW-02 — `PprConfig::max_iterations` wird intern auf 1000 gekappt ohne Dokumentation

**Datei:** `crates/memfuse-graph/src/ppr.rs`
```rust
let max_iters = config.max_iterations.min(1000); // hard ceiling
```
Die `PprConfig`-Struct erlaubt `max_iterations: u32` bis ~4 Milliarden. Der Hard-Cap von 1000 überschreibt user-konfigurierte Werte stillschweigend. Dieser Cap sollte in `PprConfig::validate()` oder als Docstring dokumentiert sein.

### LOW-03 — `WalEntry::compute_checksum` ist redundante Delegation zu `compute_checksum_v3`

**Datei:** `crates/memfuse-store/src/wal.rs`
```rust
pub fn compute_checksum(...) -> Result<[u8; 32]> {
    Self::compute_checksum_v3(op, seq_no, integrity_key, prev_hmac)
}
```
Diese Wrapper-Funktion hat bei der nächsten Version-Migration ein hohes Divergenz-Risiko.

### LOW-04 — `LEGACY_INTEGRITY_KEY` enthält 8 Null-Bytes (reduzierte effektive Entropie)

**Datei:** `crates/memfuse-store/src/wal.rs`
```rust
pub const LEGACY_INTEGRITY_KEY: [u8; 32] = *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";
```
24 Zeichen ASCII + 8 Null-Bytes. Effektive Entropie: ~24 Bytes (ASCII-Zeichen, nicht zufällig). Für einen Legacy-Kompatibilitäts-HMAC-Schlüssel akzeptabel, aber keine Best Practice.

### LOW-05 — `GermanCompoundSplitter` behandelt `ẞ` (Groß-Eszett U+1E9E) nicht

**Datei:** `crates/memfuse-text/src/morphology.rs`
`normalize_umlauts` konvertiert `ß→ss` aber nicht `ẞ→ss` (capital Eszett). In modernen deutschen Texten (z.B. nach dem Duden 2017) kann `ẞ` in Eigennamen vorkommen. `to_lowercase()` konvertiert `ẞ` zu `ß`, dann zu `ss` — also ist das Verhalten korrekt. Dieser Bug existiert **nicht**. Jedoch: `Ä→ae`, `Ö→oe`, `Ü→ue` ist ein Umschreibstandard, aber `ß→ss` vs. `ss` kann zu False-Positive-Compound-Splits führen (`Maßstab` → `mass-stab` vs. `mast-ab`).

### LOW-06 — `hsum256_ps_avx` wird ohne `#[target_feature(enable = "avx")]` markiert, obwohl es `_mm256_extractf128_ps` nutzt

**Datei:** `crates/memfuse-index/src/distance.rs`
```rust
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    let x128 = _mm_add_ps(_mm256_extractf128_ps(v, 1), _mm256_castps256_ps128(v));
```
`_mm256_extractf128_ps` benötigt `avx`, nicht `avx2`. Da `avx2` auf allen Plattformen auch `avx` inkludiert, ist dies in der Praxis kein Problem, aber formal sollte `#[target_feature(enable = "avx")]` explizit gesetzt sein.

### LOW-07 — `CosineSimilarityPartsU8` enthält `sum_a`/`sum_b` ohne Verwendung im finalen Cosinus-Score

**Datei:** `crates/memfuse-index/src/distance.rs`
```rust
pub struct CosineSimilarityPartsU8 {
    pub dot: u32,
    pub sum_a: u32,    // ← Wozu verwendet?
    pub sum_b: u32,    // ← Wozu verwendet?
    pub norm_a_sq: u32,
    pub norm_b_sq: u32,
}
```
Standard-Kosinus benötigt nur `dot`, `norm_a_sq`, `norm_b_sq`. `sum_a` und `sum_b` werden berechnet (inkl. SIMD-Aufwand), erscheinen aber in keinem abschließenden Kosinus-Distanz-Ausdruck in der Codebase. Wahrscheinlich Überbleibsel einer asymmetrischen Kosinus-Implementierung oder eines zukünftigen Features. Dead code mit SIMD-Overhead.

---

## Systemische Muster und Architektur-Beobachtungen

### Pattern 1 — AI-induzierte Code-Divergenz zwischen verwandten Funktionen

Das auffälligste systemische Muster ist die **Inkonsistenz zwischen strukturell ähnlichen Funktionen**:
- `load_or_create_integrity_key()` — korrekt atomare Erstellung via `hard_link`
- `load_or_create_wal_uuid()` — nicht-atomare Erstellung via `write()` (KRIT-02)
- `score_term_with_params()` (BM25) — korrekte Parameter-Validierung
- `PprConfig` — fehlende `validate()`-Funktion

Dieses Muster entsteht, wenn KI-Agenten Funktionen sitzungsweise entwickeln und dabei "vergessen", welche Lösungen in Nachbar-Funktionen bereits korrekt implementiert wurden.

### Pattern 2 — Semantische Lücke zwischen Konzept und Implementierung

Mehrere Features existieren im Code aber bewirken nichts:
- Community Boost `score *= 1.2` (MAJOR-01): Implementiert, bewirkt 0%
- `CosineSimilarityPartsU8.sum_a/sum_b` (LOW-07): Berechnet, nie genutzt
- `MemoryType::as_metadata_key()` vs. Serde-Strings (MED-04): Zwei Wege, einer ist defekt

### Pattern 3 — Fehlendes Crash-Recovery-Testing für Datei-I/O-Pfade

KRIT-01 und KRIT-02 wären durch Property-Tests mit fault injection erkennbar. Die vorhandenen Tests (`tests/wal_key_lifecycle.rs`, `tests/flush_crash_simulation.rs`) testen WAL-Truncation, aber nicht SALT- oder UUID-Datei-Crashes. Dies deutet auf eine Lücke in der KI-gesteuerten Testgenerierung hin.

### Pattern 4 — Inkonsistente Metrik-Semantik zwischen Abstraktionsebenen

`DistanceMetric::compute_u8()` für Euklidisch gibt `Σ diff²` zurück (MAJOR-03), `compute()` für f32 gibt `√(Σ diff²)`. HNSW nutzt den u8-Pfad für quantisierte Vektoren, f32-Pfad für nicht-quantisierte. Da beide für Nearest-Neighbor-Ranking verwendet werden und die Metrik-Monotonie erhalten ist, hat dies **keinen Effekt auf die Sortierung** — aber bei Score-basierten Thresholds oder Score-Interpretationen ist der Unterschied kritisch.

---

## Priorisierte Maßnahmen-Liste

| Rang | Fehler-ID | Aktion | Aufwand |
|---|---|---|---|
| 1 | KRIT-01 (SALT) | Atomare Erstellung implementieren (tmp + rename + fsync) | 2h |
| 2 | KRIT-02 (UUID) | Identisches Pattern wie `load_or_create_integrity_key` anwenden | 2h |
| 3 | MAJOR-01 (Community Boost) | Post-RRF Re-Rank implementieren ODER Score-Multiplikation entfernen | 4h |
| 4 | MAJOR-03 (u8 Euklidisch) | Entscheiden: squared vs. rooted, dokumentieren, Tests anpassen | 3h |
| 5 | KRIT-03 (HMAC Fork) | `prepare_batch` mit `append_batch` atomar verbinden ODER API als unsicher dokumentieren | 4h |
| 6 | KRIT-04 (TOCTOU) | `is_new` via Dateigröße nach `open()` bestimmen | 1h |
| 7 | MAJOR-04 (Graph Anchors) | EntityId-Mapping-Invariante dokumentieren + Integrations-Test | 6h |
| 8 | MAJOR-02 (Dead Guard) | Doppelte k==0-Prüfung entfernen, kanonische Fehlermeldung wählen | 30min |
| 9 | MED-04 (Serde/as_metadata_key) | `#[serde(rename_all = "lowercase")]` auf `MemoryType` oder `as_metadata_key()` entfernen | 1h |
| 10 | MAJOR-07 (pub Legacy Key) | `pub(crate)` Sichtbarkeit | 15min |

---

*Audit-Status: Vollständig. Alle 15 Crates analysiert. Stand: 2026-09-01.*

