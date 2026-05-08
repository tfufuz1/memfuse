---
description: Spec-Driven Development (SDD) Workflow for Memfuse Coding Agents
---

# Spec-Driven Development (SDD) Workflow

Das System transformiert die Softwareentwicklung mit LLMs von unvorhersehbaren Antworten in einen deterministischen Prozess. Spezifikationen sind keine passiven Dokumente, sondern die **zentrale Steuereinheit (Blackboard-Prinzip)**.

## Workflow-Schritte

1. **Spec Konsultieren (Initiales Lesen)**
   - Bevor ein Agent Code schreibt, muss er die entsprechende Spec in `docs/specs/SPEC-<ID>.md` vollständig einlesen.
   - Extrahiere die Invarianten, Abhängigkeiten und Akzeptanzkriterien (AC).

2. **Wahrheitsgarantie & Alignment**
   - Ein Agent darf **niemals** Code schreiben, der nicht durch die Spec gedeckt ist.
   - Bestehen Unklarheiten (z.B. widersprüchliche Invarianten), muss die Implementierung pausiert werden, und ein `ANCHOR:BLOCKED` hinterlassen werden, bis der *Lead Architect* die Spec klärt.

3. **Status-Updates auf dem Blackboard**
   - Die `SPEC-*.md` Datei dient als "schwarzes Brett". Ändert ein Agent den Zustand (Implementiert, in Test, oder Fehler entdeckt), protokolliert er dies mit einem Datum in der Spec-Datei unter "Status" oder "Änderungsprotokoll".

4. **Spezifikationshierarchie (Top-Down)**
   - Alle Projekte sind in Domänen und Work Packages (WPs) zerteilt, z.B. `WP-1.1-Compaction`. Suche nach deinem WP-Namen.

// turbo-all
