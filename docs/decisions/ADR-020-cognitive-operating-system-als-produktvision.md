# ADR-020: Cognitive Operating System als Produktvision


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Der strategische Forschungsbericht 2026-08-26 zeigt:
    Der Wettbewerb (Mem0 ECAI-2025, Zep/Graphiti, MemOS) hat sich zu
    kognitiven Gedächtnisarchitekturen entwickelt. MemFuse als reiner
    "4-Signal RAG-Engine" ist 2026/2027 nicht SOTA.
*   **Entscheidung**: MemFuse positioniert sich als **Cognitive Operating
    System für LLM-Agenten**. Das bedeutet:
    - Explizite Differenzierung von Gedächtnistypen (Episodic/Semantic/
      Procedural/Working) als Roadmap-Ziel ab Phase 2
    - Temporale Wissensgraphen (bi-temporal) als Phase-2-Feature
    - Memory Consolidation als Phase-3-Feature
    Die 4-Signal-Architektur bleibt erhalten und ist die korrekte Basis.
    Der neue Begriff "Cognitive OS" beschreibt das Ziel-Endprodukt.
*   **Alternativen**:
    - Beibehaltung "4-Signal Memory Engine" — zu eng, kein Alleinstellungsmerkmal
    - Pivot auf Cloud-Service — widerspricht Sovereign-Core-Doktrin (ADR-004)
*   **Begründung**: Die Forschungslandschaft 2025/2026 (Generative Agents,
    Mem0, MIRIX, A-MEM, Trajectory-Informed Memory) zeigt: passive
    Speichersysteme verlieren gegen aktiv selbstorganisierende Gedächtnis-
    Architekturen. Der strategische Hebel ist Qualität und Kognitivität
    der Memory-Layer, nicht mehr nur Retrieval-Geschwindigkeit.
*   **Konsequenzen**:
    - README, SOURCE_OF_TRUTH, ARCHITECTURE werden auf "Cognitive OS"
      umformuliert (nicht nur "Memory Engine")
    - docs/memfuse_strategic_roadmap.md wird auf 4-Phasen-Plan aktualisiert
    - Phase-2-Features (Gedächtnistypen, temporaler Graph) als ADR-geplant

---
