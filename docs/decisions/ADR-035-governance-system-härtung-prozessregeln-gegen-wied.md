# ADR-035: Governance-System-Härtung — Prozessregeln gegen wiederkehrende Trait-Default-, Typ-Dopplungs- und Stale-Finding-Fehler


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Über mehrere Wochen wiederholte sich in unabhängigen Audit-Zyklen desselben Projekts dasselbe Muster von Fehlerursachen: (1) Trait-Default-Fallen, (2) Typ-/Namensdopplungen, (3) Unverifiziertes Weiterschleifen veralteter Befunde, (4) Rein informatives Environment-Skript ohne Hard-Gate bei Blocker-Tags, (5) Word-identische Copy-Paste-SAFETY-Kommentare.
*   **Entscheidung**:
    1. **Trait-Default-Pflichttest-Regel**: Für jedes `pub trait` mit einer Default-Methode MUSS im selben PR, der einen neuen Implementor hinzufügt, ein Integrationstest existieren, der beweist, dass die Default-Implementierung nicht still greift.
    2. **Zentrales Typ-Register (`docs/TYPE_REGISTRY.md`)**: Vor Anlegen eines neuen Typs/Traits muss das Typ-Register nach Kollisionen durchsucht werden.
    3. **Audit-Intake-Verifikationsprotokoll (`.jules/AUDIT_INTAKE_PROTOCOL.md`)**: Jeder Finding aus externen Audit-Dokumenten MUSS vor Implementierung am aktuellen Quellcode gegengelesen und bei Obsoleszenz als "entkräftet" markiert werden.
    4. **Hard-Gate für BLOCKER-Tags**: `.jules/setup/environment_script.sh` bricht bei offenen `BLOCKER`-Tags mit `exit 1` ab (sofern keine explizite Blocker-Fix-Ausnahme gesetzt ist).
    5. **SAFETY-Kommentar-Unikats-Pflicht**: SAFETY-Kommentare müssen die konkrete Invariante der spezifischen Funktion benennen; word-identische Duplikate sind unzulässig.
    6. **JULES_CONTEXT.md Frischegarantie**: Warnhinweis am Dateianfang verlangt Gegenprüfung mit `WORKING_STATE.md` und aktuellem Code.
*   **Alternativen**: Weiterhin rein vertrauensbasierte Regeln ohne harte Prozess-Gates und zentrale Typ-Register. Verworfen wegen nachgewiesener wiederkehrender Fehler in Multi-Agenten-Sessions.
*   **Begründung**: Prozessuelle Härtung verhindert das Einschleichen schleichender Regressionen und reduziert Kontext-Halluzinationen in zukunftigen Jules-Sitzungen.
*   **Konsequenzen**:
    - `AGENTS.md`, `CONSTITUTION.md`, `docs/SOURCE_OF_TRUTH.md`, `rules/simd_safety.md` und `.jules/setup/environment_script.sh` aktualisiert.
    - Neue Dateien `docs/TYPE_REGISTRY.md` und `.jules/AUDIT_INTAKE_PROTOCOL.md` angelegt.

---
