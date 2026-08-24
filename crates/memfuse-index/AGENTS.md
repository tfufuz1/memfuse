# memfuse-index — Crate-Level Agent Rules

## Critical Invariants

### File Writes — Atomic Rename Pattern
`write_to_file()` MUST use tmp-file + atomic rename:
1. Write to `path.with_extension("tmp")`
2. `fsync` the file
3. `rename(tmp, final)` — atomic on POSIX
4. `fsync` the parent directory

### unsafe Scope (ADR-017)
- `distance.rs`: SIMD intrinsics (AVX2, AVX-512, NEON). `#![allow(unsafe_code)]` permitted.
- `diskann.rs` & `persistence.rs`: Single `unsafe { Mmap::map(&file) }` only. MUST have `// SAFETY:` comment.
  Module-wide `#![allow(unsafe_code)]` is FORBIDDEN.
- All other files: `#![forbid(unsafe_code)]`

### load_node() Bounds Check
`neighbor_count` MUST be validated against `max_degree` before use.
Out-of-bounds neighbor counts indicate file corruption — return `Err`, never truncate silently.

### DiskANN Header Validation
`DiskAnnHeader::sector_size` MUST be validated in `load()` against the `Config` value.
Mismatched sector sizes indicate incompatible index files.

## DiskANN Status
Experimental — behind `experimental-diskann` feature flag (ADR-013).
Not integrated into `memfuse-db` Collection flow.
