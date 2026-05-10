# MemFuse SAOS — Sequential Inline-Kommentierungssystem
## JULES-ANCHOR v2.0 — State Machine & Handoff Edition

> **Basis:** AGENT_STANDARDS.md ANCHOR-System v1.0 & comment_anchor_workflow.md
> **Modell:** Continuous Development "Conveyor Belt"
> **Zweck:** Ermöglicht eine 100% autarke **Kettenreaktion**, in der jeder Agent nach getaner Arbeit zwingend den nächsten Agenten (Successor) mit expliziten Kommentaren ausstattet. Der Code orchestriert sich selbst — ohne statische Schedules.

---

## 1. Das Kernprinzip (The Conveyor Belt)

Ein JULES-ANCHOR ist ein **selbst-beschreibender Arbeitsauftrag** direkt im Code.
Jules muss keinen externen Task-Manager befragen. Der Code selbst propagiert die Ausführung:

- Jeder Agent hinterlässt am Ende seines Runs **exakte Anweisungen für den Nachfolger**.
- Die CI (`jules-queue-dispatcher.yml`) parst nach dem Merge die ANKER und ruft via Queue automatisch den logisch nächsten Agenten auf.
- **Dies ist ein Staffellauf (Relay Race). Kein Task ist jemals `DONE`, ohne dass der Staffelstab an den nächsten Agenten übergeben wird.**

---

## 2. Vollständige ANCHOR-Syntax

```rust
// ANCHOR:[TYP]:[ID] — [Einzeiler-Beschreibung / WHAT]
// WP:[work-package] PRIO:[1-5] NEEDS:[dependency-id|NONE]
// AGENT:[zuständiger-prompt] DATE:[YYYY-MM-DD] STATUS:[LIFECYCLE-STATUS]
// TEST:[test-befehl]
// DONE:[maschinenlesbares-erfolgskriterium]
// SUCCESSOR:[AGENT-ID] — [Was soll der Nachfolger als nächstes tun?]
```

### Pflichtfelder

| Feld | Bedeutung |
|------|-----------|
| `ANCHOR:[TYP]:[ID]` | Typ (z.B. `SPEC`, `RED`, `GREEN`, `REFACTOR`, `DEBT`) + Unique ID |
| `[Beschreibung]` | Kurzfassung des "WHAT" |
| `WP` | Zugehöriges Work Package (z.B. `WP-2.1`) |
| `PRIO` | 1 (Kritisch/Blocker) bis 5 (Nice-to-have) |
| `NEEDS` | Abhänigigkeiten zu anderen ANCHOR-IDs oder `NONE` |
| `AGENT` | Zuweisung, z.B. `@JULES-04` oder `04-green` |
| `DATE` & `STATUS` | Letztes Update und aktueller Status (z.B. `READY`, `ACTIVE`, `VERIFY`) |
| `TEST` | Beweismittel für Erfolg (Pflicht für RED/GREEN) |
| `DONE` | Definition of Done für DIESEN spezifischen Agenten |
| `SUCCESSOR` | **Kritisch:** Wer übernimmt danach? (z.B. `@JULES-05`) + Was genau? |

---

## 3. Status-Lifecycle & The Chain Rule

```text
STATUS-KETTE: PLANNING → READY → ACTIVE → VERIFY → DONE (inkl. SUCCESSOR-Übergabe)
```

### Die State-Machine der TYPEN

Ein Feature durchläuft im "Handoff"-Prozess verschiedene Typen. Jeder Typ ist eng an einen spezialisierten Agenten-Schritt gebunden. Wenn ein Agent fertig ist, mutiert er den ANKER-Typ für den Nachfolger:

1. **`SPEC`** (Spezifikation fehlt) → Status `DONE` + Mutation zu `ANCHOR:RED` für den Test-Agent.
2. **`RED`** (Test fehlt) → Agent schreibt Test → Status `DONE` + Mutation zu `ANCHOR:GREEN` für den Implementierer.
3. **`GREEN`** (Impl. fehlt) → Agent implementiert bis Test grün → Mutation zu `ANCHOR:REFACTOR` o. `ANCHOR:INTEGRATION`.
4. **`REFACTOR`** / **`INTEGRATION`** → Qualitätskontrolle → Status `DONE` (Finale Auflösung oder Handover an nächstes WP).

*(Selbst wenn ein System nicht strictly RED -> GREEN läuft, MUSS das Prinzip des explicit Handovers beachtet werden.)*

### Beispiele für SUCCESSOR-Handoffs

**Negativ-Beispiel (❌ VERBOTEN):**
```rust
// ...
// TEST: cargo test foo
// DONE: Test ist grün.
// STATUS: DONE
```
*Warum verboten? Die Kette der Automatisierung reißt ab.*

**Positiv-Beispiel (✅ PFLICHT):**
```rust
// ANCHOR:REFACTOR:WP-1.2-COL-001 — Collection WAL Storage Optimization
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:@JULES-02 DATE:2026-05-10 STATUS:READY
// TEST: cargo test -p memfuse-store
// DONE: Keine unnötigen Allokationen mehr.
// SUCCESSOR: @JULES-04 — "WAL Storage ist optimiert. Collection::open() kann nun hybrid gematcht werden."
```

---

## 4. Maschinelle Auswertung (Grepping the Queue)

Die CI/CD-Pipeline (`jules-queue-dispatcher.yml`) steuert den Workflow dynamisch basierend auf den `SUCCESSOR`-Definitionen und offenen `READY`-Ankern nach einem Merge.

```bash
# Nächsten aktiven Agenten finden:
grep -rn "STATUS:READY" --include="*.rs" --include="*.md" . | grep "AGENT:"

# Successor-Ketten auswerten:
grep -rn "SUCCESSOR:" --include="*.rs" --include="*.md" .
```

---

## 5. Externe Architektur ("The Triple-Test-Gate Limit")

Damit die Kette nie durch fehlerhaften Code zum Stillstand kommt, agiert der PR-Prozess als Gate:
- ANKER mutieren nur im finalen Main-Branch, wenn PR gemerged.
- CI/CD (`jules-queue-dispatcher.yml`) liest die Codebase des Main-Branches nach dem Merge (`auto-merge-jules.yml`), erkennt den `SUCCESSOR:` und fügt den Ziel-Agenten direkt in die Queue ein (`jules-sync-locks.sh` regelt Concurrent Domain Locking).
