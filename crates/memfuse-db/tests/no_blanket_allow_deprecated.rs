// FILE-CONTEXT
// ZWECK: Automated regression guard ensuring no blanket crate-level `#![allow(deprecated)]` attribute is reintroduced.
// INVARIANTEN: crates/memfuse-db/src/lib.rs MUST NOT contain `#![allow(deprecated)]`.
// STAND: TS:2026-09-05T00:00:00Z (SESSION: 0dcb9f3b)

#[test]
fn test_no_blanket_allow_deprecated_in_lib_rs() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "crates/memfuse-db".to_string());
    let lib_path = std::path::Path::new(&manifest_dir).join("src/lib.rs");

    let source = std::fs::read_to_string(&lib_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", lib_path.display(), e));

    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("#![allow(deprecated)]") || trimmed.contains("#![allow(deprecated,") {
            panic!(
                "Regression failure: Crate-level blanket deprecation suppression found in {}:{}!\n\
                 Line {}: {}\n\
                 Rule: Crate-wide `#![allow(deprecated)]` is strictly forbidden in memfuse-db.\n\
                 Use item-level `#[allow(deprecated)]` instead and document it in DEPRECATED_DEBT.md.",
                lib_path.display(),
                lineno + 1,
                lineno + 1,
                line
            );
        }
    }
}
