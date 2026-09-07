# ADR-047: SIMD-Implementierungsstrategie — std::arch vs portable_simd (AGT-INDEX-002)


*   **Datum**: 2026-09-03
*   **Status**: ✅ Entschieden
*   **Kontext**: AGT-INDEX-002 dokumentierte, dass `std::simd` (portable_simd, Issue #86656) per
    September 2026 noch nicht auf stable Rust verfügbar ist. `memfuse-index/src/distance.rs` nutzt
    bereits korrekt `std::arch::x86_64` Intrinsics mit Runtime-Feature-Detection via
    `is_x86_feature_detected!` (AVX-512, AVX2, SSE4) und `is_aarch64_feature_detected!` (NEON).
*   **Entscheidung**: Status quo (`std::arch` + Runtime-Detection) ist der korrekte, stabile Pfad.
    Kein Refactoring auf `portable_simd` bis Issue #86656 auf stable Rust landet.
*   **Re-Evaluierungs-Trigger**: Wenn `portable_simd` in einer stable Rust-Version stabilisiert wird,
    soll `distance.rs` auf `std::simd::prelude::*` migriert werden (bessere Cross-Platform-Portabilität,
    weniger `unsafe`-Blöcke nötig).
*   **Konsequenzen**: AGT-INDEX-002 wird als RESOLVED geschlossen. WORKING_STATE.md zeigt danach 0 offene Tags.

---
