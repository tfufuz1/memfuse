//! Example/Demo governance fixtures for reference and testing outside of active crates.
//!
//! Note: These review-pass entries use sample session hashes (b8e4f1a2, c9f5e2b3) and are placed
//! outside of `crates/` so they are not scanned as real governance evidence by xtask.

// ANCHOR[DEBT:CORE-INLINE-001] STATUS:DONE (ID: AGT-CORE-a3f29c1d) (TS:2026-08-29T09:14:07Z) (SESSION: a3f29c1d)
// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T10:00:00Z) (SESSION: b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Tag-Grammatik und SESSION-Identität verifiziert.
// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-CORE-a3f29c1d) (TS: 2026-08-29T11:00:00Z) (SESSION: c9f5e2b3)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Unabhängiges Zweit-Review auf frischem Kontextschnitt durchgeführt.

// ANCHOR[TEST:CRY-001] STATUS:DONE (ID: AGT-CRYP-3779c7f0) (TS:2026-08-30T18:54:39Z) (SESSION:3779c7f0)
// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-CRYP-3779c7f0) (TS:2026-08-30T19:00:00Z) (SESSION:b8e4f1a2)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Verified parallel nonce uniqueness logic
// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-CRYP-3779c7f0) (TS:2026-08-30T19:05:00Z) (SESSION:c9f5e2b3)
// PRÜFER-KONTEXT: FRESH
// BEFUND: Verified parallel nonce uniqueness logic independent session 2

fn main() {
    println!("Governance fixtures example.");
}
