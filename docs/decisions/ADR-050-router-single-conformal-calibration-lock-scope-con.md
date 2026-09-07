# ADR-050: Router Single-Conformal Calibration & Lock Scope Consolidation

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Die veraltete Methode `recalibrate()` in `ProfileCalibrationState` wird ersatzlos entfernt. `recalibrate_conformal()` dient als einziger Kalibriermechanismus im Router.
    2. Profilselektion, Candidate Scoring und Kalibrierungs-Update in `RouterEngine::route()` werden innerhalb eines einzigen atomaren Schreib-Locks (`self.calibration.write()`) ausgeführt.
*   **Alternativen**:
    - Beibehaltung des dualen Kalibriersystems: Verworfen, da zwei konkurrierende Kalibriermethoden inkonsistente Schwellenwerte erzeugen.
    - Zweiphasiges Locking (Read Lock für Kaskade, Write Lock für Update): Verworfen wegen TOCTOU-Race-Condition zwischen Read und Write.
*   **Begründung**: Beseitigt TOCTOU-Races bei parallelen Routing-Anfragen und konsolidiert die Kalibrierung auf Conformal Prediction.

---
