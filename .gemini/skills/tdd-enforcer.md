---
name: "tdd-enforcer"
description: "Enforces the Test-Driven Development Loop (Red-Green-Refactor) using Just"
---

# TDD Enforcer Skill

Du befindest dich im **TDD Enforcer Modus**. Für die angeforderte Modifikation / Feature MUSST du exakt nach folgendem Protokoll vorgehen. Überspringe keine Phase.

## Phase 1: RED (Schreibe den Test)
1. Finde das passende Modul für den Test (meist im selben File als `#[cfg(test)] mod tests { ... }` oder in einem `tests/` Verzeichnis der Crate).
2. Schreibe einen asynchronen Testfall (`#[tokio::test]`), der das **gewünschte Verhalten** (aus der Atomic Spec) abbildet.
3. Der Test *sollte* aktuell fehlschlagen, da die Logik noch fehlt.

## Phase 2: GREEN (Implementieren)
1. Schreibe exakt die minimale Menge an fehlendem Rust-Code in der entsprechenden Crate, damit der Test grün wird.
2. Achte sofort auf die *Sovereign Core* Doctrine (kein `.unwrap()`, Rückgabe von `Result`, asynchrones Ticking).

## Phase 3: REFACTOR (Validierung & Lints)
Führe die System-Prüfung aus. Dies ist zwingend erforderlich!
Führe dieses Kommando in deiner Shell im Root (`/home/freddy/Arbeitsplatz/DEV/memfuse`) aus:

```bash
just test
```

*Hinweis*: `just test` ruft in MemFuse automatisch `just check` (fmt + clippy) und anschließend `cargo nextest run` auf.

### Umgang mit Fehlern
- Wenn eine Clippy Warning / Cargo Error auftritt: Ignoriere sie **niemals**. Lese den fehlerhaften Code und behebe das Problem (`Result` match fixen, Borrow Checker befriedigen, etc.).
- Wenn der Test immer noch Rot ist: Wiederhole Phase 2 und setze Debugging-Kommentare (oder `tracing::debug!`) ein.
- Wenn alles Grün ist, gibst du das Kommando an den Conductor zurück, dass diese Anforderung abgeschlossen ist.
