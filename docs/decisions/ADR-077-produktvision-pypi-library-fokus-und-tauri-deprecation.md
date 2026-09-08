# ADR-077: Produktvision PyPI-Library Fokus und Tauri Deprecation

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Kontext / Auslöser:** Zielarchitektur v8.0 §6 & Entscheidungsdokumentation v1.0. Das Projekt führte zuvor drei unentschiedene Produktvisionen parallel (PyPI-Library, Desktop-Enterprise-App, Voice-Assistant).

## Entscheidung
1. **Verbindliche Fokussierung auf Option 1: PyPI-Library (Position A/B, ADR-007-Richtung)**. MemFuse wird primär als hochperformante, kryptographisch isolierte Embedded AI Memory Library für Python (`memfuse-py`) und Rust entwickelt.
2. **ADR-018 (Doppelstrategie) wird explizit durch diese ADR abgelöst (`superseded`)**.
3. **`memfuse-tauri` wird als `deprecated` eingestuft** und im Rahmen des Crate-Konsolidierungs-Fahrplans physisch aus dem Repository entfernt.

## Begründung
- Die Entwicklungsdynamik (Schwarm-Entwicklung, Solo-Architekt) erfordert maximale Fokussierung auf die Kernstärke: kaskadierende Retrieval-Qualität und kryptographische Mandantenisolation.
- Eine Desktop-Enterprise-App bindet erhebliche Ressourcen in UI/Desktop-Packaging (Tauri/GTK), ohne direkten Beitrag zur Inferenz- und Gedächtnisleistung.

## Konsequenzen
- `memfuse-py` bildet die primäre FFI-Grenzschicht.
- `memfuse-tauri` wird in Folgeschritten aus der Cargo-Workspace-Topologie entfernt.
- Doku-Artefakte und README/Architecture-Guides werden entsprechend aktualisiert.
