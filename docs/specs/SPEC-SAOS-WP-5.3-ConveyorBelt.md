# SPEC-20260510-WP-5.3-ConveyorBelt

> **Phase:** Specification
> **Doctrine:** Sovereign Core / Autonomous Flow
> **Autor:** SAOS
> **Updated:** 2026-05-10

## 1. Abstract
Dieses Dokument spezifiziert das "Dynamic Queue Dispatcher" (Conveyor Belt) Architektur pattern, welches die klassische Cron-basierte Agent-Abarbeitung ersetzt. Ziel ist es, API-Limits (Free Tier) zu respektieren, Idle-Zeiten zu reduzieren und dynamische Handoffs (Successors) durch direkten Git-Diff-Scan zu erzwingen.

## 2. Architektur

### 2.1. Der Successor-State
Bisherige Anker waren statusbasiert. Das SAOS-Team nutzt jetzt Event-Routing.
Wenn ein Agent einen Task beendet (`STATUS:DONE`), generiert er zwingend einen neuen Instruction-Anker:
`// SUCCESSOR: @JULES-[ID] — "[EXPLIZITE INSTRUCTION]"`

### 2.2. Der Dispatch Lifecycle (Github Actions)
1. **Trigger:** Push auf `dev` nach Merge eines Pull Requests.
2. **Lock-Sync:** `jules-sync-locks.sh` aktualisiert den DAG State, um High-Level Agenten via Sink-Locks zu blockieren.
3. **Diff-Parsing:** `.agent/scripts/jules-dispatch.sh` vergleicht `HEAD^` und `HEAD` exakt nach der neuen `SUCCESSOR:` Signatur.
4. **API-Key Mapping:** Um GitHub Secrets im Free Tier sicher aufzulösen, nutzt `jules-invoke.yml` eine Bash-Environment Bridge mit Maskierung statt direktem Context-Mapping (`secrets[format(...)]`).
5. **Fallback Flow:** Wenn kein Successor erkannt wurde (z.B. bei Pipeline Breaks), wird standardmäßig JULES-13 als "Debt Hunter" eingeschaltet, um unvorhersehbare Deadlocks abzufangen.

## 3. Akzeptanzkriterien
- [x] Ein GitHub Actions Push Merge evaluiert den `SUCCESSOR:` direkt aus dem Code als Text-Parameter.
- [x] API Keys werden nicht geloggt und sicher dynamisch den Accounts (01-13) übermittelt.
- [x] Keine Agent-Limit Exhaustion Exceptions durch fehlerhafte Loops, dank explizitem Account-ID Handshake.
- [x] Fehlerhafter Parse fällt gracefully auf Account 13 (Debt Hunter/Cleanup) zurück.
