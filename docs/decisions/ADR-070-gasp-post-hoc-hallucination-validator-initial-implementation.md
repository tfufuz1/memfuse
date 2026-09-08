# ADR-070: GASP Post-Hoc Halluzinations-Validator (Initiale Implementierung K19)

* **Status:** Akzeptiert (Schließung von K19 als "H2 — initiale Implementierung")
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** K19 aus Gesamtspezifikation v7.0, P8-Kalibrierungsregel, P10-Reuse-Prinzip, P12-Default-Feature-Gating.

## Kontext & Problemstellung
GASP (Grounding-Aware Sensitivity by Perturbation / Post-Hoc-Validator) wurde in der Produktvision und der Gesamtspezifikation (K19) als essenzielle Verteidigungslinie gegen LLM-Halluzinationen konzipiert.
Bisher existierte im Workspace nur ein präventiver Halluzinations-Guard in `crates/memfuse-ollama/src/client.rs`, der das Sprachmodell vorab via Prompt-Constraints zur Kontexttreue instruiert.

Ein präventiver Guard kann jedoch nicht post-hoc verifizieren, ob eine bereits generierte LLM-Antwort tatsächlich durch die abgerufenen Kontext-Chunks belegt ist. Es fehlte ein eigenständiges Modul `gasp.rs`, das nachgelagert Antworten auf Tatsachenbehauptungen (insbesondere Zahlen und Fakten) prüft und bei unzureichender Belegung kontrolliert absteniert.

## Entscheidung
Wir implementieren das neue Modul `crates/memfuse-candle/src/gasp.rs` mit der Struktur `GaspValidator` unter folgenden Architektur- und Entwurfsentscheidungen:

1. **Klare Trennung der Verteidigungslinien (Prevention vs. Post-Hoc):**
   - Der bestehende präventive Guard in `memfuse-ollama` bleibt unverändert bestehen.
   - `GaspValidator` ergänzt die Pipeline als unabhängiger, nachgelagerter Post-Hoc-Check.

2. **Entkoppelte Trait-Grenze in `memfuse-core`:**
   - In `crates/memfuse-core/src/traits/mod.rs` wird der Trait `GroundingValidator` sowie die Datenstruktur `GroundingAssessment` definiert.
   - `GaspValidator` implementiert `GroundingValidator` und hat keine direkte Abhängigkeit von Layer-2/3-Fachcode (`memfuse-db`).

3. **P8-Konforme Kalibrierung & Wiederverwendung:**
   - `GaspValidator` nutzt den bestehenden `IsotonicCalibrator` und `ConfigFingerprint` aus `memfuse-calibration` (P8/P10).
   - Bei Konfigurationsänderungen (z.B. Modell- oder Quantisierungswechsel) wird die Kalibrierung via `invalidate_on_config_change` zurückgesetzt.

4. **Explizites Abstention-Muster:**
   - Fällt der Konfidenz-Score unter den Schwellenwert (`threshold`, Default: 0.70), löst `GaspValidator` einen Abstention-Pfad aus (`Err(MemFuseError::PolicyViolation(...))` mit `LowConfidenceGrounding`).
   - Leerer Kontext (Zero-Shot) liefert ein definiertes Fehlersignal (`Err(MemFuseError::InvalidInput(...))`) ohne Panic.

5. **P12-Feature-Gating:**
   - `gasp.rs` wird in `crates/memfuse-candle/Cargo.toml` hinter das Feature `candle` ge-gated (`#[cfg(feature = "candle")]`). Ohne Opt-in bleibt das Modul unsichtbar.

## Verbleibender Weg zur vollen Produktionsreife
Mit dieser Implementierung wird **K19 als "H2 — initiale Implementierung"** geschlossen. Für die vollständige Produktionsreife (H3 / Produktionsstufe) sind folgende weitere Schritte erforderlich:

1. **Benchmark-Validierung:** Evaluation des `GaspValidator` gegen reale Halluzinations-Benchmark-Datensätze (z.B. LongMemEval, HaluEval).
2. **Log-Likelihood Integration:** Erweiterung um direkte Token-Logit / Perplexitäts-Vergleiche, sobald Candle KV-Cache / Log-Likelihood Expose-APIs vollständig angebunden sind.
3. **End-to-End Orchestrierung:** Anbindung an `memfuse-mcp` Serving-Pipelines als konfigurierbare Post-Processing Middleware.

## Konsequenzen & Garantien
- **K19 geschlossen:** Das Fehlen von `gasp.rs` ist behoben.
- **Null-Regression:** Keine Änderungen an `memfuse-ollama` oder bestehenden Inferenzpfaden.
- **Typen-Integrität:** Vollständige Testabdeckung für unterstützte, halluzinierte und leere Kontext-Szenarien.
