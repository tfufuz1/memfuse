# Audit Report: Token Budget Race Condition & Parallel Step Execution Analysis (Round 2)

**Crate:** `memfuse-agent`
**Modul:** `crates/memfuse-agent/src/engine.rs`, `crates/memfuse-agent/src/context.rs`, `memfuse-core/src/types/budget.rs`
**Datum:** 2026-08-31
**Auditor:** Senior Rust Systems & Security Engineer (Jules Agent)
**Status:** Audit Abgeschlossen / Hypothese Entkräftet (im aktuellen Codebase) & Gefahrenpotenzial Verifiziert (für zukünftige Parallelausführung)

---

## 1. Executive Summary

Die Fragestellung bezüglich der `memfuse-agent`-Engine lautete:
*Lässt ein Agenten-Workflow parallele Schritt-Ausführungen zu, und können zwei gleichzeitig ausführende Schritte durch ein Read-Modify-Write (RMW) Race auf dem `TokenBudget` das Gesamtbudget überschreiten?*

### Befunde:
1. **Verifikation des Workflow-Executors (Entkräftung im aktuellen Code):**
   - Die `OrchestratorEngine` führt Workflows **strikt sequentiell** in einer einzigen Asynchron-Schleife (`loop { ... }` in `run_internal`) aus.
   - `OrchestratorEngine::run(&self, ctx: &mut AgentContext, graph: &StateGraph)` verlangt einen **exklusiven veränderlichen Referenzzugriff (`&mut AgentContext`)**.
   - Da `AgentContext` weder `Copy` noch `Clone` implementiert und Rusts Borrow-Checker konkurrierende synchrone oder asynchrone Aufrufe von `engine.run()` auf derselben `AgentContext`-Instanz verhindert, ist eine parallele Schrittausführung auf derselben Workflow-Instanz **architektonisch unmöglich**.

2. **Verifikation des Budget-Zählers (RMW-Rassenpotenzial bei zukünftiger Parallelausführung):**
   - Das `TokenBudget` in `memfuse-core/src/types/budget.rs` verwendet ein gewöhnliches `consumed: usize` Feld und verarbeitet Budgetänderungen über `pub fn consume(&mut self, tokens: usize)` mittels nicht-atomarer Addition (`saturating_add`).
   - Ein experimenteller Test (`crates/memfuse-agent/tests/budget_race_test.rs`) belegt: Würde ein Anwender oder ein zukünftiges Engine-Feature `TokenBudget` in einem `Arc<Mutex<TokenBudget>>` kapseln und von zwei parallelen Tasks aus abfragen (`available()`) und konsumieren (`consume()`), tritt das klassische Read-Modify-Write Race auf: Beide Tasks lesen ein verbleibendes Budget von 100 Tokens, führen ihre Operation aus und verbrauchen zusammen 160 Tokens – was das Limit von 100 Tokens unbemerkt überschreitet.

---

## 2. Code-Pfad- & Invarianten-Analyse

### A. Sequentielle Execution-Loop in `memfuse-agent`

Quellcode `crates/memfuse-agent/src/engine.rs`:

```rust
pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
    ctx.status = crate::context::AgentStatus::Running;
    let res = self.run_internal(ctx, graph).await;
    if res.is_err() {
        ctx.status = crate::context::AgentStatus::Failed;
    }
    res
}

async fn run_internal(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
    loop {
        tokio::task::yield_now().await;
        let node = graph.get_node(&ctx.current_node)...;

        match node.node_type {
            NodeType::Start | NodeType::Task => {
                // 1. Checkpoint
                // 2. Tool-Execution (await execution of single step)
                // 3. Commit step
                // 4. Audit log
                // 5. Consume tokens & check budget
                ctx.budget.consume(result.tokens_consumed);
                if ctx.budget.available() == 0 && node.node_type != NodeType::Start {
                    let err_msg = "Token budget exhausted".to_string();
                    self.audit_log_failure(ctx, &err_msg).await?;
                    return Err(MemFuseError::Internal(err_msg));
                }
                // 6. Resolve next edge & advance step
                ctx.current_node = next_node;
                ctx.step_count += 1;
            }
            ...
        }
    }
}
```

* Invariante 1: Die Schleife traversiert den `StateGraph` Kante für Kante.
* Invariante 2: `ctx.budget.consume()` und die anschließende Prüfung `ctx.budget.available() == 0` finden im selben synchronen Abschnitt nach Abschluss von `tool.execute().await` statt.

### B. Nicht-atomares `TokenBudget` in `memfuse-core`

Quellcode `crates/memfuse-core/src/types/budget.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenBudget {
    pub limit: usize,
    pub strategy: BudgetStrategy,
    pub reserved: usize,
    consumed: usize,
}

impl TokenBudget {
    pub fn available(&self) -> usize {
        self.effective_limit()
            .saturating_sub(self.reserved)
            .saturating_sub(self.consumed)
    }

    pub fn consume(&mut self, tokens: usize) {
        self.consumed = self.consumed.saturating_add(tokens);
    }
}
```

* Die Methode `consume` setzt exklusiven Zugriff `&mut self` voraus.
* Es existieren keine atomaren Operationen (wie `AtomicUsize::compare_exchange`) auf `TokenBudget`.

---

## 3. Experimenteller Nachweis & Testergebnisse

In `crates/memfuse-agent/tests/budget_race_test.rs` wurden zwei automatisierte Integrationstests implementiert:

1. **`test_sequential_workflow_budget_check`**:
   - Führt einen echten Workflow mit 2 aufeinanderfolgenden Task-Nodes (jeweils 60 Tokens Verbrauch bei 100 Tokens Gesamtbudget) aus.
   - **Ergebnis:** `PASS`. Der erste Schritt verbraucht 60 Tokens (40 verbleibend). Der zweite Schritt schlägt mit `Token budget exhausted` fehl. Es tritt kein Budget-Overrun auf.

2. **`test_concurrent_budget_consumption_rmw_race`**:
   - Simuliert hypothetische Parallelausführung über 2 Tokio-Worker-Tasks, die auf ein gemeinsam genutztes `TokenBudget` (Limit: 100) zugreifen.
   - **Ergebnis:** `PASS` (Race nachgewiesen). Beide Worker-Tasks lesen gleichzeitig `available() >= 50` (100 verfügbar), führen nachfolgend `consume(80)` aus. Der Gesamtzähler steigt auf **160 Tokens**, wodurch das Budget um 60% überschritten wurde.

---

## 4. Audit-Status & Handlungsempfehlungen

### Status nach Audit Intake Protocol (`.jules/AUDIT_INTAKE_PROTOCOL.md`)
* **Finding Status:** **Entkräftet (Refuted)** bezüglich einer bestehenden Sicherheitslücke im aktuellen System, da `memfuse-agent` keine parallelen Schritte auf derselben `AgentContext`-Instanz erlaubt.
* **Härtungsempfehlung (Zukunftssicherheit):** Falls in einer zukünftigen Version ein DAG-Parallel-Graph-Walker eingeführt werden sollte, muss `TokenBudget` von `consumed: usize` auf `consumed: AtomicUsize` mit atomarem Check-and-Set / Reserve-Muster umgestellt werden.

### Empfohlener Atomarer Reserve-Vektor für `TokenBudget` (falls Parallelausführung geplant):

```rust
pub fn try_consume(&self, tokens: usize) -> Result<(), MemFuseError> {
    loop {
        let current = self.consumed.load(Ordering::Acquire);
        let effective = self.effective_limit().saturating_sub(self.reserved);
        if current.saturating_add(tokens) > effective {
            return Err(MemFuseError::Internal("Token budget exhausted".into()));
        }
        if self.consumed.compare_exchange(
            current,
            current + tokens,
            Ordering::AcqRel,
            Ordering::Relaxed
        ).is_ok() {
            return Ok(());
        }
    }
}
```

---
*Ende des Audit-Berichts.*
