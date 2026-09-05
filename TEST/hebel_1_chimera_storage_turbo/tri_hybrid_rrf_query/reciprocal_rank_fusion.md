# Reciprocal Rank Fusion (RRF) in Project Chimera

Der **Reciprocal Rank Fusion (RRF)**-Algorithmus fungiert in Project Chimera als zentraler Mechanismus, um die Ergebnisse der drei verschiedenen Index-Engines (Vektor, Graph und Metadaten) in einer einzigen, hochrelevanten Rangliste zu vereinen. Da die verschiedenen Suchmethoden völlig unterschiedliche Metriken verwenden – beispielsweise die **Cosine Similarity** (0 bis 1) im Vektor-Index gegenüber **Graph-Hops** (ganze Zahlen) im Graph-Index –, macht RRF diese Werte vergleichbar, indem es sich rein auf die **Position (den Rang)** eines Dokuments konzentriert.

## 1. Die mathematische Berechnung

Für jede Ergebnisliste, die aus einer Suchmethode (z. B. Vektorsuche oder Graph-Traversierung) stammt, berechnet der Algorithmus für jedes enthaltene Dokument einen Teil-Score. Die Formel lautet:

$$ \text{Score} = \sum \frac{1.0}{k + \text{rank}} $$

*   **Rank:** Die Position des Dokuments in der jeweiligen Liste.
*   **k:** Eine Glättungskonstante, die in Project Chimera standardmäßig auf **60** gesetzt wird. Diese Konstante sorgt dafür, dass Dokumente auf den vorderen Plätzen nicht zu extrem gegenüber nachfolgenden gewichtet werden.

## 2. Aggregation der Einzelergebnisse

Das System führt die parallelen Suchergebnisse in einem vierstufigen Prozess zusammen:

1.  **Parallel Retrieval:** Der Vektor-Index, der Sparse-Index (lexikalische Suche) und der Graph-Index liefern jeweils ihre eigenen Ranglisten.
2.  **Summierung:** Ein Dokument, das in mehreren dieser Listen auftaucht, erhält einen kumulativen Score.
3.  **Ranking-Boost:** Dokumente, die in **mehreren Listen weit oben** stehen, gewinnen massiv an Bedeutung und steigen in der finalen Gesamtrangliste nach oben. Dies maximiert die Relevanz und Genauigkeit der Informationen, die dem LLM bereitgestellt werden.

## 3. Vorteile dieses Ansatzes

*   **Keine Normalisierung nötig:** Es ist keine komplizierte manuelle Gewichtung oder Umrechnung der unterschiedlichen Ähnlichkeitsmetriken erforderlich.
*   **Robustheit:** RRF gilt als sehr stabil gegenüber Ausreißern in den einzelnen Suchmethoden.
*   **Präzision:** Durch die Kombination semantischer, struktureller und lexikalischer Signale wird der Kontext für die Antwortgenerierung faktisch genauer, was Halluzinationen reduziert.

> **Metapher zur Verdeutlichung:**
> Stellen Sie sich vor, Sie suchen einen Experten für ein Thema. Ein Kollege gibt Ihnen eine Liste nach **Sympathie** (Vektorsuche), Ihr Chef eine Liste nach **Qualifikation** (Metadaten-Filter) und die Personalabteilung eine Liste nach **Berufserfahrung** (Graph-Struktur). Anstatt zu versuchen, "Sterne" mit "Jahren" zu verrechnen, schauen Sie einfach, wer in allen drei Listen auf den vorderen Plätzen steht. Wer überall unter den Top 10 ist, ist die sicherste Wahl – genau so filtert RRF die besten Dokumente heraus.
