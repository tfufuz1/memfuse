# SPEC-035: Chaos Engineering Pipeline

> **Stand:** 2026-04-05 | **Prio:** P2 (Internal Plugin) | **Status:** 🔴 PLANUNG  
> **Crate:** `chimera-chaos` (Internal Plugin) | **Basis:** SPEC-034 §3.1

---

## Ziel

Systematische Verifikation der Resilienz unter "unmöglichen" Bedingungen für interne Qualitätssicherung. ChimeraDB nutzt dieses Plugin intern für extreme Stresstests (Chaos Engineering) ohne Datenverlust oder Inkonsistenz. Es ist kein Kernbestandteil für reguläre Corporate Deployments.

---

## Implementierung: `chimera-chaos` Crate

### Neue Datei: `crates/chimera-chaos/src/lib.rs`

```rust
// SPEC-035: Chaos Engineering Pipeline
// INVARIANT: Alle Szenarien müssen mit `just nextest run -p chimera-chaos` bestehen.

pub enum ChaosScenario {
    /// Tötet 50% der Tokio-Tasks mid-execution. Verifiziert Crash-Recovery (SPEC-029).
    TaskMassacre { kill_ratio: f32 },

    /// Injiziert zufällige Bit-Flips in SSTable-Blöcke auf Disk.
    /// Verifiziert CRC32C-Erkennung (SPEC-031) und PITR-Recovery.
    BitFlipInjection { flip_probability: f64 },

    /// Simuliert 90% Packet-Loss auf dem Sync-Channel zwischen Nodes.
    /// Verifiziert Raft-Konsistenz unter Partition (SPEC-036).
    NetworkDegradation { packet_loss_ratio: f32 },

    /// Füllt RAM bis 95%, dann schreibt 1000 Docs.
    /// Verifiziert OOM-Rejection und Backpressure (SPEC-025).
    MemoryExhaustion { fill_ratio: f32 },

    /// Lässt einen Agent 10.000 Writes/s schicken (defekter Agent).
    /// Verifiziert Rate-Limiting (SPEC-028).
    RogueAgentFlood { writes_per_second: u32 },

    /// Stoppt den Prozess ohne graceful shutdown während WAL-Write.
    /// Verifiziert deterministisches Replay (SPEC-029).
    PowerCutSimulation { trigger_after_writes: usize },
}

pub struct ChaosRunner {
    /// Akzeptanzkriterien nach Szenario
    validators: Vec<Box<dyn ChaosValidator>>,
}

pub trait ChaosValidator: Send + Sync {
    /// Prüft System-Invarianten nach einem Chaos-Szenario.
    /// Gibt `Ok(())` wenn alle Invarianten erfüllt, sonst `Err(Vec<Violation>)`.
    async fn validate(&self, system: &ChimeraTestSystem) -> Result<(), Vec<InvariantViolation>>;
}
```

---

## CI-Integration

Jeder PR, der `chimera-storage`, `chimera-sync` oder `chimera-query` berührt, muss die
Chaos-Suite bestehen:

```yaml
# .github/workflows/chaos.yml
jobs:
  chaos:
    runs-on: ubuntu-latest
    steps:
      - name: Run Chaos Suite
        run: nix develop --command just chaos-test

# justfile
chaos-test:
    cargo nextest run -p chimera-chaos --test-threads=1 --timeout=300s
```

---

## Akzeptanzkriterien

| Szenario | Invariante | Akzeptanzkriterium |
|:---------|:-----------|:-------------------|
| TaskMassacre | INV-C1 | Nach Recovery: 0 Inkonsistenzen in allen Indizes |
| BitFlipInjection | SPEC-031 | Alle korrumpierten Blöcke erkannt, PITR erfolgreich |
| NetworkDegradation | SPEC-036 | Kein Split-Brain; Raft convergiert innerhalb 10s |
| MemoryExhaustion | INV-R1, INV-R4 | System lehnt neue Schreibvorgänge ab, kein OOM-Kill |
| RogueAgentFlood | SPEC-028 | Rogue-Agent geblockt, legale Agents nicht betroffen |
| PowerCutSimulation | SPEC-029 | WAL-Replay vollständig deterministisch, 0 Datenverlust |
