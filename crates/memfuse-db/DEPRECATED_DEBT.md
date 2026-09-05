# Deprecated Code Debt in `memfuse-db`

Dokumentation aller verbliebenen `#[allow(deprecated)]`-Stellen im Crate `memfuse-db`, nachdem die crate-weite Unterdrückung `#![allow(deprecated)]` aus `lib.rs` entfernt wurde.

---

### 1. `crates/memfuse-db/src/filter.rs`

- **Zeile 58, 70, 86:** `FilterOp`, `MetadataFilter` und `impl TryFrom<MetadataFilter> for FilterExpr`
- **Zeilen 159, 214, 251, 269:** Unit-Tests für `MetadataFilter`
- **Betroffene Deprecated-Typen:** `filter::MetadataFilter`, `filter::FilterOp`
- **Grund gegen sofortige Vollmigration:**
  `MetadataFilter` und `FilterOp` bilden die alte Filter-DSL von `memfuse-db`, die durch die kanonische `memfuse_core::FilterExpr` abgelöst wurde. Die Typen und Konvertierungs-Implementierungen müssen für Abwärtskompatibilität der öffentlichen API erhalten bleiben, da externe Aufrufer `MetadataFilter` importieren und nutzen können. Die Unit-Tests stellen sicher, dass die verlustfreie Umwandlung via `TryFrom` fehlerfrei funktioniert.

---

### 2. `crates/memfuse-db/src/lib.rs`

- **Zeile 108:** `pub use filter::MetadataFilter;`
- **Betroffener Deprecated-Typ:** `filter::MetadataFilter`
- **Grund gegen sofortige Vollmigration:**
  Öffentlicher Re-Export von `MetadataFilter` am Crate-Root für Abwärtskompatibilität mit bestehendem Client-Code.

- **Zeile 815:** `pub async fn search_with_filter(...)`
- **Betroffene Deprecated-Funktion:** `MemFuse::search_with_filter`
- **Grund gegen sofortige Vollmigration:**
  Bestehende Fassaden-Methode auf `MemFuse`, die `MetadataFilter` entgegennimmt. Ist als `#[deprecated]` markiert und leitet intern an `query().metadata_filter()` weiter. Muss für API-Kompatibilität erhalten bleiben.

- **Zeile 888, 896:** `pub async fn search_filtered(...)`
- **Betroffene Deprecated-Funktion:** `Collection::search_filtered`
- **Grund gegen sofortige Vollmigration:**
  Fassaden-Methode auf `MemFuse`, die die deprecated `Collection::search_filtered`-Methode aufruft.

---

### 3. `crates/memfuse-db/src/collection/search.rs`

- **Zeile 13:** `use crate::filter::MetadataFilter;`
- **Zeilen 22, 40, 57, 114, 237, 422, 438, 502, 518, 665:** Direct Search Methods on `Collection`:
  - `Collection::search()`
  - `Collection::search_with_filter()`
  - `Collection::search_with_filter_expr()`
  - `Collection::search_text()`
  - `Collection::search_filtered()`
  - `Collection::search_filtered_at()`
  - `Collection::hybrid_search()`
  - `Collection::hybrid_search_reranked()`
  - `Collection::hybrid_search_with_weights()`
  - `Collection::hybrid_search_with_strategy()`
  - `Collection::hybrid_search_with_query()`
- **Betroffene Deprecated-Funktionen:** Sämtliche direkte `search_*`- und `hybrid_search_*`-Methoden der `Collection`.
- **Grund gegen sofortige Vollmigration:**
  Diese Methoden wurden zugunsten der Fluent Builder API (`Collection::query()`) als `#[deprecated]` markiert. Da `Collection` die zentrale öffentliche Datenstruktur des Crates ist, können diese Methoden nicht sofort entfernt werden, ohne den öffentlichen API-Vertrag und bestehende Integrationen zu brechen.

---

### 4. `crates/memfuse-db/src/collection/query_builder.rs`

- **Zeile 8, 184:** `use crate::filter::MetadataFilter;` & `pub fn metadata_filter(mut self, filter: MetadataFilter) -> Self`
- **Zeile 323:** Interne Delegation an `self.collection.hybrid_search_with_query(&hybrid_query)`
- **Zeilen 450, 480, 519, 580:** Äquivalenz-Tests zwischen legacy `Collection::search_*`-Methoden und `HybridQueryBuilder`.
- **Betroffene Deprecated-Funktionen:** `MetadataFilter`, `Collection::hybrid_search_with_query` sowie direkte `Collection::search_*`-Aufrufe in Äquivalenz-Tests.
- **Grund gegen sofortige Vollmigration:**
  Der `HybridQueryBuilder` bietet `metadata_filter()` als Abwärtskompatibilitäts-Brücke für bestehenden Code mit `MetadataFilter`. Zudem muss `execute()` die interne Retrieval-Logik in `Collection` aufrufen. Die Tests verifizieren explizit, dass der neue Builder dieselben Ergebnisse wie die legacy Suchmethoden liefert.

---

### 5. `crates/memfuse-db/src/collection/tx.rs`

- **Zeile 7:** `pub fn next_tx(&self) -> Result<TxId>`
- **Betroffene Deprecated-Funktion:** `Collection::next_tx`
- **Grund gegen sofortige Vollmigration:**
  `next_tx()` wurde zugunsten der expliziten und kanonischen `allocate_tx()` als deprecated markiert. Die Methode wird für Abwärtskompatibilität öffentlicher Aufrufer beibehalten und delegiert intern direkt an `allocate_tx()`.

---

### 6. `crates/memfuse-db/src/collection/mod.rs`

- **Zeile 19:** `#[allow(deprecated)] mod tests;`
- **Grund gegen sofortige Vollmigration:**
  Einige Tests im Modul `tests` prüfen gezielt deprecated APIs (z.B. legacy Suchen) auf Korrektheit und Äquivalenz, um Regressionen in bestehenden Verträgen auszuschließen.
