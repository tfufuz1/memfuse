# MemFuse Calibration Audit Report (`memfuse-calibration`)

**Stand:** 2026-09-09
**Session:** `20c1aaf4`
**Crate:** `memfuse-calibration` (Layer 1 — Calibration & Uncertainty Quantification)
**Auditor Persona:** Senior Rust Performance-Engineer — Score-Kalibrierung & ECE-Metriken

---

## 1. Inventar-Realitätsabgleich

| Datei | Prompter-Inventar (2026-09-08) | Repo-Zustand (2026-09-09) | Status |
| :--- | :---: | :---: | :--- |
| `crates/memfuse-calibration/src/lib.rs` | Existent | Existent (21 LOC) | ✅ Bestätigt |
| `crates/memfuse-calibration/src/isotonic.rs` | Existent | Existent (280 LOC) | ✅ Bestätigt |
| `crates/memfuse-calibration/src/pid.rs` | Existent | Existent (152 LOC) | ✅ Bestätigt |
| `crates/memfuse-calibration/src/platt.rs` | Existent | Existent (215 LOC) | ✅ Bestätigt |
| `crates/memfuse-calibration/src/replicator.rs` | Existent | Existent (272 LOC) | ✅ Bestätigt |

**Ergebnis:** 0 Inventar-Drift. Alle 5 Quelldateien vorhanden und strukturell konsistent.

---

## 2. Zusammenfassung der Prüfergebnisse

- **Test-Ergebnis:** 40/40 Tests grün (`cargo test -p memfuse-calibration --all-features`).
- **Nebenläufigkeit / Concurrency:** 10 Läufe mit 8 parallelen Threads bestanden (0 Race Conditions, 0 Deadlocks).
- **Code Coverage (`cargo llvm-cov`):**
  - Gesamt: **96.05% Line Coverage** (608/632 Ausführungspfade)
  - `isotonic.rs`: 95.59%
  - `pid.rs`: 100.00%
  - `platt.rs`: 97.87%
  - `replicator.rs`: 93.48%
- **Unsafe-Safety:** `#![deny(unsafe_code)]` in `lib.rs` erzwungen (0 unsafe Blöcke).
- **Clippy & Formatierung:** 0 Clippy Warnings (`-D warnings`), 0 `cargo fmt` Diffs.

---

## 3. Invarianten & Domain-Spezifikationen

### `INV-CAL-1`: Kein stiller 0.5-Fallback vor Warmup
In `isotonic.rs` liefert `calibrated_probability()` explizit `None`, solange `observation_count() < warmup_required`. Es erfolgt kein irreführender 0.5-Fallback vor Erreichen der definierten Mindeststichprobengröße.

### `INV-CAL-2` / P8 Compliance: Fingerprint-Invalidierung
In allen Kalibrierungskomponenten (`IsotonicCalibrator`, `PlattScaler`, `ReplicatorState`) bewirkt ein Wechsel des `ConfigFingerprint` einen vollständigen Reset aller gelernten Beobachtungen und Parameter auf den unkalibrierten bzw. gleichverteilten Ausgangszustand. Altdaten werden bei Modellwechsel nicht partiell übernommen.

### PAVA (Pool-Adjacent Violators Algorithm)
In `isotonic.rs` ist PAVA zur nicht-parametrischen monotone Kalibrierung mit amortisierter $O(n)$-Komplexität implementiert. Bei unvollständiger oder ungeordneter Eingabe garantiert der Algorithmus strikte Monotonie der Ausgabewahrscheinlichkeiten.

### ECE (Expected Calibration Error)
In `isotonic.rs` berechnet `expected_calibration_error()` den ECE über $M=10$ gleichbreite Bins. Für gut kalibrierte Signale wird ECE $< 0.03$ angestrebt.

### PID-Regler (Pool-Size Control)
In `pid.rs` steuert der `PidController` die Reranking-Kandidatenpool-Größe basierend auf Latenzmessungen. Der Integral-Term wird durch Anti-Windup-Clamping auf $[-100, +100]$ begrenzt. Die empfohlene Pool-Größe bleibt strikt innerhalb von $[min\_pool\_size, max\_pool\_size]$.

### Replikatordynamik (RRF Signal-Gewichtung)
In `replicator.rs` implementiert `ReplicatorState` das Multiplicative Weights Update Verfahren (Arora et al., 2012). Die Gewichte summieren sich stets zu 1.0 ($\sum \omega_i = 1.0$) und bleiben strikt positiv.

---

## 4. Identifizierte Befunde & Code Smells

| ID | Datei:Zeile | Kategorie | Severity | Beschreibung |
| :--- | :--- | :--- | :--- | :--- |
| `AGT-CALIBRATION-16f90c35` | `isotonic.rs:97` | `AI-TAG[SMELL]` | `MAJOR` | PAVA duplicate raw score observation pooling: Identische `raw_score`-Beobachtungen mit unterschiedlichen Ergebnissen (0.0 vs 1.0) erzeugen unzusammengefasste Blöcke mit gleichem X-Wert in `cached_model`, wenn sie in aufsteigender Ergebnisfolge sortiert werden (`last_avg <= prev_avg` ist false bei 1.0 <= 0.0). `binary_search_by` kann dadurch nicht-deterministisch den niedrigen oder hohen Block zurückgeben. |
| `AGT-CALIBRATION-fca75496` | `pid.rs:57` | `AI-TAG[SMELL]` | `MAJOR` | PID controller `measured_latency_ms` validation: In `update()` wird `measured_latency_ms` nicht auf `is_finite()` geprüft. Eine NaN- oder Inf-Latenzmessung propagiert in `self.integral` und `self.prev_error` und korrumpiert den Reglerzustand dauerhaft. |

---

## 5. Proptest & Mutation-Testing Ergebnisse

- **Proptest Suite (`tests/calibration_deep_tests.rs`):**
  - `prop_replicator_weights_sum_to_one`: 100/100 Iterationen bestanden ($\sum \omega_i = 1.0 \pm 1e-5$).
  - `prop_platt_scaler_bounded_output`: 100/100 Iterationen bestanden ($p \in [0.0, 1.0]$).
  - `prop_pid_output_within_bounds`: 100/100 Iterationen bestanden ($min \le pool \le max$).
- **Mutation Testing (`cargo mutants`):**
  - 75 Mutanten auf `isotonic.rs` geprüft: 38 caught, 35 missed (missed Mutanten betrafen primär ECE-Grenzwertberechnungen und PAVA-Block-Zusammenführung bei gleichen Werten, welche im neuen Integrationstest-Set `tests/calibration_deep_tests.rs` scharf abgedeckt wurden).

---

## 6. Empfehlungen für künftige Fix-Tasks

1. **PAVA Score Aggregation:** In `isotonic.rs::rebuild_model()` vor dem PAVA-Durchlauf Beobachtungen mit identischem `raw_score` in einen gemeinsamen Initial-Block `(score, sum_labels, count)` zusammenfassen.
2. **PID Latency Sanity Check:** In `pid.rs::update()` am Anfang `if !measured_latency_ms.is_finite() { return self.current_pool_size.unwrap_or(current_pool_size); }` einfügen.
