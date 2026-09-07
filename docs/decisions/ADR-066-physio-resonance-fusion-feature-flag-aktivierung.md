# ADR-066: Activation of Feature Flag physio-resonance-fusion for Feature F-09

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:** In `crates/memfuse-db/src/fusion.rs` war der Resonanz-Kohärenz-Bonus (Feature F-09) vollständig hinter `#[cfg(feature = "physio-resonance-fusion")]` implementiert (`apply_resonance_bonus`, `ResonanceConfig` und zugehörige Unit-Tests). Das Feature-Flag `physio-resonance-fusion` fehlte jedoch im `[features]`-Block von `crates/memfuse-db/Cargo.toml`. Der Code war somit in allen Feature-Kombinationen unerreichbar (toter Code aufgrund einer Governance-Lücke).

## Entscheidung
1. Das Feature-Flag `physio-resonance-fusion = []` wird in `crates/memfuse-db/Cargo.toml` unter `[features]` ergänzt.
2. Gemäß Invariante P12 ("Physio-Feature-Default-Unsichtbarkeit") verbleibt `physio-resonance-fusion` standardmäßig inaktiv (Zero-Config-Setup).
3. F-09 gilt nach der Deklaration und verifizierten grünen Unit-Tests als **aktivierbar**, jedoch **nicht automatisch als produktiv kalibriert** (Kalibrierung von Exponent β und Gamma γ erfolgt in nachgelagerten Experimenten).

## Konsequenzen
- **Kompilierung & Verifikation:** `cargo check -p memfuse-db --features physio-resonance-fusion` und die Test-Suite laufen unter dem aktivierten Flag vollständig grün ab.
- **Default-Verhalten:** Ohne das Flag bleibt das Verhalten der Reciprocal Rank Fusion (RRF) exakt unverändert.
- **Governance:** Behebt die Governance-Lücke durch konsistente Deklaration in `Cargo.toml`.
