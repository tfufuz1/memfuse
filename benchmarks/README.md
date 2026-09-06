# MemFuse — Benchmark Harness für Retrieval-Qualität (`memfuse-bench`)

Dieses Verzeichnis enthält das eigenständige, reproduzierbare Benchmark-Harness `memfuse-bench` zur Messung und Validierung der Retrieval-Genauigkeit von MemFuse.

## Schnellstart

Zum Ausführen der Benchmarks:

```bash
# Ausführen des Benchmark-Harnesses
cargo run -p memfuse-bench --release

# Ausführen der Unit- & Integrationstests des Benchmark-Harnesses
cargo test -p memfuse-bench
```

Nach der Ausführung werden die detaillierten Ergebnisse automatisch in zwei Formaten abgelegt:
- Maschinelles JSON-Format: `benchmarks/results/results.json`
- Menschenlesbarer Markdown-Bericht: `benchmarks/results/summary.md`

---

## Implementierte Benchmark-Szenarien

### Szenario A: "Baseline vs. Kontext-Präfix"
Vergleicht die Retrieval-Genauigkeit (Recall@1, Recall@3, Recall@5, MRR und Fehlerrate) einer Suche **OHNE** das LLM-generierte Kontext-Präfix-Feature gegen dieselbe Suche **MIT** aktiviertem Kontext-Präfix (`has_context_prefix`).

- **Testprinzip**: Der Korpus enthält Dokument-Chunks mit identischem oder hochgradig ähnlichem Rumpftext (z. B. allgemeine Vertragsklauseln wie `§4 Haftung`), die aus unterschiedlichen Verträgen stammen (z. B. *B2B Lieferbedingungen* vs. *Verbraucher AGB* vs. *Datenschutzerklärung*).
- **Wirkung**: Ohne Kontext-Präfix liefert die Volltext-Suche (BM25) für spezifische Kontext-Anfragen identische Scores für alle Chunks. Mit Kontext-Präfix injiziert MemFuse das Dokumenten-Kontext-Präfix in die Indizierung, sodass BM25 das exakt gemeinte Ziel-Dokument auf Rank 1 hebt.

### Szenario B: "Ohne vs. mit Cross-Encoder-Reranking"
Vergleicht die Retrieval-Qualität der Standard 4-Signal RRF-Fusion (BM25 + HNSW-Vektor + Graph) gegen eine Suche mit folgendem **Cross-Encoder Reranking** (`CrossEncoderReranker`).

- **Testprinzip**: Der Korpus prüft Anfragen mit hoher Keyword-Überlappung oder starken Vektor-Ahnlichkeiten zu Ablenk-Dokumenten (Distraktoren).
- **Wirkung**: Nach der initialen RRF-Kandidatenauswahl re-evaluiert der Cross-Encoder die (Query, Dokument)-Paare und korrigiert die Reihenfolge zugunsten des semantisch präzisesten Ziel-Dokuments.
- **Hinweis zu Modellgewichtungen**: Befindet sich die ONNX-Modelldatei `models/bge-reranker-base.onnx` nicht auf dem Datenträger (z. B. in air-gapped CI-Umgebungen ohne Download externer Gewichte), nutzt das Harness automatisch den deterministischen `CrossEncoderReranker::passthrough()`-Fallback zur Validierung der End-to-End Pipeline.

---

## Beschreibung des Testkorpus & Grenzen der Aussagekraft

- **Korpus-Struktur**: Der Benchmark nutzt einen integrierten, deterministischen synthetischen Testkorpus (8 Dokument-Chunks, 9 vordefinierte Testabfragen) mit explizit annotierter Ground Truth (`relevant_doc_ids`).
- **Grenzen der Repräsentativität**:
  1. **Größe & Skalierung**: Der synthetische Korpus ist bewusst kompakt gehalten, um schnelle, reproduzierbare CI-Läufe ohne exzessive Laufzeit zu gewährleisten. Er trifft **keine Aussage** über Skalierbarkeit auf Millionen von Dokumenten (hierfür existieren gesonderte Performance-Benches unter `benches/`).
  2. **Domänenspezifik**: Der Korpus deckt typische vertragliche, rechtliche und technische Ambiguitäten ab, ersetzt jedoch keine projektspezifischen Evaluierungen auf echten Echtdaten-Korpora.

---

## Einordnung der Messwerte zu externen Forschungswerten

In Marketing-Materialien und in der Literatur (z. B. Anthropic 2024 *Contextual Retrieval Paper*) werden häufig generische Prozentzahlen genannt:
- *49% weniger Retrieval-Fehler durch Contextual Retrieval*
- *67% weniger Fehler in Kombination mit Cross-Encoder Reranking*

**Transparenz-Hinweis & ehrliche Einordnung**:
1. **Herkunft der Literaturwerte**: Die Werte (49 % / 67 %) stammen aus empirischen Messungen auf riesigen Multi-Tausend-Chunk Benchmark-Datensätzen (Codebases, Enterprise-Wikis) unter Nutzung proprietärer Modelle.
2. **MemFuse-eigene Messung**: Das hiesige Benchmark-Harness `memfuse-bench` misst die tatsächliche MemFuse-Implementierung in isolierten, reproduzierbaren Test-Szenarien. Auf unserem synthetischen Ambiguitäts-Testkorpus erreicht MemFuse aktuell:
   - **Szenario A (Kontext-Präfix)**: Recall@1 von 100.0% (100.0% Baseline auf synthetischem Korpus).
   - **Szenario B (Reranking)**: Recall@1 von 75.0% bei RRF-Fusion.
3. Die gemessenen Werte dienen in erster Linie der Kontrollierbarkeit und automatisierten Regressionsvermeidung bei Code-Änderungen an der MemFuse-Engine.
