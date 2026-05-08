---
description: Test-Driven Development (TDD) Workflow for Memfuse Coding Agents
---

# Der Sovereign Core TDD-Loop

TDD ist der mechanische Filter, der sicherstellt, dass **nur 100% funktionaler Code** in das Repositiory gelangt. Dies schließt "Heisenbugs" und typische KI-Halluzinationen konsequent aus.

## Workflow-Schritte

1. **Red Phase (Invariante definieren):**
   - Der Agent schreibt zuerst einen Test, der die Anforderung oder das Akzeptanzkriterium (AC) der Spec abbildet.
   - **Voraussetzung:** Dieser Test *MUSS* beim Ausführen zuerst fehlschlagen (RED). Er liefert den Kontext.

2. **Green Phase (Minimale Implementierung):**
   - Der Agent schreibt *exakt* so viel Code im Produktionssystem, dass der soeben geschriebene Test grün wird. Keine Over-Engineering, keine "Zusatzfeatures" ohne Spec.

3. **Triple-Test-Gate ausführen:**
   - Ein einzelner erfolgreicher Durchlauf reicht nicht aus (Flaky-Test-Ausschluss).
   - Der Agent Führt `just triple-test` aus, oder testet manuell 3x hintereinander in einer isolierten Umgebung:
     ```bash
     nix develop -c cargo test --workspace # Run 1
     nix develop -c cargo test --workspace # Run 2
     nix develop -c cargo test --workspace # Run 3
     ```
   - *Alle 3 Läufe* müssen ohne Code-Änderung dazwischen grün sein.

4. **Refactor Phase:**
   - Sobald das Triple-Test-Gate passiert wurde, kann der geschriebene Code gemäß den "Sovereign Core" Prinzipien (`clippy` warnings beheben, `.unwrap()`-Entfernungen durch `MemFuseError` Routing prüfen) umgeschrieben werden.
   - Nach Refactoring Triple-Test-Gate wiederholen.

// turbo-all
