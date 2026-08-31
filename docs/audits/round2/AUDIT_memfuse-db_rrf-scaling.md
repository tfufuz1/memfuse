# Audit Report: RRF Scaling & Memory Complexity Analysis (Round 2)

**Crate:** `memfuse-db`
**Modul:** `crates/memfuse-db/src/fusion.rs`
**Datum:** 2026-08-31
**Auditor:** Senior Rust Performance-Ingenieur (Jules Agent)
**Status:** Audit Abgeschlossen / Hypothese Bestätigt

---

## 1. Executive Summary

In Runde 1 wurde die RRF-Latenz (Reciprocal Rank Fusion) bei kleinen synthetischen Datensätzen mit ~12,78 µs gemessen.
In Runde 2 wurde das Skalierungsverhalten bei **großen Einzelsignal-Trefferzahlen** (1.000 bis 500.000 Treffer pro Signal, verteilt auf 4 Suchsignale = bis zu 2.000.000 Gesamttreffer vor Fusion) systematisch evaluiert.

Die Gemini-Hypothese bezüglich naiver Voll-Materialisierung und Sortierung konnte **experimentell und im Quellcode exakt bestätigt werden**:
1. **Speicherkomplexität:** Der Speicherverbrauch (Peak-RSS) wächst **streng linear $O(U)$** bezüglich der Anzahl eindeutiger Dokumente $U$. Bei 500.000 Treffern pro Signal (2 Mio. Gesamttreffer, ~800.000 eindeutige Dokumente) klettert der Peak-RSS von **12,50 MB auf 3.858,15 MB (~3,86 GB)**.
2. **Latenzkomplexität:** Die End-to-End-Fusionslatenz skaliert **superlinear $O(U \log U)$** durch den vollständigen Sortierschritt `Vec::sort_by` über alle aggregierten Dokumente. Sie steigt von **2,29 ms (1K Hits/Signal)** auf **4.785,80 ms (~4,79 s bei 500K Hits/Signal)**.
3. **Sub-Engine-Begrenzung vs. RRF-Engine:** Während die High-Level Collection-Suche (`Collection::hybrid_search_with_query`) den Parameter `k` über `MAX_SEARCH_K = 1.000` auf Sub-Engine-Ebene deckelt, besitzt die zentrale Fusionsfunktion `weighted_reciprocal_rank_fusion()` **keine Beschränkung** für die ihr übergebenen Signal-Vektoren. Wenn Sub-Engines oder externe Aufrufer unbegrenzte Treffermengen übergeben, explodieren Latenz und Speicherverbrauch.

---

## 2. Code-Pfad-Analyse (Volle vs. Top-K-Materialisierung)

Analyse der zentralen Fusionsfunktion in `crates/memfuse-db/src/fusion.rs`:

```rust
pub fn weighted_reciprocal_rank_fusion(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
) -> Vec<SearchResult> {
    let k = 60;
    // 1. VOLLSTÄNDIGE MATERIALISIERUNG ALLER SIGNAL-TREFFER
    // Map: id -> (score, metadata, matched_signals)
    let mut fused: HashMap<String, (f32, Option<serde_json::Value>, Vec<String>)> = HashMap::new();

    for (signal_name, result_set, weight) in result_sets {
        if weight <= 0.0 {
            continue;
        }
        for (rank, doc) in result_set.into_iter().enumerate() {
            let score = weight / ((k + rank + 1) as f32);
            let entry = fused.entry(doc.id).or_insert((0.0, None, Vec::new()));
            entry.0 += score;
            merge_metadata(&mut entry.1, doc.metadata);
            if !signal_name.is_empty()
                && signal_name != "unnamed"
                && !entry.2.contains(&signal_name)
            {
                entry.2.push(signal_name.clone());
            }
        }
    }

    // 2. VOLLSTÄNDIGE KONVERTIERUNG IN EINEN VEC DEKORIERTER ERGEBNISSE
    let mut ranked: Vec<SearchResult> = fused
        .into_iter()
        .map(|(id, (score, metadata, matched_signals))| SearchResult {
            id,
            score,
            metadata,
            matched_signals,
        })
        .collect();

    // 3. KRITISCHE ENGSTELLE (Zeilen 81-88): VOLLSTÄNDIGE SORTIERUNG O(U log U)
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });

    // 4. TRUNCIERUNG ERST AM ENDE
    ranked.truncate(max_results);
    ranked
}
```

### Befunde:
* **Keine Top-K-Vorfilterung während der Aggregation:** Alle übergebenen `SearchResult`-Objekte inklusive ihrer vollständigen `serde_json::Value`-Metadaten werden in der `HashMap` materialisiert.
* **Kein Bounded-Heap für Top-K:** Nach der Aggregation wird ein `Vec` der Länge $U$ (Anzahl eindeutiger Dokumente) erzeugt und mittels `sort_by` vollständig sortiert.
* **Sub-Engine Verhalten:** Auf Collection-Ebene (`crates/memfuse-db/src/collection/search.rs`) begrenzen `search_filtered_at` (Vector) und `search_at` (BM25) ihre Treffermengen auf $k \le \text{MAX\_SEARCH\_K} (1.000)$. Graph-Traversierung und externe API-Nutzungen der `weighted_reciprocal_rank_fusion` sind jedoch unbegrenzt.

---

## 3. Skalierungs-Benchmark-Tabelle

Die Messung erfolgte auf der Sandbox-Umgebung mit dem dedizierten Benchmark `benches/rrf_scale_bench.rs` unter Nutzung von 4 getrennten Suchsignalen (`vector`, `text`, `graph`, `hybrid`) mit Überlappungsfaktor 5.

| Datenpunkt | Treffer / Signal ($N$) | Gesamt-Input-Hits | Eindeutige Doks ($U$) | End-to-End Latenz (Criterion Mean) | Peak-RSS Speicher | Latenz / Input-Hit |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | 1.000 | 4.000 | ~1.600 | **2,29 ms** (2.287,2 µs) | **12,50 MB** | 0,57 µs |
| **2** | 10.000 | 40.000 | ~16.000 | **63,36 ms** (63.359 µs) | **91,61 MB** | 1,58 µs |
| **3** | 100.000 | 400.000 | ~160.000 | **923,30 ms** (923.300 µs) | **771,98 MB** | 2,31 µs |
| **4** | 500.000 | 2.000.000 | ~800.000 | **4.785,80 ms** (4.785.800 µs) | **3.858,15 MB** | 2,39 µs |

---

## 4. Wachstumsklassen-Einordnung

### A. Speicherverbrauch (Peak-RSS)
* **Wachstumsklasse:** **Strict Linear $O(U)$**
* **Kurven-Fit / Formel:** $\text{RSS}(MB) \approx 0,0048 \times U + 12,5 \text{ MB}$
* **Begründung:** Jedes eindeutige Dokument speichert Dokument-ID, JSON-Metadaten-Value und Signal-Namen im Heap. Bei 800.000 Einzelelementen fordert die `HashMap` + `Vec` Allocator-Speicher in Höhe von ~3,86 GB an.

### B. Latenz (End-to-End Execution Time)
* **Wachstumsklasse:** **Superlinear $O(U \log U)$**
* **Kurven-Fit:** Der Übergang von $N=1.000$ (2,29 ms) zu $N=500.000$ (4.785,80 ms) entspricht einem **Skalierungsfaktor von ~2.092x** bei 500-facher Datenmenge.
* **Ursache:**
  1. `HashMap`-Insertion & Re-Hashing overhead: $O(N)$
  2. `Vec::sort_by` (Quicksort/Pdqsort): $O(U \log U)$
  3. JSON Metadata Merging: $O(N \times |\text{keys}|)$

---

## 5. Konkreter Verbesserungsvorschlag (Bounded Top-K Min-Heap)

### A. Identifizierte Problemzeilen
`crates/memfuse-db/src/fusion.rs:81-88`

### B. Ziel-Architektur & Komplexität
Statt `Vec::sort_by` über alle $U$ Elemente auszuführen, sollte eine Top-K Auswahl mittels eines **Bounded Min-Heaps** (`std::collections::BinaryHeap`) der festen maximalen Kapazität $K = \text{max\_results}$ verwendet werden.

* **Neue Speicherkomplexität:** $O(U)$ für Hashmap (oder $O(K)$ falls Streaming/Early-Pruning angewendet wird), aber Ranking-Speicher nur $O(K)$.
* **Neue Sortierkomplexität:** $O(U \log K)$ anstelle von $O(U \log U)$. Da $K \le 1.000 \ll U$, reduziert sich der Sortieraufwand bei 800.000 Dokumenten um den Faktor $\frac{\log_2(800.000)}{\log_2(1.000)} \approx \frac{19.6}{9.96} \approx 2\text{x}$ rein rechnerisch, vermeidet jedoch die Allokation eines gigantischen Sortier-Vektors.

### C. Konkreter Code-Entwurf

```rust
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(PartialEq)]
struct HeapEntry {
    result: SearchResult,
}

impl Eq for HeapEntry {}

// Min-Heap Order: Der kleinstmögliche Score liegt oben an der Spitze (Peek/Pop)
impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Umgekehrter Vergleich für Min-Heap!
        other.result.score
            .partial_cmp(&self.result.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.result.id.cmp(&other.result.id))
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn weighted_reciprocal_rank_fusion_optimized(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
) -> Vec<SearchResult> {
    if max_results == 0 {
        return Vec::new();
    }

    let k = 60;
    let mut fused: HashMap<String, (f32, Option<serde_json::Value>, Vec<String>)> = HashMap::new();

    for (signal_name, result_set, weight) in result_sets {
        if weight <= 0.0 {
            continue;
        }
        for (rank, doc) in result_set.into_iter().enumerate() {
            let score = weight / ((k + rank + 1) as f32);
            let entry = fused.entry(doc.id).or_insert((0.0, None, Vec::new()));
            entry.0 += score;
            merge_metadata(&mut entry.1, doc.metadata);
            if !signal_name.is_empty()
                && signal_name != "unnamed"
                && !entry.2.contains(&signal_name)
            {
                entry.2.push(signal_name.clone());
            }
        }
    }

    // TOP-K SELEKTION MIT BOUNDED MIN-HEAP O(U log K)
    let mut heap = BinaryHeap::with_capacity(max_results + 1);

    for (id, (score, metadata, matched_signals)) in fused {
        let res = SearchResult {
            id,
            score,
            metadata,
            matched_signals,
        };

        if heap.len() < max_results {
            heap.push(HeapEntry { result: res });
        } else if let Some(min_entry) = heap.peek() {
            // Falls das neue Element besser ist als das schlechteste im Heap: ersetzen
            let is_better = res.score > min_entry.result.score
                || ((res.score - min_entry.result.score).abs() < f32::EPSILON && res.id < min_entry.result.id);
            if is_better {
                heap.pop();
                heap.push(HeapEntry { result: res });
            }
        }
    }

    // In absteigender Reihenfolge extrahieren
    let mut final_results: Vec<SearchResult> = heap.into_sorted_vec().into_iter().map(|e| e.result).collect();
    // into_sorted_vec() sortiert von klein nach groß für Min-Heap, daher reverse:
    final_results.reverse();
    final_results
}
```

---

## 6. Anhang: Rohlogs / CSV-Auszug

### `benches/results/rrf_scale_rss.csv`
```csv
timestamp_secs,stage,hits_per_signal,total_hits,vm_rss_kb,vm_rss_mb,latency_micros
1788216127,rrf_fusion,1000,4000,12804,12.50,2715.11
1788216138,rrf_fusion,10000,40000,93808,91.61,71269.57
1788216156,rrf_fusion,100000,400000,790512,771.98,946532.52
1788216237,rrf_fusion,500000,2000000,3950744,3858.15,5413897.77
```

---
*Ende des Berichts.*
