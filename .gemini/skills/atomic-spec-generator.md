---
name: "atomic-spec-generator"
description: "Generates a highly dense, focused Atomic Specification before writing code"
---

# Atomic Spec Generator Skill

Bevor eine neue Funktion / ein Modul implementiert wird, generierst du hierfür ein "Atomic Spec"-Dokument.

## Vorbereitung
Kopiere das vorgegebene Template aus `docs/specs/TEMPLATE_ATOMIC_SPEC.md` nach `docs/specs/SPEC-<DATUM>-<Name>.md` (Bsp: `SPEC-20260505-hnsw-delete.md`).

## Inhaltliches Befüllen
Erstelle das Dokument extrem kondensiert und lösungsorientiert:
- **Kontext / Ziel**: Warum bauen wir das? (1 Satz)
- **Die Invariante(n)**: Was muss am Ende zwingend WAHR sein? Formuliere das als strikte `[INV-...]: ...` Regel.
- **Speicherort / Betroffene Crate**: In welcher Crate/Datei passiert die Änderung?
- **Datenstrukturen**: Beschreibe sehr kurz neue Structs/Enums.
- **Fail-Cases**: Was ist das exakte Fehler-Verhalten, das eintreten soll (welcher `MemFuseError` wird bei Fehlern returned)?

*Niemals schwammigen Text schreiben. Coder-Agents lesen dieses Dokument asynchron, es muss präzise auf den Punkt sein und wie eine Checkliste für TDD funktionieren.*
