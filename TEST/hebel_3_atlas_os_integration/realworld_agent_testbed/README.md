# Real-World Agent Testbed: Atlas OS & MemFuse

## 1. Zweck des Testbeds
MemFuse ist als universelle Low-Level-Speicherengine für anspruchsvolle KI-Workloads konzipiert. **Atlas OS** (das neuronale AI-Desktop-Betriebssystem) liefert das ideale Testbett, um MemFuse unter echten Produktionsbedingungen zu validieren:
- **LangGraph Multi-Agent Workflows:** Parallele Worker, Supervisor und Critic-Agenten, die gleichzeitig auf das Gedächtnis zugreifen.
- **Tauri Desktop IPC:** Kontinuierliche Abfragen und Event-Streaming zur grafischen Oberfläche.
- **A2UI Live-Streaming:** Generierte UI-Komponenten, die kontextsensitive Snippets in Echtzeit anfordern.
- **Micro-Latenz-Anforderungen:** Agenten dürfen bei Memory-Lookups keine Latenzspitzen erfahren.

## 2. Enthaltene Testbed-Komponenten

| Datei | Beschreibung |
|:---|:---|
| [`specialized_agents_memfuse.py`](./specialized_agents_memfuse.py) | LangGraph-Agenten-Harness mit integrierter MemFuse-Speicherschicht |
| [`stress_test_atlas_memfuse.py`](./stress_test_atlas_memfuse.py) | Paralleler Multi-Agent Stresstest (10 parallele Agenten, Messung von Durchsatz, Avg & P95 Latenz) |
| [`specialized_agents_original.py`](./specialized_agents_original.py) | Originale Atlas-Spezialagenten als Referenz |

## 3. Ausführung des Stresstests
In einer Umgebung mit Python 3 (`nix-shell` oder aktiviertem venv):
```bash
python3 TEST/hebel_3_atlas_os_integration/realworld_agent_testbed/stress_test_atlas_memfuse.py
```
Der Test simuliert:
- 10 nebenläufige Agenten-Worker
- Gleichzeitige Vektorsuchen, Decision-Logs und Kontextabfragen
- Automatische Erfassung von Durchsatz (ops/sec) und P95-Latenz
