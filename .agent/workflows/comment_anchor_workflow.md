---
description: Comment-ANCHOR System and Log-System for Memfuse Coding Agents
---

# Das Comment-ANCHOR System

Da LLMs Schwierigkeiten haben, globalen Kontext über Tausende Codezeilen zu halten, dient der Code selbst als **indirektes Kommunikations- und Auditprotokoll**. 

## Struktur-Syntax

`// ANCHOR:[TYP]:[COMP-ID] — [Grund/Status/Bemerkung]`

## Häufige Anwendungsfälle

- **Übergaben an andere Agenten:** Baut Agent A eine Komponente, die Agent B später anpassen muss, platziert er:
  `// ANCHOR:TODO:WP-2.1 — Implementiere hier die HNSW Suche...`
  
- **Sicherheits-Audit & Safety:** Verwendest du `unsafe` (nur in absoluten Ausnahmefällen wie SIMD `distance.rs`), MUSS ein `SAFETY` Anker stehen, der es dem nächsten Agenten oder Compiler zeigt:
  `// ANCHOR:SAFETY:SIMD-001 — Dieser Block liest aus Alignment-sicheren Vektoren...`

- **Architektur & Edge Cases:** Entscheidungen, die unerwartet wirken:
  `// ANCHOR:IMPL:WP-1.1 — Tombstone Cleanup überspringt aktuelle SeqNum, um Phantom Reads zu meiden.`

## Workflow

1. Jedes Mal, wenn du an einem Code-Abschnitt arbeitest, suche nach relevanten `// ANCHOR` am Datei-Beginn oder der Funktion.
2. Wenn du Code refaktorisierst oder als WIP markierst, hinterlasse Anker.
3. Lösche einen ANCHOR niemals wortlos! Ändere erst seinen Status (`STATUS:DONE`), dokumentiere ihn, und lass ihn falls er für Audit (z.B. SAFETY) ist, stehen.

// turbo-all
