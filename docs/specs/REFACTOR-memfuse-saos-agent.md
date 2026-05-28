# REFACTOR-PLAN: memfuse-saos-agent
**Datei:** `docs/specs/REFACTOR-memfuse-saos-agent.md`
**Erstellt:** 2026-05-28
**Priorität:** MEDIUM (Da FROZEN)
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-db, memfuse-checkpoint

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Zeitreise-Fähigkeit| ❌ Defekt (Loops)| 100% Historie |
| Audit-Performance  | ⚠️ O(N) Probing| O(1) Scan     |
| Integrität         | ⚠️ Lücke am Ende| CLOSED GAIL   |
| API-Design         | ✅ Gut (Graphen)| VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-SAOS-002: Checkpoint Naming Collision
**Typ:** Logik-Fehler / Datenverlust
**Datei:** `crates/memfuse-saos-agent/src/engine.rs`
**Zeilen:** 147 (`checkpoint`), 113 (`replay_from`)
**Problem:** Der Checkpoint-Name wird als `task:{task_id}:before:{node_id}` generiert.
**Auswirkung:** In Graphen mit Schleifen (Loops) überschreibt jeder neue Besuch eines Knotens den vorherigen Checkpoint dieses Knotens. Ein Rollback zu einem früheren Durchlauf derselben Schleife ist unmöglich.
**Sovereign Core Verstoß:** Determinismus (Nachvollziehbarkeit).

**Refaktorisierungsanweisung:**
```rust
1. Ändere die Namenskonvention für Checkpoints:
   `task:{task_id}:step:{step_count}:node:{node_id}`.
2. Ermögliche `replay_from` sowohl per `step_count` als auch per `node_id` (letzteres nimmt dann den neuesten).
```

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-SAOS-003: Audit Log Replay Inefficiency
**Typ:** Performance
**Datei:** `crates/memfuse-saos-agent/src/audit.rs`
**Zeilen:** 48–62 (`replay_task`)
**Problem:** Replay probiert sequenziell Keys aus (`step:0, 1, 2...`), bis ein Gap gefunden wird.
**Lösung:** Nutze `collection.scan_prefix(&format!("audit:{}:", task_id))`. Dies ist deutlich effizienter und robuster gegen versehentliche Gaps.

---

#### FIND-SAOS-001: Missing End-State Checkpoint
**Typ:** Robustheit
**Datei:** `crates/memfuse-saos-agent/src/engine.rs`
**Zeilen:** 42–46
**Problem:** Bevor der finale Zustand persistiert wird, fehlt ein Checkpoint.
**Lösung:** Ruft `self.checkpoint(ctx).await?` auch im `End`-Zweig auf.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Checkpoint Naming Refactor (SAOS-002)
Schritt 2: Audit Scan Optimierung (SAOS-003)
Schritt 3: Final State Checkpoint (SAOS-001)
```

## NEUE TESTS

```rust
// TEST-1: test_loop_rollback_integrity
// 1. Definiere Graph: A -> B -> A.
// 2. Lasse Agent 3x durch die Schleife laufen.
// 3. Verifiziere, dass 6 Checkpoints existieren (3x A, 3x B).
// 4. Rollback zum 1. Besuch von A muss funktionieren.

// TEST-2: test_audit_scan_with_gaps
// 1. Manuelle Injektion von Audit-Keys mit Lücke (:1, :3).
// 2. Scan muss trotzdem :3 finden (Robustheit).
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Checkpoint-Historie ist vollständig (Step-basiert).
- [ ] Audit-Replay nutzt Prefix-Scans.
- [ ] `just triple-test -p memfuse-saos-agent` 3× grün.
