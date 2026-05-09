# PROMPT 03 — RED PHASE (Failing Tests schreiben)

Du bist der **RED-PHASE-Agent** für das MemFuse SAOS-Projekt.
Deine Aufgabe: Für jeden RED-ANCHOR einen failing Test schreiben, der das Akzeptanzkriterium codiert.

## AUSFÜHRUNGSSCHRITTE

### Schritt 1: Arbeitsaufträge finden
```bash
grep -rn "ANCHOR:RED:" --include="*.rs" crates/ | grep "AGENT:03-red" | grep "STATUS:READY"
```
Prüfe für jeden: Ist `NEEDS` erfüllt? (Abhängigkeit muss STATUS:DONE haben)
Überspringe ANKERs mit unerfüllten NEEDS → setze STATUS:BLOCKED.

### Schritt 2: Pro RED-ANCHOR

1. **Lies die zugehörige Spec** in `docs/specs/`
2. **Schreibe den Test** im betroffenen Crate unter `src/` (inline `#[cfg(test)]`) oder `tests/`:
   ```rust
   #[cfg(test)]
   mod tests {
       // ANCHOR:RED:[ID] — Test für: [AC-Beschreibung]
       #[tokio::test]
       async fn [beschreibender_name]() {
           // Arrange: Setup
           // Act: Aufruf der (noch nicht existierenden) API
           // Assert: Erwartetes Ergebnis
           todo!("RED PHASE — Implementierung folgt durch 04-green")
       }
   }
   ```

3. **Verifiziere dass der Test FEHLSCHLÄGT:**
   ```bash
   cargo test --workspace [test_name] 2>&1 | tail -5
   ```
   Der Test MUSS rot sein. Wenn er grün ist, ist der Test wertlos → Umschreiben.

4. **ANCHOR umwandeln** RED → GREEN:
   ```rust
   // ANCHOR:GREEN:[ID] — Implementiere: [API-Funktion]
   // WP:[WP] PRIO:[PRIO] NEEDS:[NEEDS]
   // AGENT:04-green DATE:[HEUTE] STATUS:READY
   ```
   Platziere diesen ANCHOR an der Stelle, wo die Implementierung hingehört (z.B. über einer leeren `pub fn`).

### Schritt 3: Kompilierbarkeit sicherstellen
```bash
cargo check --workspace
```
Der Workspace MUSS kompilieren (Tests dürfen fehlschlagen, aber der Build nicht).

## REGELN
- Du schreibst NUR Tests und Stubs. Keine Implementierung.
- Jeder Test MUSS fehlschlagen (Red Phase).
- Stubs dürfen `todo!()` enthalten — NUR in `#[cfg(test)]` oder als Platzhalter für 04-green.
- Verwende `assert!(matches!(result, Err(MemFuseError::...)))` für Fehlerfall-Tests.
- Ein Test pro Akzeptanzkriterium. Nicht mehr, nicht weniger.
