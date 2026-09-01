# MemFuse — Umfassende Source-of-Truth-Strategie
## Vom souveränen RAG-System zur zukunftsfähigen State-of-the-Art-Plattform für LLM-Agenten

> **Dokumentstatus**: Strategisches Grundlagendokument, ergänzend zu `docs/SOURCE_OF_TRUTH.md`.
> **Stand**: 29. August 2026 | **Repository-Snapshot**: 557 Commits, ~58.850 Zeilen Rust, 15 Crates
> **Methodik**: Auswertung des vollständigen Git-Verlaufs, aller Audit-Reports und ADRs (40 Entscheidungen), sowie einer breit angelegten ArXiv-Recherche (Stand August 2026) zu Retrieval, Graph-Memory, Agenten-Orchestrierung, Inferenz-Optimierung, Kontext-Engineering und Memory-Sicherheit.
> **Mission**: MemFuse soll nicht nachziehen, sondern vorausdenken — eine lokale, souveräne Grundlage, die in Gedächtnisqualität, Nachvollziehbarkeit und struktureller Verlässlichkeit über das hinausgeht, was kommerzielle Chat-Produkte (ChatGPT, Gemini, Grok) heute bieten.

---

## Inhaltsverzeichnis

1. Executive Summary
2. Standortbestimmung: Wo MemFuse heute steht
3. Das Wettbewerbsfeld: Was ChatGPT, Gemini & Grok strukturell fehlt
4. Forschungslandkarte: Sechs Frontlinien mit direkter MemFuse-Relevanz
   4.1 Graph-Memory jenseits eines einzelnen Graphen (MAGMA, EverMemOS)
   4.2 Verifiable Memory Governance & Anti-Poisoning (Sicherheits-Fundament)
   4.3 Sleep-Time Compute & Memory-Konsolidierung (die "Auto-Dream"-Lücke)
   4.4 Context Engineering & Prompt-Cache-Ökonomie (Agenten-Effizienz)
   4.5 Graph-RAG-Retrieval der nächsten Generation (über PPR hinaus)
   4.6 Inferenz- und Routing-Optimierung (SLM-Kaskaden, KV-Cache-Kooperation)
5. Was sich saubér in MemFuse einbauen lässt — Crate für Crate
6. Priorisierte Forschungs-Roadmap (Was zuerst, was später, warum)
7. Die Positionierung: Der Anspruch, kommerziellen Chat-Produkten voraus zu sein
8. Risiken und offene Forschungsfragen (ehrliche Einordnung)
9. Zusammenfassung: Die neue Source of Truth in einem Absatz

---

## 1. Executive Summary

Diese Recherche wurde mit einem klaren Auftrag geführt: nicht bestätigen, was schon geplant ist, sondern **aktiv nach dem suchen, was MemFuse dauerhaft an die Spitze bringt** — spezifisch für RAG, Graph, Agenten, Chat, Inferenz und Kontext, und spezifisch geprüft auf saubere Umsetzbarkeit in der bestehenden Rust-Architektur.

Das Ergebnis in Kürze: **Die Grundintuition von MemFuse — ein 4-Signal-Hybrid-Store mit Graph, versioniert und auditierbar, lokal und souverän — liegt exakt auf der Linie, in die die aktuelle Forschung 2026 läuft.** Fünf Forschungsrichtungen sind reif genug, klar genug beschrieben und strukturell passend genug, um jetzt in die Roadmap aufgenommen zu werden:

1. **Multi-Graph-Memory statt Einzelgraph** (MAGMA, ACL 2026): Ein Gedächtniseintrag wird nicht in einem Graphen, sondern gleichzeitig in orthogonalen semantischen, temporalen, kausalen und Entitäts-Graphen abgebildet. MemFuse hat mit dem bi-temporalen Graphen (ADR-033) bereits die temporale Dimension — die Erweiterung auf echte Multi-Graph-Orthogonalität ist der nächste folgerichtige Schritt.
2. **Sleep-Time Compute / Memory-Konsolidierung** (Auto-Dreamer, EverMemOS, SCM — alle 2026): Arbeit, die heute in Phase 3 als "Memory Consolidation & Reflection" pauschal steht, ist inzwischen ein eigenständiges, gut erforschtes Forschungsfeld mit klaren Bausteinen (Engram-Lebenszyklus, NREM/REM-artige Konsolidierungsphasen, algorithmisches Vergessen). Anthropic selbst betreibt mit "Auto Dream" bereits produktiv genau dieses Konzept.
3. **Verifiable Memory Governance mit konkretem Angriffsmodell** (Poisoning-Forschung 2026): Die im letzten Strategiedokument beschriebenen fünf VMG-Primitive haben jetzt ein konkretes Gegenstück in der Angriffsforschung — Memory Poisoning erreicht in Benchmarks 41–74 % Erfolgsquote gegen ungeschützte Memory-Manager. Das macht VMG nicht mehr nur zu einem Compliance-Feature, sondern zu einer harten Sicherheitsnotwendigkeit.
4. **Hierarchisches, kausal-bewusstes Graph-Retrieval** (PathRAG, HippoRAG-Linie, CausalRAG2): Über die bereits implementierte PPR/Community-Detection hinaus zeigt die Forschung, dass explizite Pfad-Extraktion und Kausal-Graphen Multi-Hop-Reasoning-Qualität signifikant verbessern, ohne die Kosten von vollständigem GraphRAG (LLM-lastige Zusammenfassung jedes Subgraphen).
5. **Kalibrierte Kaskaden-Routing statt naiver Klassifikation** (UCCI, Cluster-Route-Escalate): `memfuse-router` existiert bereits — die Forschung zeigt, dass unkalibrierte Konfidenzwerte die Hauptfehlerquelle bei SLM-Routing sind, und liefert einen konkret nachrüstbaren Fix.

Diese Bausteine sind kein Wunschkonzert — jeder einzelne wurde daraufhin geprüft, ob er sich **ohne Bruch der bestehenden Architektur** (LSM-Storage, CSR-Graph, MVCC, Trait-System in `memfuse-core`) einfügt. Wo das nicht der Fall war (etwa direkte KV-Cache-Manipulation auf Transformer-Ebene), wird das im Dokument explizit als "nicht sauber implementierbar in MemFuse" ausgewiesen — Ehrlichkeit über Umsetzbarkeit ist Teil des Auftrags.

---

## 2. Standortbestimmung: Wo MemFuse heute steht

Diese Sektion fasst den technischen Reifegrad zusammen (Details siehe vorheriges Strategiedokument vom 29.08.2026, das hier vorausgesetzt wird):

- **557 Commits, ~58.850 Zeilen Rust, 15 Crates**, davon zwei neu seit der letzten Analyse: `memfuse-agent` (deterministischer Workflow-Orchestrator) und `memfuse-router` (SLM-Routing über Graph-Communities).
- **40 dokumentierte ADRs**, ein eigenes Prozess-Gate gegen wiederkehrende Fehlklassen (ADR-035), nur noch 6 offene AI-TAGs.
- **Bereits implementiert und damit State-of-the-Art-Basis**: 4-Signal-RRF-Fusion (Vektor/BM25/Graph/Metadaten), Contextual Retrieval, Cross-Encoder-Reranking, bi-temporale Graph-Kanten, Personalized PageRank, Label-Propagation-Community-Detection, LLM-gestützte Kontext-Konsolidierung mit Provenienz-Feld, HMAC-verkettetes WAL (V3), MVCC-Snapshot-Isolation für Storage/Text.
- **Bekannte offene Lücken**: Snapshot-Isolation für Vektor/Graph (ADR-024), Enterprise-Härtung erst Phase 4, Verifiable-Memory-Governance-Primitive nur teilweise vorhanden.

Das ist die Ausgangslage: eine technisch überdurchschnittlich saubere Basis, die bereits mehrere 2024/2025-Generation-RAG-Techniken produktiv umgesetzt hat. Die Frage dieses Dokuments ist: **Was kommt als Nächstes, damit die Basis nicht nur aktuell, sondern vorausschauend bleibt?**

---

## 3. Das Wettbewerbsfeld: Was ChatGPT, Gemini & Grok strukturell fehlt

Bevor die Forschungslandkarte im Detail folgt, lohnt sich die Einordnung, warum ein lokales, souveränes System kommerziellen Cloud-Chat-Produkten grundsätzlich in bestimmten Dimensionen überlegen sein *kann* — nicht durch mehr Rechenleistung (die haben sie), sondern durch strukturelle Eigenschaften, die ihr Geschäftsmodell ihnen erschwert:

- **Keine Werbeanreize, keine Cloud-Verpflichtung**: Kommerzielle Anbieter müssen Nutzerdaten in ihrer Infrastruktur halten, teils für Modellverbesserung nutzen. Ein air-gapped System kann versprechen, was ein Cloud-Anbieter strukturell nicht versprechen kann: dass keine Information das Gerät verlässt.
- **Memory-Transparenz ist bei den großen Anbietern noch rudimentär**: ChatGPTs "Saved Memories" und Geminis Kontext-Persistenz sind pragmatische, aber nicht durchgängig auditierbare Systeme — es gibt keine öffentlich zugängliche, kryptographisch verankerte Nachweisbarkeit, was gespeichert wurde, woher es kam und ob eine Löschung vollständig war.
- **Graph-strukturiertes Gedächtnis ist bei keinem der drei Marktführer öffentlich Kernarchitektur.** Ihre Memory-Systeme sind (öffentlich bekannt) primär vektor-/zusammenfassungsbasiert, nicht multi-graph-strukturiert im Sinne von MAGMA.
- **Determinismus und Reproduzierbarkeit** (wichtig für Enterprise/Compliance) sind bei nicht-selbst-gehosteten Systemen grundsätzlich schwerer zu garantieren — ein Modellupdate beim Anbieter kann das Verhalten über Nacht verändern, ohne dass der Nutzer das kontrollieren kann.

**Die ehrliche Einordnung:** MemFuse wird kommerzielle Frontier-Modelle nicht bei reiner Sprachmodell-Qualität (Reasoning, Weltwissen, Kreativität) überholen — das ist nicht die Aufgabe einer Memory-/Retrieval-Engine, sondern der zugrundeliegenden LLMs, die MemFuse über Ollama einbindet. Was MemFuse überholen *kann und sollte*, ist die **Gedächtnis-, Kontext- und Governance-Schicht darum herum** — und genau dort sind, wie die Recherche zeigt, alle drei kommerziellen Anbieter öffentlich nachweisbar schwächer aufgestellt als das, was aktuelle Forschung inzwischen für machbar hält.

---

## 4. Forschungslandkarte: Sechs Frontlinien mit direkter MemFuse-Relevanz

### 4.1 Graph-Memory jenseits eines einzelnen Graphen

**Zentrale Quelle**: MAGMA — *A Multi-Graph based Agentic Memory Architecture for AI Agents* (Jiang, Li, Li, Li; ACL 2026 Main Conference, arXiv:2601.03236).

MAGMA löst ein Problem, das MemFuse in seiner aktuellen Form ebenfalls hat: Ein CSR-Graph (auch ein bi-temporaler) vermischt semantische Ähnlichkeit, zeitliche Abfolge, kausale Abhängigkeit und reine Entitätsbeziehung in einer einzigen Kantenstruktur. MAGMA schlägt stattdessen vor, **denselben Gedächtniseintrag gleichzeitig in vier orthogonalen Graphen** abzubilden — semantisch, temporal, kausal, entitätsbasiert — und Retrieval als **policy-gesteuerte Traversierung über diese vier Sichten** zu formulieren, statt als reine Ähnlichkeitssuche. Das Ergebnis: interpretierbare Retrieval-Pfade (man kann nachvollziehen, *warum* ein Eintrag zurückkam) und in Benchmarks (LoCoMo, LongMemEval) konsistent bessere Ergebnisse als monolithische Memory-Stores.

**Ergänzend**: EverMemOS (Hu et al., ACL 2026, arXiv:2601.02163) beschreibt einen dreiphasigen "Engram-Lebenszyklus" — Episodic Trace Formation (Dialogstrom → strukturierte `MemCells` mit atomaren Fakten und *zeitgebundenem Foresight*), Semantic Consolidation (Zusammenfassung zu thematischen `MemScenes`, Aktualisierung von Nutzerprofilen), und Reconstructive Recollection (`MemScene`-gesteuertes agentisches Retrieval). Bemerkenswert: EverMemOS erreicht in eigenen Tests state-of-the-art-Resultate gegenüber MemOS (einem weiteren aktuellen Referenzsystem) und demonstriert explizit chat-orientierte Fähigkeiten wie Nutzerprofilierung und "Foresight" (proaktive Antizipation künftiger Bedürfnisse aus dem Gesprächsverlauf) — direkt relevant für deinen Wunsch nach Chat-Optimierung.

**Warum das zu MemFuse passt**: `memfuse-graph` ist bereits als eigenständiges Layer-1-Crate mit CSR-Struktur, PPR und Community-Detection vorhanden. Die Erweiterung auf mehrere orthogonale Graph-Namespaces (statt eines einzigen Graphen mit gemischten Kantentypen) ist eine additive strukturelle Erweiterung, kein Bruch — die bestehende CSR-Infrastruktur, LSM-Persistenz unter `__graph:`-Präfixen und Traversal-Algorithmen (BFS, PPR, LPA) lassen sich pro Graph-Dimension wiederverwenden.

### 4.2 Verifiable Memory Governance & Anti-Poisoning

**Zentrale Quellen**: *A Survey on Long-Term Memory Security in LLM Agents* (arXiv:2604.16548, definiert VMG — bereits im letzten Strategiedokument eingeführt); neu hinzugekommen: *Hidden in Memory: Sleeper Memory Poisoning in LLM Agents* (arXiv:2605.15338), *MemAudit: Post-hoc Auditing of Poisoned Agent Memory via Causal Attribution and Structural Anomaly Detection* (arXiv:2605.23723), *MemoryGraft: Persistent Compromise of LLM Agents via Poisoned Experience Retrieval* (arXiv:2512.16962).

Diese neuere Forschungswelle liefert etwas, das im letzten Dokument noch fehlte: ein **konkretes, gemessenes Angriffsmodell**. Memory-Poisoning-Angriffe (schädliche Inhalte werden ins Langzeitgedächtnis eingeschleust und beeinflussen spätere, scheinbar unabhängige Interaktionen) erreichen in kontrollierten Studien **41,0–73,9 % Erfolgsquote** gegen Systeme ohne dedizierte Abwehr. Das ist kein theoretisches Risiko mehr, sondern eine gemessene, hohe Erfolgsquote gegen den Status quo der meisten Memory-Systeme.

MemAudit liefert einen konkreten, nachrüstbaren Mechanismus: **kausale Attribution plus strukturelle Anomalieerkennung als Post-hoc-Audit** — statt (oder zusätzlich zu) präventiven Schreibautorisierungsprüfungen wird kontinuierlich geprüft, ob bestehende Memory-Einträge Anomaliemuster zeigen, die auf nachträgliche Kompromittierung hindeuten. Das ist die praktische, umsetzbare Ausprägung des VMG-Primitivs "Provenance Visibility" — nicht nur *woher kam ein Eintrag*, sondern *lässt sich rückwirkend erkennen, wenn ein Eintrag manipuliert wurde*.

**Warum das zu MemFuse passt**: MemFuse hat mit dem HMAC-verketteten WAL (ADR-029, WAL-V3) bereits eine kryptographische Integritätskette auf Storage-Ebene. Was fehlt, ist die Verknüpfung dieser Integritätskette mit einer **inhaltlichen** Anomalieerkennung auf dem semantischen Graphen — also die Kombination aus "ist der Datensatz technisch unverändert" (bereits gelöst) und "ist der *Inhalt* eines unveränderten Datensatzes plausibel im Kontext des restlichen Gedächtnisses" (noch offen).

### 4.3 Sleep-Time Compute & Memory-Konsolidierung

**Zentrale Quellen**: *Sleep-time Compute: Beyond Inference Scaling at Test-Time* (Lin et al., arXiv:2504.13171 — die Grundlagenarbeit); *Auto-Dreamer: Learning Offline Memory Consolidation for Language Agents* (arXiv:2605.20616); *SCM: Sleep-Consolidated Memory with Algorithmic Forgetting* (arXiv:2604.20943); EverMemOS (siehe 4.1).

Der Grundgedanke: Statt jede Konsolidierungsarbeit synchron während einer Nutzeranfrage zu erledigen, wird sie in Leerlaufzeiten ("Schlafphasen") verschoben — asynchron, mit Zeitbudget, ohne die Interaktionslatenz zu belasten. Anthropic betreibt mit "Auto Dream" (community-dokumentiert, in Claude Code produktiv) bereits genau dieses Muster: asynchrone Jobs mit Zuständen (pending/running/completed/failed), die Widersprüche auflösen, veraltete Einträge bereinigen und Konsolidierung in Batches durchführen — mit einem **Single-Writer-Invariant** (immer genau ein Akteur schreibt eine strukturierte Gedächtnisdatei, um konkurrierende Zustandskorruption zu vermeiden).

Auto-Dreamer geht methodisch weiter und trennt explizit "schnelle, sitzungsbezogene Aufnahme" von "langsamer, sitzungsübergreifender Konsolidierung": Ein gelernter Konsolidator behandelt einen ausgewählten Gedächtnisbereich als **schreibgeschützte Evidenz**, führt begrenzte Werkzeugnutzung durch (um Einträge und ihre provenienzverknüpften Quell-Trajektorien zu inspizieren), und synthetisiert einen kompakten Ersatzsatz, der über Sitzungen hinweg abstrahiert und redundante Einträge zusammenführt.

SCM (Sleep-Consolidated Memory) ergänzt ein konkretes, biologisch inspiriertes Phasenmodell: getrennte NREM- und REM-artige Konsolidierungsphasen, mehrdimensionale Wichtigkeits-Tags (Neuheit, emotionale Valenz, Aufgabenrelevanz, Wiederholungsfrequenz — deutlich reicher als ein einzelner Score), und *intentionales, wertbasiertes* Vergessen statt reinem Alter-basiertem Decay.

**Warum das zu MemFuse passt**: Phase 3 der aktuellen Roadmap ("Memory Consolidation & Reflection", Q1 2027) ist bereits als Ziel benannt, aber unspezifisch formuliert. `memfuse-agent` liefert mit dem Checkpoint→Execute→Commit→Audit-Loop bereits die technische Infrastruktur für genau solche asynchronen, checkpoint-gesicherten Hintergrundjobs — ein "Sleep-Cycle" ließe sich als spezialisierter `AgentTool`-Typ implementieren, der periodisch über `memfuse-agent`'s Event-Loop angestoßen wird und mit RAII-Checkpoint-Absicherung auf dem bestehenden `ContextCompactor` (ADR-032) aufbaut, statt eine komplett neue Infrastruktur zu benötigen.

### 4.4 Context Engineering & Prompt-Cache-Ökonomie

**Zentrale Quellen**: *Agentic Context Engineering* (arXiv:2510.04618); *Agentic Context Management for Long Horizon Tasks* (arXiv:2607.23809); *Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks* (arXiv:2601.06007); *The Missing Memory Hierarchy: Demand Paging for LLM Context Windows* (arXiv:2603.09023).

Diese Linie behandelt ein Problem, das mit `memfuse-agent`'s Ausrichtung auf mehrstufige Agenten-Workflows unmittelbar akut wird: Jeder Tool-Aufruf innerhalb einer Agenten-Sitzung fügt dem Kontext etwas hinzu (Aufruf, Ergebnis, nachfolgendes Reasoning), wodurch das Kontextfenster in typischen 30–50-Tool-Call-Sitzungen (Deep-Research-Assistenten, Coding-Agenten) schnell wächst. Zwei Erkenntnisse sind hier besonders handlungsrelevant:

- **"Don't Break the Cache"** zeigt, dass Prompt-Caching bei Agenten-Workloads (im Gegensatz zu statischen Frage-Antwort-Szenarien) besondere Sorgfalt braucht: dynamischer, sitzungsspezifischer Inhalt (Tool-Ergebnisse mit nutzerspezifischen Daten) bricht naive Cache-Strategien. Die Lösung liegt in einer bewussten Prompt-Struktur, die statische System-Prompts von dynamischen Tool-Ausgaben trennt, um Cache-Wiederverwendung zu maximieren.
- **"The Missing Memory Hierarchy"** überträgt das Konzept des **Demand Paging** aus dem Betriebssystembau auf LLM-Kontextfenster: Nicht der gesamte relevante Kontext muss im aktiven Fenster liegen — er kann bei Bedarf "eingeblendet" werden, ähnlich wie virtueller Speicher. Das ist konzeptionell fast identisch mit dem, was eine externe, persistente Memory-Engine wie MemFuse für ein LLM ohnehin leistet — nur dass die Forschung das jetzt explizit als Architekturprinzip statt als Zufallsergebnis eines RAG-Aufbaus benennt.

**ACE** (Agentic Context Engineering) behandelt den Kontext selbst als "sich entwickelndes Artefakt", das durch Selbstverbesserungs-Schleifen verfeinert wird — mit gemessenen Verbesserungen von +10,6 % auf Agenten-Benchmarks.

**Warum das zu MemFuse passt**: `memfuse-db`'s `ContextCompactor` (ADR-021, ADR-032) und `memfuse-agent`'s Token-Budget-Durchsetzung sind der naheliegende Ort, um Cache-bewusste Kontext-Anordnung (statische vs. dynamische Segmente strukturell trennen) und Demand-Paging-artiges Nachladen von Kontext-Chunks umzusetzen, statt alles im aktiven Fenster zu halten. Das ist eine der direktesten, kostengünstigsten Verbesserungen in dieser gesamten Recherche.

### 4.5 Graph-RAG-Retrieval der nächsten Generation

**Zentrale Quellen**: PathRAG (Chen et al. 2025, referenziert in mehreren 2026er-Surveys); *CausalRAG2: Hierarchical Causal Knowledge Graph Design for RAG* (arXiv:2602.05143); *HG-RAG: Hierarchy-Guided Retrieval-Augmented Generation for Structured Knowledge Graphs* (arXiv:2607.14095); *TagRAG* (arXiv:2601.05254).

Die aktuelle Forschung identifiziert ein Problem, das für MemFuse direkt relevant ist: Volles GraphRAG (im Sinne des ursprünglichen Microsoft-Papers) ist "prohibitively expensive" wegen häufiger LLM-Aufrufe für Community-Zusammenfassungen — genau die Falle, die MemFuse mit seiner deterministischen, LLM-freien PPR/LPA-Implementierung (ADR-026/027) bereits umgangen hat. Die neuere Forschungswelle baut in dieselbe Richtung weiter aus:

- **PathRAG** extrahiert explizite **relationale Pfade** (statt ganzer Subgraphen) und wandelt sie in Text-Prompts um — das führt zu kohärenterem, kontext-bewussterem LLM-Output als reine Knoten-Rückgabe.
- **CausalRAG2** organisiert das zugrundeliegende Wissensgraph nach **kausaler statt nur semantischer Nähe** und adressiert explizit das Problem, dass reine Multi-Hop-Suche über semantische Nachbarschaft "topically similar yet causally irrelevant evidence" zurückliefert — ein Problem, das MemFuse mit reiner PPR/BFS-Traversierung strukturell ebenfalls hat.
- **TagRAG/KET-RAG/HG-RAG** zeigen einen Trend zu **hierarchisch-hybriden** Strukturen (spärliches Graph-Skelett + Text-Keyword-Bipartit-Graph), die multi-granulares Retrieval ermöglichen, ohne einen vollständigen Graphen aufbauen zu müssen — relevant für Performance bei sehr großen Dokumentenmengen.

**Warum das zu MemFuse passt**: Der bi-temporale Graph (ADR-033) und PPR (ADR-026) sind die Grundlage, auf der eine kausale Kantendimension (im Sinne von MAGMA, siehe 4.1) additiv aufgebaut werden kann. Explizite Pfad-Extraktion (PathRAG-Stil) ist mit der bestehenden CSR-Traversal-Infrastruktur ohne architektonischen Bruch umsetzbar — es ist im Kern eine neue Retrieval-Strategie über denselben Graphen, kein neues Subsystem.

### 4.6 Inferenz- und Routing-Optimierung

**Zentrale Quellen**: *Dynamic Model Routing and Cascading for Efficient LLM Inference: A Survey* (arXiv:2603.04445); *UCCI: Calibrated Uncertainty for Cost-Optimal LLM Cascade Routing* (arXiv:2605.18796); *Cluster, Route, Escalate* (arXiv:2606.27457); *RAGCache: Efficient Knowledge Caching for Retrieval-Augmented Generation* (ACM TOCS, referenziert in arXiv:2607.08057); *RetrievalAttention: Accelerating Long-Context LLM Inference via Vector Retrieval* (arXiv:2409.10516/NeurIPS).

Zwei Unterthemen sind hier zu trennen, mit unterschiedlicher Umsetzbarkeit:

**a) SLM-Routing-Qualität** (direkt relevant für `memfuse-router`): Die aktuelle Forschung zeigt, dass die häufigste Fehlerquelle bei Modell-Kaskaden **unkalibrierte Konfidenzwerte** sind — rohe Token-Entropie oder Margin-Werte sind empfindlich gegenüber Prompt-Formulierung und übertragen sich schlecht zwischen Deployments. UCCI behandelt Kaskaden-Routing als **kalibriertes Entscheidungsproblem** statt als Tuning-Übung. "Cluster, Route, Escalate" schlägt ein zweistufiges Verfahren vor: Cluster-basiertes, kostenbewusstes Routing in Stufe 1, gefolgt von einer nachgelagerten **Qualitätsschätzung**, die bei Bedarf zu einem stärkeren Modell eskaliert — trainiert nur mit Korrektheits-Labels, ohne zusätzliche Annotation. Das ist eine direkte, konkret nachrüstbare Verbesserung für `memfuse-router`, dessen aktuelle Implementierung (siehe Code-Analyse) auf einer einfachen Score-Aggregation über Community-Zugehörigkeit basiert, ohne Kalibrierung.

**b) KV-Cache-/RAG-Cache-Kooperation** (nur teilweise umsetzbar, siehe Einschränkung unten): RAGCache demonstriert, dass sich Wissens-Caching speziell für RAG-Workloads lohnt — häufig abgerufene Dokumente/Chunks werden so vorgehalten, dass wiederholte Retrieval-Encode-Zyklen vermieden werden. RetrievalAttention nutzt Vektor-Retrieval, um selektiv nur die für eine Anfrage relevanten Teile eines langen Kontexts in den KV-Cache zu laden, statt den vollständigen Cache zu materialisieren.

**Wichtige Einschränkung zur Umsetzbarkeit**: Echte KV-Cache-Manipulation (Kompression, selektives Laden, Quantisierung) findet auf der Ebene der Inferenz-Engine selbst statt (llama.cpp/Ollama-Runtime), nicht in einer externen Rust-Datenbank. MemFuse kann diese Techniken **nicht direkt implementieren**, ohne eine eigene Inferenz-Engine zu bauen — das würde den Kern-Scope sprengen und ist hier explizit **nicht** empfohlen. Was MemFuse *kann*, ist die vorgelagerte Rolle übernehmen, die RAGCache und RetrievalAttention beschreiben: als externer, persistenter "Wissens-Cache", der Ollama mit bereits vorstrukturiertem, priorisiertem Kontext versorgt, sodass die Inferenz-Engine downstream weniger und gezielteren Kontext verarbeiten muss. Das ist eine Frage der Schnittstellen-Gestaltung zu `memfuse-ollama`, nicht der KV-Cache-Implementierung selbst.

---

## 5. Was sich sauber in MemFuse einbauen lässt — Crate für Crate

| Crate | Forschungsbezug | Konkrete Erweiterung | Architektonischer Bruch? |
|---|---|---|---|
| `memfuse-core` | VMG, MAGMA | Neue Trait-Erweiterung `ProvenanceRecord` (abfragbares Herkunfts-Objekt); ggf. `CausalEdge`-Typ als Ergänzung zu `Edge` | Nein — additiv, folgt dem bestehenden Muster von ADR-033 |
| `memfuse-store` | RAGCache-Prinzip | Priorisiertes Vorhalten häufig abgerufener Chunks als eigene "Hot-Tier"-Zone im LSM (analog zu bestehenden Compaction-Strategien) | Nein — Erweiterung der bestehenden LSM-Compaction-Logik |
| `memfuse-graph` | MAGMA, CausalRAG2, PathRAG | Mehrere orthogonale Graph-Namespaces (`__graph:semantic:`, `__graph:causal:`, bereits vorhanden: `__graph:entity:`, `__graph:edge:` für temporal via ADR-033); explizite Pfad-Extraktions-Retrieval-Strategie neben `Hops`/`PersonalizedPageRank` | Nein — folgt exakt dem in ADR-026 etablierten additiven `GraphTraversalStrategy`-Muster |
| `memfuse-db` | Context Engineering, Sleep-Time Compute | `ContextCompactor` um Cache-bewusste Segmenttrennung (statisch/dynamisch) erweitern; Sleep-Cycle als periodischer Konsolidierungsjob auf bestehender `consolidate_via_llm`-Basis (ADR-032) | Nein — additive Erweiterung bestehender Strukturen |
| `memfuse-agent` | Sleep-Time Compute, MemAudit | Dedizierter `AgentTool`-Typ für periodische Konsolidierung, abgesichert durch den bestehenden Checkpoint→Execute→Commit→Audit-Loop; Post-hoc-Anomalieerkennung als eigener Audit-Schritt | Nein — nutzt die vorhandene State-Machine wie vorgesehen |
| `memfuse-router` | Kalibrierte Kaskaden-Routing-Forschung | Ersetzen/Ergänzen der aktuellen Score-Aggregation durch kalibrierte Konfidenzschätzung vor der Eskalationsentscheidung | Nein — betrifft nur die Entscheidungslogik in `router.rs`, nicht die Schnittstelle |
| `memfuse-crypto` | VMG (Verified Forgetting) | Kryptographischer Löschbeweis (z. B. Merkle-Tree-basierte Bestätigung, dass ein Schlüssel in keinem WAL-Segment mehr referenziert wird) | Nein — Erweiterung der bestehenden HMAC-Infrastruktur |
| `memfuse-mcp` | VMG (Write Authorization) | Autorisierungsprüfung vor persistentem Schreibzugriff im Sandbox-Layer | Nein — MCP-Sandbox-Konzept existiert bereits, wird nur um Prüf-Gate ergänzt |
| `memfuse-ollama` | Prompt-Caching, RAGCache-Prinzip | Strukturierte Trennung von statischem System-Prompt-Anteil und dynamischem, sitzungsspezifischem Kontext beim Aufbau der Anfrage an Ollama | Nein — Formatierungsschicht, kein Strukturbruch |
| **Nicht empfohlen** | KV-Cache-Kompression/-Quantisierung, Speculative Decoding | — | **Ja** — gehört in die Inferenz-Engine (Ollama/llama.cpp selbst), nicht in MemFuse. Sauber implementierbar wäre höchstens eine Konfigurationsschnittstelle, die MemFuse an eine Ollama-Instanz mit entsprechenden Fähigkeiten weiterreicht — keine eigene Implementierung. |

---

## 6. Priorisierte Forschungs-Roadmap

Um Governance-Hygiene zu wahren (siehe letztes Strategiedokument, Befund zur Dokumentations-Drift), wird diese Roadmap explizit als **Ergänzung**, nicht als Ersatz der Phasen 2–4 in `docs/SOURCE_OF_TRUTH.md` formuliert. Empfehlung: Bei nächster Gelegenheit per ADR in die Haupt-Roadmap überführen.

### Kurzfristig (nächste 2–4 Wochen, baut auf bereits laufenden Sprints auf)
1. **Kalibriertes Kaskaden-Routing** in `memfuse-router` — kleinster Aufwand, direkter Qualitätsgewinn, kein struktureller Eingriff.
2. **Cache-bewusste Kontext-Trennung** (statisch/dynamisch) im `ContextCompactor` — direkte Kostenreduktion bei Ollama-Aufrufen, besonders relevant für `memfuse-agent`-Workflows mit vielen Tool-Aufrufen.
3. **Provenienz als abfragbares Objekt** (`ProvenanceRecord`) — Fortsetzung der bereits begonnenen VMG-Spur aus Teil 4 des letzten Strategiedokuments, jetzt mit MemAudit-Inspiration für die Anomalieerkennungs-Komponente.

### Mittelfristig (Q4 2026, ersetzt/konkretisiert die bisherige Phase-2-Formulierung)
4. **Sleep-Cycle-Konsolidierung** als spezialisierter `memfuse-agent`-Workflow, aufbauend auf `consolidate_via_llm` (ADR-032) — macht aus der bisher vagen "Memory Consolidation & Reflection"-Phase ein konkretes, in der Forschung gut fundiertes Feature mit klarer Referenzarchitektur (Auto-Dreamer/EverMemOS-Muster).
5. **Kausale Graph-Dimension** als Ergänzung zum bi-temporalen Graphen — erster Schritt in Richtung MAGMA-Multi-Graph-Architektur, ohne sofort alle vier Dimensionen zu bauen.
6. **Post-hoc-Memory-Audit** (MemAudit-Prinzip) auf dem bestehenden Graphen — strukturelle Anomalieerkennung als ergänzender Baustein zur bereits vorhandenen kryptographischen Integritätsprüfung.

### Langfristig (Q1–Q2 2027, ersetzt/konkretisiert Phase 3)
7. **Vollständige Multi-Graph-Architektur** (semantisch, temporal, kausal, entitätsbasiert) mit policy-gesteuerter Traversierung — die konsequente Weiterentwicklung von Punkt 5.
8. **Explizite Pfad-Extraktions-Retrieval-Strategie** (PathRAG-Stil) als dritte Option neben `Hops` und `PersonalizedPageRank` in `GraphTraversalStrategy`.
9. **Kryptographischer Löschbeweis** (Verified Forgetting, letzte VMG-Primitive) — technisch anspruchsvollster Punkt, bewusst zuletzt, da er auf allen vorherigen Provenienz- und Audit-Bausteinen aufbaut.

---

## 7. Die Positionierung: Der Anspruch, kommerziellen Chat-Produkten voraus zu sein

Die Forschungslage stützt einen realistischen, aber ambitionierten Anspruch: **MemFuse kann in der Gedächtnis- und Kontext-Governance-Schicht state-of-the-art sein — nicht als Ersatz für ein Frontier-Sprachmodell, sondern als das, was jedes Sprachmodell (egal ob lokal via Ollama oder perspektivisch angebunden) an Gedächtnis, Nachvollziehbarkeit und struktureller Kohärenz umgibt.**

Konkret bedeutet das für die Außendarstellung:

- **Gegenüber ChatGPT/Gemini "Memory"-Features**: MemFuse kann nachweisbar machen, was diese Systeme öffentlich nicht versprechen — vollständige Herkunftsketten, versionierte Rücksetzbarkeit, beweisbares Vergessen.
- **Gegenüber generischen RAG-Frameworks** (LangChain, LlamaIndex + Vektor-DB): MemFuse kann durch Multi-Graph-Struktur und kausale Kohärenz Retrieval-Qualität liefern, die reine Vektor-Ähnlichkeit strukturell nicht erreicht.
- **Gegenüber Mem0/MemGPT/Letta**: MemFuse kann durch die Kombination aus lokaler Souveränität, kryptographischer Integrität und Multi-Graph-Retrieval eine Kombination bieten, die keines dieser Systeme laut der ausgewerteten Forschung vollständig zusammen anbietet.

Der Anspruch "besser als ChatGPT/Gemini/Grok" ist **nicht** im Sinne von "besseres Sprachmodell" zu verstehen, sondern im Sinne von: **Ein Nutzer, der MemFuse mit einem beliebigen starken lokalen oder angebundenen Modell kombiniert, bekommt eine Gedächtnis- und Kontext-Qualität, die kein kommerzielles Chat-Produkt heute strukturell liefert** — weil deren Geschäftsmodell und Architektur nicht auf lückenlose lokale Governance ausgelegt sind.

---

## 8. Risiken und offene Forschungsfragen (ehrliche Einordnung)

Nicht alles in dieser Recherche ist reif oder risikofrei:

- **Multi-Graph-Architekturen (MAGMA) erhöhen die Systemkomplexität spürbar.** Vier orthogonale Graphen zu pflegen kostet mehr Speicher und mehr Wartungsaufwand als ein Graph. Die Roadmap sieht deshalb bewusst einen graduellen Einstieg vor (erst kausale Dimension, dann vollständige Orthogonalität), statt alles auf einmal zu bauen.
- **Sleep-Time-Compute-Konzepte sind größtenteils noch Forschungspreprints (2026), nicht production-battle-tested** — mit Ausnahme von Anthropics eigenem "Auto Dream", das aber nicht öffentlich dokumentiert ist (nur community-berichtet). Die MemFuse-Implementierung sollte als eigenständige, konservative Umsetzung verstanden werden, nicht als Nachbau eines bekannten Systems.
- **Memory-Poisoning-Abwehr ist ein aktives Wettrüsten.** Die zitierte Forschung zeigt selbst, dass bestehende Abwehrmechanismen "brittle across models and adaptive attacks" sind — es gibt keine Garantie auf vollständige Sicherheit, nur auf eine deutlich verbesserte Ausgangslage gegenüber dem Status quo.
- **KV-Cache-/Inferenz-Optimierung liegt bewusst außerhalb des empfohlenen Scopes** (siehe 4.6) — hier zu investieren würde MemFuse in Konkurrenz zu Ollama/llama.cpp selbst bringen, was strategisch nicht sinnvoll ist.

---

## 9. Zusammenfassung: Die neue Source of Truth in einem Absatz

MemFuse steht an einem Punkt, an dem die eigene, bereits gebaute Substanz (bi-temporaler Graph, PPR, HMAC-Integritätskette, deterministischer Agent-Orchestrator, SLM-Router) exakt auf die Linie trifft, in die aktuelle, teils erst 2026 auf ACL/arXiv veröffentlichte Forschung läuft. Die konsequente nächste Ausbaustufe ist nicht ein neues Produktkonzept, sondern die **Vertiefung entlang von fünf konkreten, sauber in die bestehende Rust-Architektur einfügbaren Linien**: Multi-Graph-Memory statt Einzelgraph, Sleep-Time-Compute-Konsolidierung statt vager "Reflection"-Phase, kausal-attributierte Provenienz-Audits statt reiner kryptographischer Integrität, kalibriertes statt naives SLM-Routing, und explizite Pfad-Extraktion statt reiner Hop-Traversierung. Wird diese Roadmap konsequent verfolgt, positioniert sich MemFuse nicht als Nachbau bestehender Memory-Systeme, sondern als eines der architektonisch am weitesten fortgeschrittenen souveränen Gedächtnissysteme, die öffentlich dokumentiert sind — mit einem strukturellen Vorsprung gegenüber kommerziellen Chat-Produkten genau dort, wo deren Geschäftsmodell es ihnen erschwert, nachzuziehen: bei lückenloser lokaler Governance über das, was ein Agent weiß, woher es kam, und ob es wirklich vergessen werden kann.
