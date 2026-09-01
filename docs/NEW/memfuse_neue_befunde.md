# MemFuse — Ergänzender Audit: Neu identifizierte, bisher unbekannte Fehler

**Datum:** 2026-09-01
**Reviewer:** Senior Rust-Architekt (Fokus: Concurrency, Storage-Engines, Vektor-Suche)
**Repository:** `https://github.com/tfufuz1/memfuse` (frischer Klon, HEAD zum Analysezeitpunkt)
**Abgrenzung:** Alle in `memfuse_audit-analyse.md` (KRIT-01…LOW-07) und `memfuse_audit-analyse-2.md`
(BUG-GRA-003, BUG-AGT-001, BUG-GRA-004, BUG-RTR-001, BUG-AGT-002, BUG-CKP-001, BUG-AGT-003,
BUG-TXT-001…004, BUG-DB-001, BUG-RTR-002, BUG-AGT-004, BUG-STR-001, BUG-GRA-005) genannten
Befunde werden hier **bewusst ausgeklammert**. Dieses Dokument enthält ausschließlich neue,
eigenständig identifizierte Fehler.

---

## Executive Summary

Bei einer gezielten Tiefenprüfung der bisher am wenigsten auditierten Pfade — insbesondere
`memfuse-index::hnsw::search_filtered()` und der SQ8-Quantisierungs-Bound-Expansion in
`do_insert()` — wurden **2 neue, eigenständige Fehler** mit unmittelbarer Relevanz für
Korrektheit bzw. Suchqualität gefunden:

| ID | Schwere | Kategorie | Kurzbeschreibung |
|---|---|---|---|
| NEU-01 | 🔴 KRIT | Logikfehler / Zugriffskontrolle | `search_filtered()` überspringt die `deleted_nodes`-Tombstone-Prüfung **immer**, sobald ein benutzerdefinierter Filter übergeben wird — betrifft den produktiven Pfad für metadatengefilterte Vektorsuche bei Collections < 1000 Dokumenten |
| NEU-02 | 🟡 MED | Concurrency / Datenqualität | SQ8-Quantizer-Bound-Expansion in `do_insert()` nutzt `try_write()` (best-effort) statt eines garantierten Locks — unter Lastkonkurrenz werden Vektor-Grenzen nicht deterministisch erweitert, was zu stiller, lastabhängiger Präzisionsverschlechterung führt |

Aufgrund des Umfangs der Codebasis (≈80.000 Zeilen Rust über 15 Crates) wurde die Tiefenprüfung
auf die laut Komplexitätsindex kritischsten und am wenigsten von den vorherigen Audits
abgedeckten Bereiche fokussiert (HNSW-Suchpfad, SQ8-Quantisierung, LSM-Compaction,
Checkpoint-Store, MCP-Sandbox-Kryptographie). Eine erschöpfende Zeile-für-Zeile-Prüfung aller
15 Crates konnte im gegebenen Rahmen nicht geleistet werden; die unten dokumentierten Funde
sind jedoch vollständig verifiziert (Code gelesen, Aufrufpfad nachvollzogen, Reproduktionsszenario
konstruiert).

---

## 🔴 KRITISCH

---

### NEU-01 — `HnswIndexCore::search_filtered()`: Tombstone-Filter (`deleted_nodes`) wird bei aktivem Custom-Filter komplett übersprungen

**Datei:** `crates/memfuse-index/src/hnsw.rs`, Funktion `search_filtered()` (Zeile ~1855)
**Kategorie:** Logikfehler / Stille Fehlfunktion / Soft-Delete-Bruch
**Betroffener Produktionspfad:** `crates/memfuse-db/src/collection/search.rs::search_filtered_at()` → `HnswIndex::search_filtered(..., Some(filter))` für **jede** metadatengefilterte Vektorsuche (`WHERE`-Klausel) auf Collections mit weniger als 1000 Dokumenten (Pre-Filter-Strategie, Zeile 101 ff.), sowie für jeden zukünftigen Aufrufer, der eine eigene Filterfunktion übergibt.

**Beschreibung:**

In der finalen Ergebnis-Schleife von `search_filtered()` wird die Prüfung, ob ein Kandidat
softgelöscht (`deleted_nodes`-Roaring-Bitmap) ist, nur im **`else`-Zweig** durchgeführt:

```rust
let nodes = self.inner.nodes.read();
let deleted = self.inner.deleted_nodes.read();
let mut results = Vec::with_capacity(k);

for c in candidates.iter() {
    let node = nodes.get(c.index).ok_or_else(|| { ... })?;
    if node.committed_tx == 0 {
        continue;
    }
    let doc_id = node.doc_id;

    if let Some(f) = filter {
        if !f(doc_id) {
            continue;
        }
        // ← HIER FEHLT DIE PRÜFUNG: deleted.contains(c.index as u64) wird NIE geprüft!
    } else if deleted.contains(c.index as u64) {
        continue;
    }
    ...
    results.push(ScoredDocument::new(doc_id, score));
}
```

Die Bedingung ist als `if/else if` statt als zwei unabhängige, kumulative Prüfungen implementiert.
Das bedeutet: **sobald überhaupt ein Filter übergeben wird**, wird die Tombstone-Prüfung
(`deleted.contains(...)`) vollständig übersprungen — unabhängig davon, ob die übergebene
Filterfunktion selbst irgendetwas mit Löschstatus zu tun hat.

**Reproduzierbarer, produktiver Aufrufpfad:**

`Collection::search_filtered_at()` in `crates/memfuse-db/src/collection/search.rs`:

```rust
if total_docs < 1000 {
    let matched_ids = self.get_matching_doc_ids_at(&filter, seq).await?;
    ...
    let filter_fn = move |id: DocId| matched_ids.contains(&id);
    let scored_docs = self
        .index
        .search_filtered(query, k, Some(&filter_fn))   // ← filter = Some(...)
        .await?;
```

`filter_fn` prüft ausschließlich, ob eine `DocId` in der Menge `matched_ids` enthalten ist
(ermittelt via Metadaten-Scan gegen den LSM-Storage zum Snapshot `seq`). Sie hat **keine
Kenntnis** von der HNSW-internen `deleted_nodes`-Bitmap — diese ist ein getrenntes
Buchführungssystem, das ausschließlich intern im Index gepflegt wird (z. B. via
`HnswIndex::delete()` → `do_delete()`, oder via `rollback_to_tx()`, welches Knoten direkt
und unabhängig vom Storage-Layer in `deleted_nodes` einträgt).

**Konkretes Fehlerszenario:**

1. `HnswIndex::rollback_to_tx(target)` markiert alle Knoten mit `committed_tx > target` als
   `deleted` (physischer Soft-Delete, siehe Zeilen 2104–2120), **ohne** eine entsprechende
   Löschung im LSM-Storage vorauszusetzen oder zu erzwingen. Storage- und Index-Rollback sind
   zwei unabhängige Subsysteme ohne 2-Phase-Commit-Kopplung; ein teilweiser Fehler (Absturz,
   Netzwerkfehler bei verteilten Setups, Reihenfolge-Races zwischen den beiden
   `rollback_to_tx`-Aufrufen in `Collection`) kann dazu führen, dass ein Dokument im
   LSM-Storage (und damit in `get_matching_doc_ids_at`) noch sichtbar ist, im HNSW-Index aber
   bereits als `deleted` markiert wurde — oder umgekehrt bei anderen Reihenfolgen.
2. Sobald `matched_ids` diese DocId (fälschlich oder aus einem anderen Grund) enthält, liefert
   `search_filtered()` sie **trotz aktivem Tombstone** als valides Suchergebnis zurück, weil
   Zeile ~1855 die `deleted`-Prüfung komplett auslässt.
3. Noch unabhängiger vom Rollback-Fall: Jeder zukünftige oder externe Aufrufer, der
   `search_filtered()` mit einer eigenen Filterfunktion nutzt (z. B. Tenant-Isolation,
   ACL-Filter, Community-Filter wie im MAJOR-01-Fund des ersten Audits), verliert **grundsätzlich
   und für immer** den eingebauten Soft-Delete-Schutz — ein architektonischer Fehler, keine
   Randbedingung.

**Auswirkung:**
- Bruch der Soft-Delete-Garantie im wichtigsten produktiven Suchpfad (metadatengefilterte
  Vektorsuche) unter genau den Bedingungen (Rollback, Recovery, Multi-Subsystem-Inkonsistenz),
  unter denen Datenintegrität am kritischsten ist.
- Gelöschte / zurückgerollte Vektoren mit potenziell inkonsistenten Graph-Verbindungen
  (Nachbarn zeigen ggf. auf andere zwischenzeitlich gelöschte oder überschriebene Indizes)
  können erneut in Suchergebnisse gelangen.
- Da `nodes`-Indizes nach `do_insert()` **nie wiederverwendet** werden (append-only, siehe
  Zeile 1093–1104), ist keine Datenvermischung zwischen unterschiedlichen Dokumenten zu
  befürchten — das Risiko beschränkt sich auf das erneute Auftauchen von Dokumenten, die als
  gelöscht/zurückgerollt gelten sollten.

**Fix:**

```rust
if deleted.contains(c.index as u64) {
    continue;
}
if let Some(f) = filter {
    if !f(doc_id) {
        continue;
    }
}
```

Die Tombstone-Prüfung muss **unconditional** vor (oder kumulativ mit) der Custom-Filter-Prüfung
erfolgen, nicht als exklusive Alternative dazu. Ein Regressionstest sollte `rollback_to_tx()`
gefolgt von einer metadatengefilterten Suche über denselben Snapshot abdecken.

---

## 🟡 MITTEL

---

### NEU-02 — SQ8-Quantizer-Bound-Expansion in `do_insert()` ist "best-effort" (`try_write`) statt garantiert — lastabhängige, stille Präzisionsverschlechterung

**Datei:** `crates/memfuse-index/src/hnsw.rs`, Funktion `do_insert()` (Zeile ~1065–1077)
**Kategorie:** Concurrency / Datenqualität / Nicht-Determinismus

**Beschreibung:**

Wenn Scalar-Quantisierung (SQ8) aktiv ist und ein neuer Vektor eingefügt wird, versucht
`do_insert()` opportunistisch, die Quantizer-Grenzen (`mins`/`maxes`) zu erweitern, falls der
neue Vektor außerhalb des bisher trainierten Bereichs liegt:

```rust
let vector_data = if self.config.quantize {
    if let Some(mut q_guard) = self.quantizer.try_write() {
        if let Some(q) = q_guard.as_mut() {
            q.expand_bounds_to_fit(vector);      // Grenzen erweitern (mutable Zugriff)
            VectorData::U8(q.quantize(vector))
        } else {
            VectorData::F32(vector.to_vec())
        }
    } else if let Some(q) = self.quantizer.read().as_ref() {
        // ← try_write() ist fehlgeschlagen (Lock kontenzioniert) → KEINE Bound-Expansion!
        VectorData::U8(q.quantize(vector))
    } else {
        VectorData::F32(vector.to_vec())
    }
} else {
    VectorData::F32(vector.to_vec())
};
```

`self.quantizer` ist ein `parking_lot::RwLock`. Läuft parallel eine Suche (`search()` /
`search_filtered()`), die den Quantizer im Lesemodus hält (z. B. für `query_quantized` oder
die asymmetrische Reranking-Distanz), oder ein anderer konkurrierender `try_write()`-Aufruf,
schlägt `try_write()` fehl. In diesem Fall wird auf den **Read-Pfad** ausgewichen, der den
Vektor mit den *unveränderten, alten* Grenzen quantisiert — `expand_bounds_to_fit()` wird
für diesen Vektor **nie** aufgerufen.

`ScalarQuantizer::quantize()` behandelt außerhalb der Grenzen liegende Werte per `clamp()`
(kein Absturz, kein UB), aber jede Dimension des Vektors, die außerhalb `[min, max]` liegt,
wird auf den jeweiligen Rand geklemmt — der eingefügte Vektor verliert dauerhaft und
irreversibel Information für die betroffenen Dimensionen, was direkt die Distanzberechnung
und damit die Retrieval-Qualität für dieses Dokument beeinträchtigt (u. U. dauerhaft, da SQ8
die Quantisierung beim Insert fixiert und der Klartext-Vektor danach nicht mehr vorgehalten
wird).

**Kernproblem:** Ob eine Grenzerweiterung stattfindet, hängt vom **Zeitpunkt und der
Lastsituation** ab (Lock-Kontention durch parallele Suchanfragen), nicht vom tatsächlichen
Bedarf. Zwei identische Inserts mit demselben Out-of-Range-Vektor können — abhängig vom
Scheduling — zu unterschiedlichen Quantisierungsergebnissen führen. Das System hat zwar eine
Diagnose (`out_of_range_queries`-Zähler mit Warn-Log ab >5 % Rate in `quantize()`), aber
**keinen Korrekturmechanismus** — die Warnung dokumentiert das Symptom, aber der `try_write`
-Pfad ist genau die Ursache dafür, dass die Rate unter Last strukturell ansteigt, weil
gerade unter hoher Suchlast (viele Reader) die Bound-Expansion am häufigsten ausfällt — also
exakt dann, wenn Korrektheit am wichtigsten wäre.

**Fix (zwei Optionen):**

1. **Garantierte Aktualisierung:** `try_write()` durch ein reguläres `write()`
   (blockierend, aber `parking_lot`-Locks sind kurzlebig und nicht async-gehalten, daher
   unkritisch für Deadlocks) ersetzen, sodass jede Bound-Expansion deterministisch erfolgt:
   ```rust
   let vector_data = if self.config.quantize {
       let mut q_guard = self.quantizer.write();
       if let Some(q) = q_guard.as_mut() {
           q.expand_bounds_to_fit(vector);
           VectorData::U8(q.quantize(vector))
       } else {
           VectorData::F32(vector.to_vec())
       }
   } else {
       VectorData::F32(vector.to_vec())
   };
   ```
2. **Falls `try_write` bewusst zur Vermeidung von Writer-Blocking bei Lesern beibehalten
   werden soll:** Out-of-Range-Vektoren, für die die Bound-Expansion fehlgeschlagen ist, in
   eine Nacharbeits-Queue stellen und bei der nächsten geplanten Rekalibrierung
   (`quantizer_recalibration_sample_size`) zwangsweise erneut verarbeiten, statt sie
   stillschweigend mit veralteten Grenzen zu quantisieren.

Ein Regressionstest sollte konkurrierende `search()`- und `insert()`-Aufrufe mit einem
absichtlich außerhalb der Trainingsgrenzen liegenden Vektor simulieren und verifizieren, dass
`expand_bounds_to_fit` in jedem Fall (nicht nur im unkontentierten Fall) angewendet wird.

---

## Hinweis zur Prüfmethodik und Abdeckung

Die vorliegende Ergänzung fokussierte bewusst auf Codepfade, die in den beiden vorherigen
Audit-Dokumenten nicht im Detail nachvollzogen wurden — insbesondere den finalen
Reranking-/Filter-Zusammenführungspfad in `memfuse-index::hnsw` sowie die
Locking-Semantik der SQ8-Quantisierung. Weitere Bereiche mit erhöhtem Risiko, die im
gegebenen Zeitrahmen nur oberflächlich (ohne konkreten neuen Fund) geprüft wurden und für
eine Folgeprüfung empfohlen werden:

- `memfuse-store::compaction.rs` — Tombstone-GC- und Dedup-Logik im Merge-Iterator (`HeapItem`
  implementiert `Ord`/`PartialEq` inkonsistent zueinander; aktuell ohne Funktionsbruch, da
  `BinaryHeap` nur `Ord` nutzt, aber ein Clippy-`derive_ord_xor_partial_ord`-Kandidat und
  Risiko bei zukünftigem Code, der `PartialEq`/`Eq` zur Deduplizierung heranzieht).
- `memfuse-checkpoint::PersistentCheckpointStore` — Pin/Unpin-Reihenfolge bei
  `create_checkpoint()` wurde geprüft und als korrekt befunden (Pin-before-save,
  Unpin-after-save-success), sollte aber bei künftigen Änderungen an der
  `SnapshotRegistry`-Semantik erneut verifiziert werden.
- `memfuse-embed` (ONNX-Pfad) und `memfuse-py` (PyO3-Bridge) wurden aus Zeitgründen nur
  oberflächlich gesichtet; beide sind laut Komplexitätsindex "Moderat" eingestuft und sollten
  bei einer vollständigen Nachprüfung mit vergleichbarer Tiefe wie oben behandelt werden.

---

## Priorisierte Maßnahmen

| Priorität | ID | Aktion | Aufwand |
|---|---|---|---|
| P0 | NEU-01 | `deleted.contains()`-Prüfung in `search_filtered()` von `else if` auf unconditional umstellen; Regressionstest mit `rollback_to_tx` + gefilterter Suche | 1h |
| P1 | NEU-02 | `try_write()` → `write()` für Quantizer-Bound-Expansion in `do_insert()`, oder Nacharbeits-Queue für fehlgeschlagene Expansionen | 2h |

---

*Dieser Bericht ergänzt, ersetzt aber nicht die vorherigen Audit-Dokumente. Alle Befunde
wurden gegen den tatsächlichen Quellcode im frisch geklonten Repository verifiziert
(Datei- und Zeilenangaben beziehen sich auf den Stand des Klons zum Analysezeitpunkt).*
