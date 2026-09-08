# ADR-069: Standard-Terminologie statt biologischer Metaphern und Anbieter-Branding

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Autoren:** Principal Architect / Core Engineering Team
* **Kontext / Referenzen:** Refactoring-Phase P1–P5, ADR-005, ADR-020, ADR-066

## Kontext

Biologische Metaphern (z. B. "physio", "synaptic", "immune", "thermostat", "sleep cycle", "nucleation") führten in der Vergangenheit zu unnötiger kognitiver Last für neue Entwickler und erweckten den unzutreffenden Eindruck bionischer oder neuromorpher Systeme, obwohl es sich um mathematisch und algorithmisch präzise definierte Retrieval-, Indexierungs- und Datenbank-Komponenten handelt.

Gleichzeitig führt herstellerbezogenes Branding in Typnamen, Modulbezeichnungen oder Architekturbezeichnungen zu sachlich unzutreffenden Zuschreibungen, Verwirrung bezüglich Systemgrenzen und unbewussten Vendor-Lock-in-Assoziationen.

Im Rahmen der Refactoring-Phasen P1–P5 wurden diese Bezeichnungen systematisch bereinigt. Dieses ADR fixiert die resultierende Namens-Norm als verbindliche Vorgabe für alle zukünftigen Code-Beiträge, Architektur-Dokumente und Pull Requests.

## Entscheidung

Sämtliche zukünftige Beiträge im MemFuse-Workspace müssen sich an die nachfolgenden Normen für Typen, Feature-Flags und Architektur-Labels halten.

### 2.1 Typen & Schnittstellen

| Vorher (Metapher / Branding) | Nachher (MemFuse Norm) | Beschreibung / Funktion |
|---|---|---|
| `SynapticEdge` | `WeightedEdge` | Gewichtete Graph-Kante mit Vertraulichkeits- und Relevanz-Scores |
| `ThermostatDecay` / `FreeEnergyThermostat` | `AdaptiveDecay` | Dynamischer Abklingmechanismus für Speicherpunkte basierend auf Zugriffsintervallen |
| `ImmuneSuppression` / `ImmunMemory` | `GraphEdgeFilter` / `NodeSuppression` | Filterung und temporäre Unterdrückung widersprüchlicher Wissensgraphen-Kanten |
| `SleepCycleEngine` | `ConsolidationEngine` | Periodische Hintergrund-Konsolidierung, Index-Schnitt und Community-Synthese |
| `CognitiveOS` | `MemFuse Agentic Memory Engine` | Orchestrierung von Kontext, Langzeitspeicher und Werkzeugschnittstellen |
| `NucleationPruning` | `TombstonePruning` | Bereinigung gelöschter HNSW-Vektorknoten während Index-Rebuilds |

### 2.2 Feature-Flags

| Alt / Veraltet (`physio-*`) | Neues Norm-Feature-Flag | Verwendungsbereich |
|---|---|---|
| `physio-features` | `decay-thermostat` / `adaptive-decay` | Steuerung dynamischer Abklingungsfunktionen in `memfuse-db` |
| `physio-replicator-weights` | `adaptive-rrf-weights` | Kalibrierung von Multiplicative-Weights für RRF Fusion |
| `physio-synaptic-edges` | `weighted-graph-edges` | Aktivierung gewichteter Kanten im Wissensgraphen |
| `physio-percolation` | `graph-percolation` | Aktivierung von Graph-Perkolations-Algorithmen |
| `physio-resonance-fusion` | `resonance-fusion` | Resonanz-Kohärenz-Bonus bei Hybrid-Retrieval |
| `physio-nucleation` | `tombstone-pruning` | Experimentelles Pruning gelöschter Vektorknoten |

### 2.3 Architektur-Labels & Muster

| Anbieter-Branded / Metaphorisches Label | MemFuse Norm-Bezeichnung | Anwendungsfall |
|---|---|---|
| Provider-Branded Retrieval / Anthropic Contextual Retrieval | MemFuse Context-Prefix Retrieval Pattern | Anreicherung von Dokumentenchunks mit Kontext-Präfixen vor Embedding |
| Provider-Branded Routing / Conformal Cascade | MemFuse Conformal SLM Routing Pattern | Kalibrierte Modell-Auswahl und Kaskaden-Routing basierend auf Konfidenzen |
| Cognitive Memory Architecture | MemFuse Agentic Memory Engine | Multi-Layer-Architektur für lokale KI-Agenten-Speicherverwaltung |
| Bi-Temporal Knowledge Graph | MemFuse Bi-Temporal Graph Pattern | Zeitreihen- und Erfassungszeit-Tracking in Wissensgraphen |
| Unlearning / Deletion Proof | MemFuse Deletion Proof Pattern | GDPR Art. 17 konforme, kryptographisch nachweisbare Datenlöschung |

## Konsequenzen

1. **Verbot neuer `physio-*`-Feature-Flags:** Neue Beiträge dürfen unter keinen Umständen neue `physio-*`-Feature-Flags in `Cargo.toml`-Dateien oder bedingten Kompilierungsattributen (`#[cfg(feature = "...")]`) einführen. Bestehende historische Flags werden schrittweise gemäß Deprecation-Prozess migriert.
2. **Standardisierte Musterbezeichnungen:** Neue Architektur-Patterns, Dokumentationsabschnitte und Entwurfsmuster werden ausschließlich in der Form `"MemFuse [Funktion] Pattern"` bezeichnet. Anbieter-Namen oder vergleichendes Provider-Branding dürfen nicht als Namenspräfix für Repositorium-eigene Muster verwendet werden.
3. **Ausnahme für faktische Integrationsreferenzen:** Ausdrücklich von dieser Norm ausgenommen sind faktische, technisch erforderliche Schnittstellen- und Integrationsbezeichner. Dazu zählen:
   - Reale MCP-Client-Identifikatoren (z. B. `"Claude Desktop"`, `"VS Code MCP Host"`),
   - Reale Modell-IDs und Gewichts-Referenzen (z. B. `"bge-reranker-base"`, `"nomic-embed-text"`),
   - Protokoll-Standard-Spezifikationen (z. B. Model Context Protocol / MCP, JSON-RPC 2.0).
   Diese stellen keine herstellerbezogene Attribution von MemFuse-Architektur-Mustern dar, sondern sind funktionale Notwendigkeiten für Interoperabilität.
