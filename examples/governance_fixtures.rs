//! Example reference fixtures for governance REVIEW-PASS grammar.
//!
//! Note: This file resides in `examples/` outside of `crates/` so that
//! `cargo xtask scan-tags` does not collect these demo review passes as real governance entries.

// ANCHOR[DEBT:CORE-INLINE-001] STATUS:DONE (ID: AGT-CORE-a3f29c1d) (TS:2026-08-29T09:14:07Z) (SESSION: a3f29c1d)
// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T10:00:00Z) (SESSION: b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Tag-Grammatik und SESSION-Identität verifiziert.
// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T11:00:00Z) (SESSION: c9f5e2b3)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Unabhängiges Zweit-Review auf frischem Kontextschnitt durchgeführt.

// ANCHOR[PERF:EVAL-001] STATUS:DONE (TS:2026-08-29T00:00:00Z) (SESSION: a3f29c1d) — Semantic Retrieval Evaluation Framework
// REVIEW-PASS[1/2] STATUS:PASS (ID: PERF:EVAL-001) (TS: 2026-08-29T10:00:00Z) (SESSION: b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: recall benchmark verified
// REVIEW-PASS[2/2] STATUS:PASS (ID: PERF:EVAL-001) (TS: 2026-08-29T11:00:00Z) (SESSION: c9f5e2b3)
// PRÜFER-KONTEXT: FRESH
// BEFUND: recall benchmark second review pass

fn main() {
    println!("Governance review pass fixtures example.");
}
