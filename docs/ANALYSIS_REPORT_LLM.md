# MemFuse Projekt-Analysebericht (Vollständige LLM-Spezifikation)

## 1. System-Übersicht & Architektur
MemFuse ist eine eingebettete Hybrid-Search-Datenbank, die speziell für den Einsatz in KI-Agenten-Workflows (SAOS - Sovereign Agent Operating System) entwickelt wurde.

### Architektur-Schichten (DAG-Konform)
1.  **Foundation (Layer 0)**: `memfuse-core`, `memfuse-crypto`
2.  **Engines (Layer 1)**: `memfuse-store` (LSM), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-graph` (CSR)
3.  **Orchestration (Layer 2)**: `memfuse-db` (Collections, 2PC, Fusion)
4.  **Integration (Layer 3+)**: `memfuse-py`, `memfuse-saos-agent`, `memfuse-sandbox`, `memfuse-checkpoint`, `memfuse-cluster`, `memfuse-embed`

---

## 2. Detaillierte Crate-Analyse

### 2.1 memfuse-core (Das Fundament)
*   **Zweck**: Definition gemeinsamer Datentypen und Schnittstellen.
*   **Kern-Komponenten**:
    *   `DocId`, `TxId`, `EntityId`: Typsichere Wrapper um `u64`. `DocId` wird oft via Blake3-Hash aus einem String-Key generiert.
    *   `TxBuffer<T>`: Ein generischer, nach `TxId` geshradeter Puffer (Default 64 Shards). Er erlaubt parallele Schreibvorgänge in verschiedenen Transaktionen ohne Lock-Interferenz.
    *   `SnapshotRegistry`: Implementiert MVCC-Sichtbarkeit. Verwaltet eine Map von aktiven `seq_no`s und berechnet das Minimum. Dies ist kritisch für die `CompactionEngine`, um zu entscheiden, welche Tombstones gelöscht werden dürfen.
    *   `DistanceMetric`: Enum für `Cosine`, `Euclidean`, `DotProduct`. Implementiert sowohl `f32` als auch skalierte `u32` (für quantisierte Berechnungen) Distanzlogik.
*   **Fehler/Lücken**:
    *   Die Standard-Implementierung von `scan_prefix_at` im `StorageEngine`-Trait wirft einen `PolicyViolation`-Fehler. Dies zwingt Implementierer dazu, MVCC-konforme Scans explizit zu bauen.

### 2.2 memfuse-crypto (Die Sicherheitsschicht)
*   **Zweck**: Kryptographische Härtung des gesamten Stacks.
*   **Kern-Komponenten**:
    *   `KeyManager`: Nutzt HKDF-SHA256, um aus einer Passphrase und einem Salt Datei-spezifische Schlüssel abzuleiten.
    *   `VolatileEncryptionKey`: Schützt Schlüssel im RAM durch den `zeroize`-Trait. Verhindert, dass Schlüssel nach Gebrauch im Speicher verbleiben (Cold-Boot Schutz).
    *   `IntegrityVerifier`: Validiert eine HMAC-SHA256 Kette im WAL. Jeder Eintrag enthält den HMAC des vorherigen Eintrags (`prev_hmac`), was die Kette manipulationssicher macht.
*   **Status**: Exzellent. Nutzt moderne Primitiven (AES-GCM-SIV) und vermeidet `unsafe` komplett.

### 2.3 memfuse-store (Die Persistenz-Engine)
*   **Zweck**: Dauerhafte Speicherung von Key-Value Paaren (LSM-Tree).
*   **Kern-Komponenten**:
    *   `Wal`: Write-Ahead-Log mit Integritätsschutz. Unterstützt atomare Batches.
    *   `MemTable`: In-Memory BTreeMap. Nutzt `VersionedData` (u64, Bytes, u64), um `seq_no` und `tx_id` pro Version zu speichern.
    *   `Sstable`: Immutable Dateien auf Disk. Enthalten Bloom-Filter für schnelles Skipping und einen Index-Block. Der Trailer (36 Bytes) speichert die `TxId`-Grenzwerte.
    *   `CompactionEngine`: Implementiert Size-Tiered Compaction (STCS). Wichtig: Tombstones werden nur bei einer "Full Compaction" gelöscht, wenn sie älter als der älteste aktive Snapshot sind.
*   **Fehler/Lücken**:
    *   `LsmStorage::scan` führt Merges über MemTables und SSTables durch. Die Sichtbarkeitsprüfung gegen `last_committed_tx` ist für SSTables implizit (da sie nur Commits enthalten), aber für Konsistenz-Checks bei Rollbacks könnte eine explizite Prüfung pro Eintrag (wie in der MemTable) sicherer sein.

### 2.4 memfuse-index (Die Vektor-Engine)
*   **Zweck**: Hochperformante ANN-Suche.
*   **Kern-Komponenten**:
    *   `HnswIndex`: Implementiert den Hierarchical Navigable Small World Algorithmus.
    *   `distance.rs`: Enthält SIMD-optimierte (AVX2, AVX-512) Distanzfunktionen. Nutzt dynamischen Dispatch.
    *   `ScalarQuantizer`: Trainiert on-the-fly (nach ca. 50-256 Inserts) und transformiert `f32` Vektoren in `u8` (SQ8). Dies reduziert den Speicherbedarf drastisch.
    *   `MmapIndex`: Erlaubt das direkte Suchen auf disk-basierten HNSW-Strukturen via Memory Mapping.
*   **Status**: Sehr ausgereift. Die Rebuild-Logik (bei >20% Deletions) sorgt für langfristige Graph-Gesundheit.

### 2.5 memfuse-text (Die Volltext-Engine)
*   **Zweck**: Keyword-Suche mit BM25.
*   **Kern-Komponenten**:
    *   `InvertedIndex<S>`: Nutzt die `StorageEngine` zur Speicherung von Posting-Lists.
    *   `GermanCompoundSplitter`: Zerlegt komplexe deutsche Wörter. Logik: Wenn ein Wort mit einem bekannten Präfix (aus einem internen Wörterbuch) beginnt, wird es gesplittet.
    *   `Bm25Scorer`: Berechnet Relevanz-Scores basierend auf TF-IDF.
*   **Lücken**:
    *   Integrationstests schlagen fehl, weil die `MockStorage` in den Tests `scan_prefix_at` nicht korrekt implementiert (wirft `PolicyViolation`).

### 2.6 memfuse-graph (Die Relations-Engine)
*   **Zweck**: Signal 3 (Relationale Relevanz).
*   **Kern-Komponenten**:
    *   `CsrGraph`: Nutzt Compressed Sparse Row Format. Optimiert für BFS-Traversierung.
    *   Score-Decay: Jede Kante (Hop) multipliziert den Score mit 0.7.
*   **Status**: Funktional, aber rein In-Memory.

### 2.7 memfuse-db (Die Orchestrierung)
*   **Zweck**: Zusammenführung aller Signale.
*   **Kern-Komponenten**:
    *   `DbTransaction`: Das "Gehirn" für Atomarität. Implementiert einen 2PC-ähnlichen Ablauf:
        1. `CommitIntent::Pending` in LSM schreiben.
        2. LSM-Commit.
        3. HNSW-Commit.
        4. Bei Erfolg: `CommitIntent::Committed` schreiben.
    *   `Collection`: Bietet Namespacing und `repair()`-Logik. `repair()` liest alle Dokumente aus dem LSM und stellt sicher, dass sie im HNSW-Index vorhanden sind (Forward-Recovery nach Crash).
    *   `fusion.rs`: Implementiert Reciprocal Rank Fusion (RRF).

---

## 3. Geschäftslogik: "Funktioniert alles nach Plan?"

### Status-Check:
*   **Atomarität**: Ja, durch `DbTransaction` und `repair_on_open`.
*   **Isolation**: Ja, durch `SnapshotRegistry` (MVCC) und Namespace-Prefixes.
*   **Dauerhaftigkeit**: Ja, durch HMAC-gechainten WAL und SSTable FSyncs.
*   **Hybrid-Search**: Ja, RRF fusioniert Vektor-, Text- und Graph-Ergebnisse korrekt.

### Identifizierte Schwachstellen & Lücken:
1.  **Graph-Persistenz**: Im Gegensatz zu Vektoren und Text werden Graph-Kanten (CSR) nicht automatisch beim `MemFuse::open` aus dem LSM rehydriert. Der Graph ist nach Neustart leer.
2.  **OOM-Handling**: Der `ResourceTracker` blockiert bei 95% Belegung weitere Schreibvorgänge, löst aber keinen automatischen "Emergency Flush" oder Cache-Eviction aus.
3.  **Tombstone-Latenz**: Im Inverted Index werden Posting-Lists bei Updates nicht sofort bereinigt (Lazy Tombstones). Die `resolve_tombstones()` Methode muss manuell oder durch Hintergrund-Jobs aufgerufen werden, um Speicherplatz freizugeben.
4.  **Inkonsistente Trait-Defaults**: Der Fehler in `scan_prefix_at` behindert die Testbarkeit von Komponenten, die gegen den Trait statt gegen die konkrete `LsmStorage` testen.

---

## 4. Handlungsanweisungen für die Weiterentwicklung (LLM-Guide)

1.  **Neuer Trait-Implementierer**: Wenn du eine neue `StorageEngine` baust, implementiere zwingend `scan_prefix_at` unter Berücksichtigung der `seq_no`, sonst brechen die Text-Suche und das Checkpointing.
2.  **Transaktionen**: Nutze immer die `Collection::insert_op` / `update_op` Muster innerhalb einer `DbTransaction`, um die Synchronität zwischen LSM und HNSW zu wahren.
3.  **Graph-Daten**: Erweitere `Collection::load_index`, um auch Graph-Relationen (`__rel:`) aus dem LSM in den `CsrGraph` zu laden.
4.  **SIMD**: Bei Änderungen an `distance.rs` achte auf die deterministische EPSILON-Grenze von `1e-4` im Vergleich zum Skalar-Fallback.

---
*Ende des Berichts. Erstellt durch Senior Lead Rust Systemarchitekt Jules.*
