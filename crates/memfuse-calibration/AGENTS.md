# `memfuse-calibration` Crate Instructions

## Zweck
Kalibrierungssubsystem für Konfidenz- und Routing-Scores (Isotonic Calibrator, Platt Scaler).

## Invarianten
- Zero-Panic-Doctrine: Keinesfalls `.unwrap()` oder `.expect()` im Produktionscode verwenden.
