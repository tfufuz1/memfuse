# Vendor-Agnostisches LLM-Entwicklungs- & Härtungsprotokoll
> Referenziert aus `AGENTS.md`. Dieses Protokoll ist für ALLE AI-Systeme absolut bindend.

---

## 1. Das Sieben-Schleifen-Ausführungsprotokoll (Verification Loop Stack)

Jede Codeänderung MUSS diese sieben sequenziellen Schleifen durchlaufen. Wird eine Schleife übersprungen, gilt die Arbeit als fehlerhaft.

```mermaid
graph TD
    A[Schleife 1: Prä-Generierungs-Verifikation] --> B[Schleife 2: Selbstkritik vor Abschluss]
    B --> C[Schleife 3: Automatisierter Gate-Stack]
    C --> D[Schleife 4: Human-in-the-loop]
    D --> E[Schleife 5: Periodischer Drift-Audit]
    E --> F[Schleife 6: ADR-Pflicht]
    F --> G[Schleife 7: Rollentrennung Planer/Implementierer]
    G --> H[Schleife 8: Mehrfach-Review]
    H --> I[Erfolgreicher Commit/Done]
```

---

## 2. Die acht Schleifen im Detail

### Schleife 1: Prä-Generierungs-Verifikation (Read-Before-Write)
*   **API-Halluzinations-Schutz**: Bevor eine Methode, ein Struct, eine Funktion oder ein Crate-Export verwendet wird, muss der Pfad der API geöffnet und die exakte Signatur gelesen werden.
*   **Lockfile-Abgleich**: Die Version des Crates ist in `Cargo.lock` nachzuschlagen. Keine Versionen oder API-Strukturen aus dem Trainingsdaten-Wissen des LLM annehmen.

### Schleife 2: Selbstkritik vor Abschlussmeldung
*   **Diff-Review**: Bevor die Arbeit als abgeschlossen gemeldet wird, prüft der Agent den eigenen Diff gegen die `AGENTS.md`-Regeln (Boundary-Tiers, Tabus, Konventionen).
*   **Silent-Failure-Review**: Sicherstellen, dass keine Fehler stumm verschluckt werden (z. B. leere `catch`-Blöcke oder ungeprüfte standardmäßige Fallbacks bei Deserialisierung).

### Schleife 3: Automatisierter Gate-Stack
*   **Statische Analyse**: Code-Formatierung prüfen und Compiler/Linter-Warnungen als Fehler behandeln (`just check` ausführen).
*   **Test Gate**: Gesamte Testsuite ausführen (`just test` bzw. `just triple-test` bei Concurrency).
*   **Mutation-Gedankenexperiment**: Würde die Umkehrung eines Operators (< zu <=, + zu -) im geänderten Code mindestens einen Test brechen? Falls nein, ist die Testabdeckung ungenügend.
*   **Secret-Scan**: Überprüfung, dass keine Passwörter, API-Keys oder private Keys im Diff enthalten sind.

### Schleife 4: Human-in-the-loop-Checkpoints
*   **Ask-first-Verhalten**: Bei Aktionen, die als `ASK-FIRST` in `AGENTS.md` klassifiziert sind, muss der Agent die Ausführung pausieren und die explizite Freigabe des Benutzers einholen. Kein "impliziter Konsens".

### Schleife 5: Periodischer Drift-Audit
*   **Regel-vs-Code-Audit**: Regelmäßiger Abgleich, ob die in `AGENTS.md` und `rules/*.md` festgelegten Regeln noch dem tatsächlichen Stand des Codes und den Abhängigkeiten entsprechen.
*   **Sofort-Auslöser**: Jedes Major-Bump einer Abhängigkeit, jede Toolchain-Änderung und jede neue Crate lösen einen sofortigen Drift-Audit aus.

### Schleife 6: ADR-Pflicht
*   **ADR in docs/decisions/**: Jede nicht-triviale Architekturentscheidung muss in `docs/decisions/` dokumentiert werden (mit Datum, Status, Alternativen, Begründung), bevor mit der Umsetzung begonnen wird. Dies verhindert wiederholte Diskussionen oder Fehlklassifizierungen bewusster Abweichungen.

### Schleife 7: Rollentrennung Planer/Implementierer
*   **Phasen-Trennung**: Planung, Code-Generierung und Validierung sind strikt getrennt.
    1.  **Planer**: Dokumentiert nicht-triviale Entscheidungen in `docs/decisions/` (ADR) und wartet auf Genehmigung.
    2.  **Implementierer**: Arbeitet den priorisierten Backlog aus `docs/SOURCE_OF_TRUTH.md` ab.
    3.  **Verifizierer**: Führt `just triple-test` aus und aktualisiert den Status in `docs/SOURCE_OF_TRUTH.md`.

### Schleife 8: Mehrfach-Review (Unabhängige Session-Prüfdurchläufe)
*   **Mehrfach-Session-Pflicht**: Jede nicht-triviale Implementierung (mehr als 1 Datei, Public API, `unsafe`, Crypto, WAL oder Concurrency) erfordert MINDESTENS 2 (Standard) bzw. 3 (für `AGENTS.md §5 ASK`-sicherheitskritische Bereiche) `REVIEW-PASS`-Einträge mit `STATUS:PASS` von unterschiedlichen `SESSION:`-Hashes, bevor ein `ANCHOR` auf `STATUS:DONE` gesetzt werden darf.
*   **Unabhängigkeitsgebot**: Jeder `REVIEW-PASS` MUSS aus einer frischen Jules-Sitzung stammen (`PRÜFER-KONTEXT: FRESH`). Kein Agent darf eigene Änderungen selbst abzeichnen.
*   **Grammatik**:
    ```rust
    // REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL (ID: AGT-<CRATE>-<hash>) (TS: YYYY-MM-DDTHH:MM:SSZ) (SESSION: <hash>)
    // PRÜFER-KONTEXT: FRESH
    // BEFUND: Detaillierte Prüfungsergebnisse.
    ```

---

## 3. Das AI-TAG Härtungs-Protokoll (Anti-Drift)
Kann ein identifiziertes Risiko oder ein Architektur-Drift nicht sofort behoben werden, MUSS ein Inline-Tag gemäß der kanonischen Definition in [`rules/tag_taxonomy.md`](tag_taxonomy.md) hinterlassen werden:
```rust
// AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung des Problems (ID: AGT-<CRATE>-<hash>) (TS: YYYY-MM-DDTHH:MM:SSZ) (SESSION: <hash>)
// BEFUND: Detaillierte Analyse des Ist-Zustands im Code.
// RISIKO: Was passiert bei Last, Ausfall oder Edge Cases?
// EMPFEHLUNG: Konkreter Vorschlag für die Behebung.
// TODO[STABILIZE]: Priorität, Modul/Crate, Ziel-Strang.
```
Beim Abschluss:
```rust
// RESOLVED: AGT-XXXX — <fix> (TS: YYYY-MM-DDTHH:MM:SSZ) (SESSION: <hash>)
```
*   **Kanonische Spezifikation**: Die vollständige und einzig verbindliche Grammatik, Pflichtfelder (`TS:`, `SESSION:`, `AGT-<CRATE>-<hash>`) und CI-Gates sind in [`rules/tag_taxonomy.md`](tag_taxonomy.md) definiert.
*   **Kategorien**: `HALLUCINATION` | `DUPLICATION` | `TEST-MIRRORING` | `DEPENDENCY` | `SPEC-DRIFT` | `CONTEXT-GAP` | `BOUNDARY-MISSING` | `CONVENTION-DRIFT` | `CONCURRENCY` | `PANIC-SAFETY` | `SMELL`.
*   **Severities**: `BLOCKER` | `CRITICAL` | `MAJOR` | `MINOR`.

---

## 4. Prompt-Thrashing & Eskalation
Wenn nach zwei Iterationen derselbe Fehler oder Compiler-Fehlermeldung nicht behoben werden kann:
1.  **Stop & Report**: Codeänderungen stoppen (kein Vibe Coding).
2.  **Fehler-Isolation**: Den Fehler auf ein minimales Beispiel reduzieren.
3.  **Eskalation**: Dem Entwickler die Situation präsentieren und um präzise Instruktionen bitten.
