# Jules Prompt Library — Gemeinsame Präambel

Diese Datei enthält die **Standard-Präambel** die jedem Jules Scheduled Task vorangestellt wird.
Account-spezifische Kontexte liegen in `accounts/XX-NAME.md`.

---

## PRÄAMBEL (für alle Tasks kopieren)

```
Repository: dieses Repository (bereits verbunden via Jules Dashboard)
Basis-Branch: dev
Feature-Branch: jules/[ACCOUNT]-[TASK-NAME] (Jules erstellt diesen automatisch)

═══════════════════════════════════════════════════════════════
  ANCHOR v2 — INLINE-ZUSTANDSPROTOKOLL (PFLICHT-LEKTÜRE)
═══════════════════════════════════════════════════════════════

Du arbeitest mit dem ANCHOR v2 State-Machine-System.
Die ANCHOR-Kommentare im Code SIND deine Arbeitsaufträge.
Du findest, bearbeitest und erzeugst ANKERs — das ist der
primäre Koordinationsmechanismus zwischen allen Jules-Instanzen.

ANCHOR-SYNTAX:
  // ANCHOR:[TYP]:[ID] — [Beschreibung]
  // WP:[work-package] PRIO:[1-5] NEEDS:[dependency-id|NONE]
  // AGENT:[deine-account-nr] DATE:[YYYY-MM-DD] STATUS:[STATUS]

STATUS-LIFECYCLE:
  PLANNING → READY → ACTIVE → VERIFY → DONE

DEIN WORKFLOW (jeder Run, immer gleich):

  1. SCAN: Finde alle ANKERs die deinen AGENT-Tag tragen
     grep -rn "AGENT:[DEINE-NR]" crates/ --include="*.rs" | grep "STATUS:READY"

  2. PLAN: Für jeden gefundenen ANCHOR erstelle einen
     internen Implementierungsplan (in deinem Kopf, nicht als Datei)

  3. IMPLEMENT: Bearbeite den ANCHOR — schreibe Code, Tests, oder Docs
     Ändere STATUS:READY → STATUS:ACTIVE während du arbeitest

  4. VERIFY: Führe Tests aus
     cargo test --workspace
     cargo clippy --all-targets -- -D warnings

  5. ADVANCE: Wenn Tests grün → ändere TYP und AGENT zum nächsten Schritt:
     SPEC→RED (AGENT:03) → GREEN (AGENT:04) → REFACTOR (AGENT:05) →
     INTEGRATION (AGENT:06) → DONE
     Wenn Tests rot → setze STATUS:BLOCKED mit Grund

  6. ERZEUGEN: Wenn du neue Arbeit findest (z.B. Tech-Debt, fehlende
     Docs, Security-Issues), erzeuge neue ANKERs mit dem passenden
     AGENT-Tag für den zuständigen Account.

REGELN:
  - Bearbeite NUR ANKERs die DEINEN AGENT-Tag tragen und STATUS:READY haben
  - Prüfe NEEDS vor Bearbeitung — wenn Dependency nicht DONE → STATUS:BLOCKED
  - Maximal 5 ANKERs pro Run bearbeiten
  - Wenn KEINE ANKERs für dich existieren → Fallback-Aufgaben (siehe Account-Prompt)

═══════════════════════════════════════════════════════════════
  SOVEREIGN CORE DOCTRINE — ABSOLUT VERBINDLICH & ZERO-PANIC
═══════════════════════════════════════════════════════════════

1. ZERO-PANIC: Kein .unwrap() / .expect() im Produktionscode.
   → ? Operator + MemFuseError Propagation.

2. ASYNC-I/O: Kein std::fs in async Kontexten → tokio::fs.

3. ZERO-UNSAFE: #![forbid(unsafe_code)] in jedem Crate.
   Ausnahme: distance.rs (SIMD) mit SAFETY ANCHOR.

4. WARNINGS=ERRORS: cargo clippy -- -D warnings muss sauber sein.

5. DOC-PFLICHT: Jede pub struct/fn braucht /// Doc-Comment.

6. BACKWARD-COMPAT: Keine API-Signatur-Breaks ohne Spec.

═══════════════════════════════════════════════════════════════
  DONE-DEFINITION (Triple-Test-Gate)
═══════════════════════════════════════════════════════════════

Ein ANCHOR ist DONE wenn:
  1. Zugehörige Tests bestehen 3× hintereinander ohne Änderung
  2. cargo clippy -- -D warnings ist grün
  3. Keine bestehenden Tests sind neu rot
  4. ANCHOR-STATUS ist auf DONE gesetzt

═══════════════════════════════════════════════════════════════
  CONVEYOR BELT HANDOFF (SUCCESSOR PROTOCOL) - PFLICHT!
═══════════════════════════════════════════════════════════════

WICHTIG (ARCHITEKTUR-PFLICHT): Gemäß der Conveyor Belt / Dynamic Queue Architektur MUSST du jedes Mal, wenn du einen ANKER auf STATUS:DONE setzt, zwingend den nächsten Agenten in der Pipeline aufrufen.
Instruktion: Setze in den Code (oder das Markdown) als letzten Schritt folgenden Block:
// SUCCESSOR: @JULES-[nächster-Agent] — "[Deine Instruktion an den Nachfolger]"
(Hinweis: Falls du eine Aufgabe im Auftrag eines anderen bearbeitest, nutze anstelle des Standard-Nachfolgers den ursprünglichen Agenten).
OHNE SUCCESSOR BRICHT DIE AUTOMATISCHE CHAIN AB! ES WÜRDE EIN FATALER DEADLOCK ENTSTEHEN.

═══════════════════════════════════════════════════════════════
  ITERATIVE SELBSTKORREKTUR-SCHLEIFE (PFLICHT)
═══════════════════════════════════════════════════════════════

DOKTRIN: Du musst deinen Code in Schleifen und Mechanismen so lange iterativ überarbeiten und korrigieren, bis er wirklich 100% vollfunktionsfähig ist.
- Es muss immer testbar sein.
- Die Tests müssen durchgehend bestehen (Triple-Test-Gate).
- Wenn die Validierung (cargo test / clippy / kani) fehlschlägt, darfst du nicht aufgeben. Gehe direkt in die Fehleranalyse und Implementierungsphase zurück.
- Analysiere den Fehler, korrigiere den Code und verifiziere erneut.
- Du bleibst in dieser Schleife, bis 100% Funktionalität sichergestellt ist und die Tests (bzw. Checks) 3x erfolgreich durchlaufen sind.
```
