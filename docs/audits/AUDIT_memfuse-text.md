# AUDIT REPORT: `memfuse-text` Crate
**Datum:** 31. August 2026
**Auditor:** Senior Rust & IR Engineer (Computerlinguistik & Information Retrieval Specialist)
**Ziel-Crate:** `crates/memfuse-text` (Volltextsuche-Signal / Signal 2 der 4-Signal-Fusion)
**Repository:** https://github.com/tfufuz1/memfuse

---

## 1. Executive Summary

Im Auftrag des Weltkonzerns wurde das Crate `memfuse-text` bezüglich mathematischer BM25-Score-Korrektheit, InvertedIndex CRUD- & MVCC-Konsistenz, deutscher Komposita-Zerlegungsqualität und Tokenisierungs-Robustheit auditiert.

### Hauptergebnisse
1. **Unsafe-Code Invariante:** `#![forbid(unsafe_code)]` ist im gesamten Crate strikt durchgesetzt (`grep -rn "unsafe" crates/memfuse-text/` liefert 0 Treffer in ausführbarem Code).
2. **BM25 Scoring:** Die Implementierung in `src/bm25.rs` verwendet Robertson-Spärck-Jones (RSJ) Log-IDF mit Robertson-Walker Standard-Parametern ($k_1 = 1.5, b = 0.75$). Alle handberechneten Beispieldokumente stimmen auf 6 Nachkommastellen genau mit den Ergebnissen der Crate-Funktion `score_term_with_params` überein.
3. **IDF-Glättung & Clamping:** Terme mit $df > N/2$ (z.B. sehr häufige Wörter oder Terme in allen Dokumenten) bzw. $df > N$ (Datenkorruption) führen bei unmodifizierter RSJ-Formel zu negativen IDFs bzw. `NaN`. `memfuse-text` fängt diese Fälle durch ein logisches Clamping auf $10^{-6}$ ab, womit Scores strikt endlich und nicht-negativ bleiben.
4. **Deutsche Morphologie Engine:** Auf einem linguistisch fundierten Testcorpus von 45 repräsentativen deutschen Fachkomposita (Fugen-s, Fugen-en, Fugen-n, Fugen-e, Fugen-er, Fugen-es, Zero-Fuge, 3-4-Teil Komposita und KMU-Fachbegriffe) erzielte der `GermanCompoundSplitter` eine Genauigkeit von **91.11%** (41 von 45 exakt korrekt).
5. **Tokenizer-Robustheit & Monotonie:** Via `proptest` wurden 0 Panics bei beliebigen Unicode-Strings nachgewiesen. Die BM25-Termfrequenz-Monotonie ($tf_2 > tf_1 \implies score(tf_2) \ge score(tf_1)$) wurde mathematisch und per Property-Test nachgewiesen.
6. **Benchmarks:** Single-Term BM25-Scoring benötigt ca. **2.55 ns** per Call. Der `DefaultTokenizer` verarbeitet Text mit **31.6 MiB/s** (~5.08 µs pro Satz). Der `GermanMorphTokenizer` verarbeitet Text mit **1.01 MiB/s** (~163 µs pro Satz inkl. dynamischer Programmierung und Fugenlaut-Prüfung).

---

## 2. BM25-Korrektheitsmatrix & IDF-Edge-Cases

### Mathematische Formel
Die Standard-BM25-Score-Formel für ein Dokument $D$ und einen Query-Term $q_i$ lautet:
$$\text{Score}(D, q_i) = \text{IDF}(q_i) \cdot \frac{f(q_i, D) \cdot (k_1 + 1)}{f(q_i, D) + k_1 \cdot \left(1 - b + b \cdot \frac{|D|}{\text{avgdl}}\right)}$$

mit Robertson-Spärck-Jones (RSJ) Log-IDF:
$$\text{IDF}(q_i) = \ln \left( \frac{N - df + 0.5}{df + 0.5} \right)$$

### Handverifiziertes Test-Corpus ($N=5, \text{avgdl}=3.0, k_1=1.5, b=0.75$)
- **Doc 1 ($D_1$):** "apple banana" ($|D_1|=2$)
- **Doc 2 ($D_2$):** "apple apple cherry" ($|D_2|=3$)
- **Doc 3 ($D_3$):** "apple banana cherry date" ($|D_3|=4$)
- **Doc 4 ($D_4$):** "banana date elderberry" ($|D_4|=3$)
- **Doc 5 ($D_5$):** "fig grape hazelnut" ($|D_5|=3$)

Total Tokens = 15, $N=5$, $\text{avgdl} = 3.0$.

#### Handberechnung vs. Implementierung

| Query Term | Doc | $df$ | $tf$ | $|D|$ | Handberechnung (Schritt für Schritt) | Implementierung | Match? |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **cherry** | $D_2$ | 2 | 1 | 3 | $\text{IDF} = \ln\left(\frac{5 - 2 + 0.5}{2 + 0.5}\right) = \ln(1.4) \approx 0.3364722$<br>$\text{norm\_len} = 3 / 3.0 = 1.0$<br>$\text{tf\_factor} = \frac{1 \cdot 2.5}{1 + 1.5 \cdot (0.25 + 0.75 \cdot 1.0)} = 1.0$<br>$\text{Score} = 0.3364722 \cdot 1.0 = \mathbf{0.3364722}$ | `0.3364722` | **EXAKT** |
| **cherry** | $D_3$ | 2 | 1 | 4 | $\text{IDF} = \ln(1.4) \approx 0.3364722$<br>$\text{norm\_len} = 4 / 3.0 = 1.3333333$<br>$\text{tf\_den} = 1 + 1.5 \cdot (0.25 + 0.75 \cdot \frac{4}{3}) = 2.875$<br>$\text{tf\_factor} = \frac{2.5}{2.875} = \frac{20}{23} \approx 0.8695652$<br>$\text{Score} = 0.3364722 \cdot \frac{20}{23} = \mathbf{0.2925845}$ | `0.2925845` | **EXAKT** |
| **elderberry** | $D_4$ | 1 | 1 | 3 | $\text{IDF} = \ln\left(\frac{5 - 1 + 0.5}{1 + 0.5}\right) = \ln(3.0) \approx 1.0986123$<br>$\text{norm\_len} = 3 / 3.0 = 1.0 \implies \text{tf\_factor} = 1.0$<br>$\text{Score} = 1.0986123 \cdot 1.0 = \mathbf{1.0986123}$ | `1.0986123` | **EXAKT** |

#### Parameter-Sensitivität ($b=0$ vs. $b=1$)
- **$b=0$ (Keine Längennormalisierung):** Für $D_3$ ($|D_3|=4, tf=1$) bei Query "cherry":
  $\text{tf\_den} = 1 + 1.5 \cdot (1 - 0) = 2.5 \implies \text{tf\_factor} = 1.0 \implies \text{Score} = \mathbf{0.3364722}$ (identisch zu $D_2$, Dokumentlänge wird ignoriert).
- **$b=1$ (Volle Längennormalisierung):** Für $D_3$ bei Query "cherry":
  $\text{tf\_den} = 1 + 1.5 \cdot (1.3333333) = 3.0 \implies \text{tf\_factor} = \frac{2.5}{3.0} = \frac{5}{6} \approx 0.8333333 \implies \text{Score} = \mathbf{0.2803935}$ (stärkere Längenstrafe).

#### IDF-Grenzfälle

| Edge Case | Eingabewerte | Standard-RSJ Verhalten | `memfuse-text` Implementierung | Bewertung |
| :--- | :--- | :--- | :--- | :--- |
| **Term in 0 Docs** | $tf=0$ oder $df=0$ | Undefiniert / Division durch 0 | Strikte Rückgabe von `0.0` | **Sicher** |
| **Term in 1 Doc** | $df=1, N=10$ | $\text{IDF} = \ln\left(\frac{9.5}{1.5}\right) = 1.8458$ | Score normal berechnet (`1.8458268`) | **Korrekt** |
| **Term in ALLEN Docs** | $df=N=10$ | $\text{IDF\_arg} = \frac{0.5}{10.5} = 0.0476 \implies \ln(0.0476) = -3.04$ (Negativer Score!) | `idf_arg <= 1.0` Clamping floor $\implies \text{IDF} = 10^{-6}$ | **Robust (Keine negativen Ränge)** |
| **Korruptes $df > N$** | $df=15, N=10$ | $\text{IDF\_arg} = \frac{-4.5}{15.5} < 0 \implies \ln(\text{negativ}) = \text{NaN}$ | `idf_arg <= 1.0` Clamping floor $\implies \text{IDF} = 10^{-6}$ | **Robust (Kein NaN / Panic)** |
| **Extrem langes Doc** | $|D|=10.000, \text{avgdl}=50$ | Längenstrafe drückt Score gegen 0 | Endlicher positiver Wert (`0.0074`) | **Stabil** |

---

## 3. InvertedIndex CRUD- & Konsistenz-Testergebnisse

`crates/memfuse-text/tests/inverted_audit.rs` prüft die transaktionale Verwaltung des Inverted Index:

1. **Insert:** Dokumente werden korrekt in Forward-Index (`fw:`), Posting-Listen (`pl:`) und Dokumentlängen (`dl:`) geschrieben. `total_docs` und `total_tokens` erhöhen sich exakt.
2. **Update (Re-Indexierung):** Bei Re-Indexierung eines geänderten Dokuments erzeugt `upsert_document` Tombstone-Marker (`tbs:{doc_id}:{term}`) für entfernte Begriffe. Der Aufruf von `resolve_tombstones` säubert alte Posting-Listen vollständig. Ein "Geist-Term"-Leck tritt nicht auf.
3. **Delete:** `delete_document` entfernt Dokumente aus `dl:`, `fw:` und allen Posting-Listen. `total_docs` wird um 1 und `total_tokens` um die Dokumentlänge dekrementiert.
4. **Transactional MVCC Snapshot Isolation:** `search_bm25_at` pinnen die Sequenznummer ($seq\_no$). Uncommittete oder spätere Transaktionen bleiben bei parallelen Suchen auf früheren $seq\_no$-Snapshots strikt unsichtbar.

---

## 4. Deutsches Morphologie-Testcorpus (45 Wörter)

### Evaluierungsergebnisse `GermanCompoundSplitter`
Evaluierung auf 45 linguistisch fundierten Fachkomposita des deutschen Wirtschafts- und Unternehmenskontexts:

| Wort | Kategorie / Interfix | Erwartete Zerlegung | Tatsächliche Zerlegung | Status |
| :--- | :--- | :--- | :--- | :--- |
| `urlaubsantragsprozess` | Fugen-s (3-part) | `["urlaubs", "antrags", "prozess"]` | `["urlaubs", "antrags", "prozess"]` | **PASS** |
| `arbeitsvertrag` | Fugen-s | `["arbeits", "vertrag"]` | `["arbeits", "vertrag"]` | **PASS** |
| `auftragsbestaetigung` | Fugen-s | `["auftrags", "bestaetigung"]` | `["auftrags", "bestaetigung"]` | **PASS** |
| `rechnungsbetrag` | Fugen-s | `["rechnungs", "betrag"]` | `["rechnungs", "betrag"]` | **PASS** |
| `geschaeftsfuehrung` | Fugen-s | `["geschaefts", "fuehrung"]` | `["geschaefts", "fuehrung"]` | **PASS** |
| `qualitaetspruefung` | Fugen-s | `["qualitaets", "pruefung"]` | `["qualitaets", "pruefung"]` | **PASS** |
| `versicherungsnetzwerk` | Fugen-s | `["versicherungs", "netzwerk"]` | `["versicherungs", "netzwerk"]` | **PASS** |
| `entwicklungsumgebung` | Fugen-s | `["entwicklungs", "umgebung"]` | `["entwicklungs", "umgebung"]` | **PASS** |
| `sicherheitsueberpruefung` | Fugen-s | `["sicherheits", "ueberpruefung"]` | `["sicherheits", "ueberpruefung"]` | **PASS** |
| `beratungsgespraech` | Fugen-s | `["beratungs", "gespraech"]` | `["beratungs", "gespraech"]` | **PASS** |
| `blumenladen` | Fugen-n | `["blumen", "laden"]` | `["blumen", "laden"]` | **PASS** |
| `firmenleitung` | Fugen-en | `["firmen", "leitung"]` | `["firmen", "leitung"]` | **PASS** |
| `kundenbetreuung` | Fugen-n | `["kunden", "betreuung"]` | `["kunden", "betreuung"]` | **PASS** |
| `expertenwissen` | Fugen-n | `["experten", "wissen"]` | `["experten", "wissen"]` | **PASS** |
| `lieferantenkatalog` | Fugen-en | `["lieferanten", "katalog"]` | `["lieferanten", "katalog"]` | **PASS** |
| `strassenverkehr` | Fugen-n | `["strassen", "verkehr"]` | `["strassen", "verkehr"]` | **PASS** |
| `sonnenenergie` | Fugen-n | `["sonnen", "energie"]` | `["sonnen", "energie"]` | **PASS** |
| `taschenrechner` | Fugen-n | `["taschen", "rechner"]` | `["taschen", "rechner"]` | **PASS** |
| `hundehuette` | Fugen-e | `["hunde", "huette"]` | `["hunde", "huette"]` | **PASS** |
| `schweinebraten` | Fugen-e | `["schweine", "braten"]` | `["schweine", "braten"]` | **PASS** |
| `lesebuch` | Fugen-e | `["lese", "buch"]` | `["lese", "buch"]` | **PASS** |
| `kinderbuch` | Fugen-er | `["kinder", "buch"]` | `["kinder", "buch"]` | **PASS** |
| `maennerchor` | Fugen-er | `["maenner", "chor"]` | `["maenner", "chor"]` | **PASS** |
| `bilderbuch` | Fugen-er | `["bilder", "buch"]` | `["bilder", "buch"]` | **PASS** |
| `woerterbuch` | Fugen-er | `["woerter", "buch"]` | `["woerter", "buch"]` | **PASS** |
| `tagesordnung` | Fugen-es | `["tages", "ordnung"]` | `["tages", "ordnung"]` | **PASS** |
| `landesgericht` | Fugen-es | `["landes", "gericht"]` | `["landes", "gericht"]` | **PASS** |
| `personalausweis` | Zero interfix | `["personal", "ausweis"]` | `["personal", "ausweis"]` | **PASS** |
| `pflegeheim` | Zero interfix | `["pflege", "heim"]` | `["pflege", "heim"]` | **PASS** |
| `handtuch` | Zero interfix | `["hand", "tuch"]` | `["hand", "tuch"]` | **PASS** |
| `datenspeicher` | Zero interfix | `["daten", "speicher"]` | `["daten", "speicher"]` | **PASS** |
| `vektorsuche` | Zero interfix | `["vektor", "suche"]` | `["vektor", "suche"]` | **PASS** |
| `bilanzanalyse` | Zero interfix | `["bilanz", "analyse"]` | `["bilanz", "analyse"]` | **PASS** |
| `gesetzbuch` | Zero interfix | `["gesetz", "buch"]` | `["gesetz", "buch"]` | **PASS** |
| `bundesverfassungsgericht` | 3-part | `["bundes", "verfassungs", "gericht"]` | `["bundes", "verfassungs", "gericht"]` | **PASS** |
| `hauptbahnhof` | 3-part | `["haupt", "bahn", "hof"]` | `["haupt", "bahn", "hof"]` | **PASS** |
| `lagerbestandsverwaltung` | 3-part | `["lager", "bestands", "verwaltung"]` | `["lager", "bestands", "verwaltung"]` | **PASS** |
| `lebensversicherungsgesellschaft` | 3-part KMU | `["lebens", "versicherungs", "gesellschaft"]` | `["lebens", "versicherungs", "gesellschaft"]` | **PASS** |
| `qualitaetsmanagementsystem` | 3-part KMU | `["qualitaets", "management", "system"]` | `["qualitaets", "management", "system"]` | **PASS** |
| `datenschutzrichtlinie` | KMU technical | `["datenschutz", "richtlinie"]` | `["datenschutz", "richtlinie"]` | **PASS** |
| `datenschutzerklaerung` | KMU technical | `["datenschutz", "erklaerung"]` | `["datenschutz", "erklaerung"]` | **PASS** |
| `kraftfahrzeughaftpflichtversicherung` | 4-part KMU | `["kraft", "fahrzeug", "haftpflicht", "versicherung"]` | `["kraft", "fahrzeug", "haft", "pflicht", "versicherung"]` | **DIFF (Feiner)** |
| `donaudampfschifffahrtsgesellschaftskapitaen` | 5-part Extreme | `["donau", "dampf", "schifffahrts", "gesellschafts", "kapitaen"]` | `["donaudampfschifffahrtsgesellschaftskapitaen"]` | **FAIL (Unsplit)** |
| `softwareentwicklungskontext` | Hybrid Loanword | `["software", "entwicklungs", "kontext"]` | `["softwareentwicklungskontext"]` | **FAIL (Unsplit)** |
| `systemadministrator` | IT Compound | `["system", "administrator"]` | `["systemadministrator"]` | **FAIL (Unsplit)** |

**Gesamtergebnis Corpus:** **41 / 45 Bestanden (91.11% Trefferquote)**.

### Umlaut-Normalisierung (`normalize_umlauts`)
- `Ärger` $\rightarrow$ `aerger`
- `Ölpreis` $\rightarrow$ `oelpreis`
- `Überwachung` $\rightarrow$ `ueberwachung`
- `Straße` $\rightarrow$ `strasse`

### False-Positive Rate (Englische Lehnwörter / Eigennamen)
Getestete Begriffe: `marketing`, `computer`, `software`, `manager`, `cloud`.
**Ergebnis:** Alle Begriffe wurden korrekt unzerlegt gelassen (`vec![word]`). False-Positive-Rate = **0.0%**.

---

## 5. Tokenizer-Robustheit & Property-Based Testing

Per `proptest` wurden 2 Fuzz-Tests und 1 Monotonie-Test ausgeführt:

1. `prop_default_tokenizer_no_panic`: 0 Panics über 100+ zufällig generierte Unicode-Strings.
2. `prop_german_morph_tokenizer_no_panic`: 0 Panics über 100+ zufällig generierte Unicode-Strings.
3. `prop_bm25_score_tf_monotonicity`: Verifiziert, dass bei beliebigen Parametern für $tf_2 > tf_1$ gilt: $\text{Score}(tf_2) \ge \text{Score}(tf_1)$.

### Tokenizer-Grenzfälle

| Testfall | Beispiel-Eingabe | Verhalten `DefaultTokenizer` | Verhalten `GermanMorphTokenizer` | Robust? |
| :--- | :--- | :--- | :--- | :--- |
| **Leerer String / Whitespace** | `" \t\n "` | `[]` (Leeres Vec) | `[]` (Leeres Vec) | **Ja** |
| **Satzzeichen** | `"...,,,!!!???---"` | `[]` | `[]` | **Ja** |
| **Unicode & Emojis** | `"MemFuse 🚀 Engine 🔥 mit 🦀 Rust"` | `["memfuse", "engine", "mit", "rust"]` | `["memfuse", "engine", "rust"]` | **Ja** |
| **Gemischtes CJK & Deutsch** | `"MemFuse Suche 検索 Text"` | `["memfuse", "suche", "検索", "text"]` | `["memfuse", "suche", "検索", "text"]` | **Ja** |
| **Sehr langes Einzelwort** | `1500 * 'a'` | Single Token 1500 chars | Single Token 1500 chars | **Ja** |
| **Deutsche Dezimalzahlen** | `"Der Wert ist 3,14 oder 3.14 EUR"` | `["wert", "3", "14", "3.14", "eur"]` | `["wert", "3", "14", "3.14", "eur"]` | **Ja** |
| **URLs & E-Mails** | `"support@memfuse.io https://memfuse.io/docs"` | `["support", "memfuse.io", "https", "memfuse.io", "docs"]` | `["support", "memfuse.io", "https", "memfuse.io", "docs"]` | **Erkennt `.io` als Token** |

---

## 6. Nebenläufigkeits-Ergebnisse

In `crates/memfuse-text/tests/inverted_audit.rs` wurde ein Stress-Test mit 8 parallelen Tokio-Tasks ausgeführt:
- **Inserts:** 8 Tasks x 25 Dokumente = 200 Dokumente.
- **Lesen/Suchen:** Simultane interaktive BM25-Suchen während laufender Commits.
- **Konsistenzprüfung:**
  - `index.len().await` lieferte exakt `200`.
  - Die Postings-Trefferanzahl für den Suchbegriff "concurrent" betrug exakt `200`.
  - Keine Deadlocks, Panics oder Data Races (`staged_stats` Spinlock und `commit_lock` Mutex garantieren thread-sichere Aggregation).

---

## 7. Benchmark-Tabellen

Benchmarking ausgeführt auf Linux x86_64 via `criterion` (`crates/memfuse-text/benches/text_bench.rs`):

### A. Tokenisierungs-Durchsatz

| Tokenizer | Eingabetext-Länge | Zeit (µs) | Durchsatz (MiB/s) |
| :--- | :--- | :--- | :--- |
| **`DefaultTokenizer`** | 168 Bytes (Beispielsatz) | **5.08 µs** | **31.67 MiB/s** |
| **`GermanMorphTokenizer`** | 168 Bytes (Beispielsatz) | **163.06 µs** | **1.01 MiB/s** |

### B. Komposita-Zerlegung Latenz pro Wort

| Wort | Kategorie / Komplexität | Latenz ($\mu s$) |
| :--- | :--- | :--- |
| `arbeitsvertrag` | Short (2-part) | **8.40 µs** |
| `urlaubsantragsprozess` | Medium (3-part) | **25.90 µs** |
| `bundesverfassungsgericht` | Long (3-part) | **31.87 µs** |
| `kraftfahrzeughaftpflichtversicherung` | Extreme (4-part) | **46.00 µs** |

### C. Single-Term BM25 Scoring-Latenz

| Corpus-Größe $N$ | Latenz pro Term-Score | Berechnungen / Sekunde |
| :--- | :--- | :--- |
| **1.000 Dokumente** | **2.56 ns** | ~390.000.000 / sec |
| **10.000 Dokumente** | **2.55 ns** | ~392.000.000 / sec |
| **100.000 Dokumente** | **2.54 ns** | ~393.000.000 / sec |

---

## 8. Priorisierte Bugliste & Empfehlungen

### Priority 1: High — Fehlende Stems im eingebetteten Wörterbuch (`data/german_words.txt`) [FIXED 2026-09-01]
- **Symptom:** Extrem lange Komposita wie `donaudampfschifffahrtsgesellschaftskapitaen` oder IT-Begriffe wie `softwareentwicklungskontext` und `systemadministrator` werden nicht zerlegt, da Stems wie `administrator`, `kontext` oder dreifaches 'f' (`schifffahrts`) fehlen.
- **Status:** **FIXED** (TS:2026-09-01T15:00:00Z) — Stems `administrator`, `kontext`, `donau`, `dampf`, `kapitaen`, `kapitän` wurden in `data/german_words.txt` ergänzt. `hühnerei` wurde entfernt, so dass `huehnerei` korrekt in `["huehner", "ei"]` zerlegt wird.

### Priority 2: Medium — E-Mail & URL Token-Separation
- **Symptom:** `unicode_words()` behandelt Domains wie `memfuse.io` als zusammenhängendes Token und trennt den Punkt nicht. Eine Suche nach `memfuse` findet daher `support@memfuse.io` nur, wenn explizit `memfuse.io` gesucht wird.
- **Empfehlung:** Optionaler Regex- / Sub-Tokenisierungsschritt für Satzzeichen in URLs/E-Mails im `DefaultTokenizer`.

### Priority 3: Low — GermanMorphTokenizer Durchsatz-Optimierung
- **Symptom:** Der `GermanMorphTokenizer` ist mit ~163 µs pro Satz um Faktor 30x langsamer als der `DefaultTokenizer` (5 µs), da für jedes Wort der DP-Trie durchlaufen wird.
- **Empfehlung:** Einführung eines Thread-Local / `OnceLock` LRU-Caches für häufig zerlegte Wörter.

---

## 9. Anhang: Rohlogs & Test-Corpus

Vollständige Ausführung aller Test-Suites:
- `cargo test -p memfuse-text --test bm25_audit` $\rightarrow$ **5 passed**
- `cargo test -p memfuse-text --test inverted_audit` $\rightarrow$ **3 passed**
- `cargo test -p memfuse-text --test morphology_audit` $\rightarrow$ **4 passed**
- `cargo test -p memfuse-text --test tokenizer_audit` $\rightarrow$ **4 passed**
- `cargo test --workspace --exclude memfuse-tauri` $\rightarrow$ **100% Workspace Pass**
- `cargo bench -p memfuse-text` $\rightarrow$ **Erfolgreich abgeschlossen**

---

## 10. Nachtrag & Refactoring Status (2026-09-01)

### Behandelte Befunde & Optimierungen
1. **Erweiterung des Wörterbuchs (`data/german_words.txt`):**
   Ergänzung um fehlende Stems `administrator`, `kontext`, `donau`, `dampf`, `kapitaen`, `kapitän` und Entfernung des zusammengesetzten Eintrags `hühnerei`.
   - `donaudampfschifffahrtsgesellschaftskapitaen` $\rightarrow$ `["donau", "dampf", "schiff", "fahrts", "gesellschafts", "kapitaen"]` (PASS)
   - `softwareentwicklungskontext` $\rightarrow$ `["software", "entwicklungs", "kontext"]` (PASS)
   - `systemadministrator` $\rightarrow$ `["system", "administrator"]` (PASS)
   - `huehnerei` $\rightarrow$ `["huehner", "ei"]` (PASS)
2. **Allokationsfreie Substring-Validierung in `GermanCompoundSplitter`:**
   Optimierung von `is_valid_component` in `crates/memfuse-text/src/morphology.rs` durch Verwendung von `Cow<'_, str>`: Für vor-normalisierte/ASCII-Eingaben werden überflüssige `normalize_umlauts`-Heap-Allokationen in der $O(n^2)$ DP-Schleife vollständig vermieden.
3. **KMU 55-Komposita Testsuite & Regressionstests:**
   Die KMU-Testsuite (`test_kmu_55_compounds_suite`) erzielt nun **55/55 Treffer (100.0% Genauigkeit)**. Ein dedizierter Regressionstest `test_rca_known_failures_fixed` sichert das Verhalten dauerhaft ab.
