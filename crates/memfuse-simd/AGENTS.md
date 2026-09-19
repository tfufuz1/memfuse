# AGENTS.md — memfuse-simd
> Layer 0 | SIMD Distance Kernels & Dynamic Hardware Dispatch | ~2000 LOC

## 1. Zweck & Architekturrolle

Ring 0 Unsafe Island Crate für hoch-optimierte SIMD Distanz-Berechnungen (Cosine, L2, Dot Product).
Isoliert hardware-spezifische Instruktionen (AVX2, AVX-512, NEON) und dynamische Runtime-Dispatch-Logik,
während eine vollständig sichere öffentliche API bereitgestellt wird. Exponiert `#![deny(unsafe_op_in_unsafe_fn)]`.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Öffentliche, safe API-Schnittstelle & Re-Exports |
| `dispatch.rs` | Hardware-Feature-Erkennung & dynamisches Dispatching |
| `kernels/` | Hardware-spezifische Distanz-Kernel (Scalar, AVX2, AVX-512, NEON) |

## 3. Kritische Invarianten

### Safe API Boundary
Die öffentliche Schnittstelle von `memfuse-simd` ist zu 100% sicher (`safe`). Alle `unsafe`-Blöcke
sind strikt innerhalb des Crates gekapselt und müssen die `#![deny(unsafe_op_in_unsafe_fn)]` Lint erfüllen.

### Zero-Panic / Scalar Fallback
Für jede Vektor-Distanzfunktion muss immer ein valider skalarer Fallback existieren, falls die
ausführende CPU die SIMD-Erweiterungen nicht unterstützt.

## 4. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-store` (L1), `memfuse-index` (L1 Peer), `memfuse-db` (L2)
