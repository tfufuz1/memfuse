# SDD Specification: `memfuse-text`

**Status:** DRAFT  
**Crate-Layer:** 1 (Engine)  
**Souveränität:** BM25, Morphological, LSM-backed.

---

## 1. Systemgrenzen & Verantwortlichkeit (MECE)

`memfuse-text` (Signal 2) implementiert die terminologische Relevanzprüfung via Volltextsuche.

### Verantwortlichkeiten:
- **Scoring:** Implementierung der BM25-Formel mit Floor-IDF zur Vermeidung von negativen Gewichten bei häufigen Termen.
- **Invertierter Index:** LSM-gestützte Posting-Listen im Format `pl:{term}:{doc_id}`.
- **Update-Strategie:** "Tombstone-Path" zur Reduktion von Schreiblast; markiert Dokumente als `tbs:{doc_id}` für lazy Compaction.
- **Sprachintelligenz:** Morphologische Tokenisierung (Compound splitting) für Deutsch zur Erhöhung des Recalls.

### Nicht-Verantwortlichkeiten:
- **Vektor-Semantik:** Delegiert an `memfuse-index`.
- **Persistente Speicherung:** Nutzt `memfuse-core::StorageEngine` (LSM).

---

## 2. Kritische Invarianten & SDD-Garantien

| ID | Invariante | Beschreibung |
|---|---|---|
| **TEXT-INV-001** | **Tombstone-Consistency** | Ein Update schreibt nur den neuen Forward-Index + Tombstone. Alte Terme werden erst durch `resolve_tombstones()` gelöscht. |
| **TEXT-INV-002** | **Numerical Safety** | `score_term` fängt NaN/Inf ab, die durch Division durch Null oder Logarithmen von Werten <= 0 entstehen könnten. |
| **TEXT-INV-003** | **Morph-Identity** | Der Tokenizer fügt sowohl die Stammform als auch den ursprünglichen Compound-Token hinzu. |

---

## 3. Schnittstellen-Spezifikation (High-Precision)

### 3.1 TextIndex Trait (`inverted.rs`)
- **`upsert_document(tx, id, text)`**: Tokenisiert, berechnet TFs und speichert Posting-Listen.
- **`resolve_tombstones(tx)`**: Iteriert `tbs:` Keys und bereinigt verwaiste Terme im Storage.

### 3.2 BM25 Scorer (`bm25.rs`)
- Nutzt feste Parameter `k1 = 1.2` und `b = 0.75`.
- Berechnet globale Statistiken (Average Doc Length) aus `meta:stats`.

---

## 4. Codebase-Checklist (src/)

| Modul | Status | Bezug auf Spec |
|---|---|---|
| `lib.rs` | ✅ | Zentraler Bm25Scorer & Trait-Impl. |
| `bm25.rs` | ✅ | Mathematische Scoring-Formel. |
| `inverted.rs`| ✅ | LSM-Posting-List Logik (CORE-INV-001). |
| `morphology.rs`| ✅ | German Compound Splitting (WP-6.5). |
| `tokenizer.rs` | ✅ | Unicode Segmentation & Stopwords. |

---

## 5. Verifikation (Triple-Gate)

- **I - Kompilierbarkeit:** `cargo check -p memfuse-text`
- **II - Stil:** `cargo clippy -p memfuse-text`
- **III - Verhalten:** 
  - `test_bm25_ranks_exact_keyword_higher`: Nachweis der Ranking-Güte.
  - `test_german_expansion_ratio`: Messung der Token-Recall Steigerung (> 20%).
  - `test_forward_index_consistency`: Validierung des Tombstone-Life-Cycles.
