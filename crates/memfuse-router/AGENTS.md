# AGENTS.md — memfuse-router
> Layer 3 | SLM-Routing, Conformal Calibration, Context Windowing | ~1300 LOC

## 1. Zweck & Architekturrolle

Entscheidet dynamisch, welches Small Language Model (SLM) für eine gegebene
Agenten-Anfrage optimal ist (`RouterEngine`). Bewertet `SlmProfile` (Kapazität, 
Max-Token-Budget, Domänen-Affinität) und wendet `ConformalCalibrator` an, um
die Modellauswahl basierend auf empirischen Fehler-Raten adaptiv zu steuern.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]` |
| `router.rs` | `RouterEngine` — Die Haupt-Logik, `RoutingDecision`, `ConfidenceMetrics` |
| `profile.rs` | `SlmProfile` — Konfiguration eines Modells, `ConformalCalibrator`, `ProfileCalibrationState` |
| `dispatch.rs` | `dispatch_to_slm` — Execution-Layer für den ausgewählten Pfad |

## 3. Kritische Invarianten

### Token-Budget Einhaltung
Jedes `SlmProfile` definiert ein `max_context_tokens` Limit.
Die `RouterEngine` **darf niemals** ein Modell auswählen, dessen Kapazität für das 
aktuelle `ContextWindow` nicht ausreicht. Wenn kein SLM groß genug ist,
muss der `ContextCompactor` (Layer 2) das Fenster vorab trimmen.

### Conformal Calibration Update (AGT-RTR-001)
Die `ProfileCalibrationState` muss nach jeder Interaktion anhand des LLM-Confidence-Scores 
(bzw. Non-Conformity-Scores) geupdated werden. Die Router-Entscheidung kalibriert
sich so dynamisch, wenn Modelle anfangen zu halluzinieren.

### Community-Score-Boost
Modelle erhalten einen internen Score-Boost (z.B. `1.2x`), wenn die Anfrage
Domänen berührt, für die das SLM laut Profil fine-tuned wurde (z.B. Rust-Code für DeepSeek-Coder).

## 4. Public API Quick-Reference

```rust
// === Profile & Calibration (profile.rs) ===
pub struct SlmProfile {
    pub name: String,
    pub max_context_tokens: usize,
    pub base_confidence: f32,
    pub domain_tags: Vec<String>,
}

pub struct ProfileCalibrationState { ... }
impl ProfileCalibrationState {
    pub fn recalibrate_conformal(&mut self, non_conformity_score: f32) -> bool;
}

// === Router Engine (router.rs) ===
pub struct RouterEngine { ... }
impl RouterEngine {
    pub fn new(collection: Arc<Collection<LsmStorage>>, profiles: Vec<SlmProfile>) -> Self;
    pub async fn route(&self, ctx: &AgentContext, window: &ContextWindow) -> Result<RoutingDecision>;
}

// === Dispatching (dispatch.rs) ===
pub async fn dispatch_to_slm(decision: &RoutingDecision) -> Result<String>;
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — SLM blind ohne Profil aufrufen:
dispatch_to_slm(&RoutingDecision { target: "llama3".into(), ... }).await; // Profil fehlt!
// ✅ KORREKT — RouterEngine entscheiden lassen:
let decision = router.route(ctx, window).await?;
dispatch_to_slm(&decision).await;

// ❌ FALSCH — Kalibrierung nicht aktualisieren:
// ✅ KORREKT — Nach erfolgreichem/fehlerhaftem Task das `ProfileCalibrationState` anpassen.
```

## 6. Concurrency & Lock-Hierarchie

`RouterEngine` hält intern via `parking_lot::RwLock` die aktiven Profile und 
deren Kalibrierungs-Statistiken (`calibration_stats`). Updates hierauf (`recalibrate_conformal`) 
sind synchron und kurz.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0), `memfuse-db` (L2)
- **Verbotene Imports**: `memfuse-mcp` (L4), `memfuse-tauri` (L4)
- **Genutzt von**: `memfuse-mcp`, `memfuse-tauri`, ggf. `memfuse-agent` als Tool

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-040 | SLM Routing Strategy & Calibration |
| `rules/llm_protocol.md` | Context-Window Validation Limits |
