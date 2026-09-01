# Security & Technical Audit Report: `memfuse-tauri` (Layer 4)
**Stand:** 2026-09-02 | **Session:** e090c2fa | **Auditor:** Senior Rust Desktop Engineer (Jules Agent)

---

## Executive Summary

`memfuse-tauri` is the Layer 4 Desktop App Shell for MemFuse, responsible for processing non-trusted external files (PDF, DOCX, EML, Markdown, TXT), handling Tauri IPC command dispatches, running regex transformations, and connecting to local Ollama LLM instances.

A comprehensive audit was conducted across all 6 target modules and test suites in `crates/memfuse-tauri/src/`. The audit confirms **0 safety violations, 0 unwrap/expect calls in production code, 100% DAG top-down isolation compliance, and 100% pass rate across all 47 unit & integration tests**.

---

## Key Technical & Security Audit Findings

### 1. Parser Robustness & Panic Safety (Untrusted External Files)
- **PDF Ingestion (`ingestion/pdf.rs`)**:
  - `extract_pdf_bytes` and `extract_pdf_text` wrap `pdf_extract::extract_text_from_mem` inside `std::panic::catch_unwind` and `tokio::task::spawn_blocking`.
  - Malformed, corrupted, truncated, or deeply nested PDF files are safely handled without crashing the Tokio runtime or the Tauri desktop process.
  - File sizes exceeding `MAX_INGEST_FILE_SIZE_BYTES` (100 MB) are systematically rejected with `MemFuseError::InvalidInput`.
- **DOCX Ingestion (`ingestion/docx.rs`)**:
  - `extract_docx_bytes` and `extract_docx_text` wrap `docx_rs::read_docx` in `std::panic::catch_unwind` and `tokio::task::spawn_blocking`.
  - Deeply nested XML tables and corrupted ZIP/DOCX structures return structured errors.
- **EML / Email Ingestion (`ingestion/email.rs`)**:
  - `extract_email_bytes` and `extract_email` wrap `mailparse::parse_mail` in `std::panic::catch_unwind` and `tokio::task::spawn_blocking`.
  - `strip_html` uses size-bounded regex (`size_limit(10 * 1024)`) for script/style tag stripping and block-level tag newline insertion to prevent catastrophic backtracking.
- **Ingestion Pipeline (`ingestion/pipeline.rs`)**:
  - Implements bounded parallel embedding (`EMBED_CONCURRENCY = 8`) with `buffer_unordered`.
  - Aggregates errors per chunk, returning structured `IngestReport` without crashing on partial failures.
  - Automatically extracts graph entities via `SimpleEntityExtractor` and updates `GraphIndex`.

### 2. IPC Security & Input Validation
- **Path Traversal Protection (`commands/mod.rs`)**:
  - `validate_path_within_base` canonicalizes paths using `std::fs::canonicalize` and strictly verifies `canonical_path.starts_with(&canonical_base)`.
  - Path traversal attempts (`../../etc/passwd`) are rejected with `MemFuseError::PolicyViolation`.
- **Collection Name Validation (`commands/collections.rs`)**:
  - `validate_collection_name` enforces length <= 256, non-empty, alphanumeric + `_` + `-`, and rejects reserved `__` prefixes.
- **Query / Message Length Bounds (`commands/search.rs`, `commands/chat.rs`)**:
  - Enforces `MAX_QUERY_LEN = 65_536` (64 KiB) limit on user queries in `hybrid_search` and `chat_with_rag` to mitigate memory exhaustion.

### 3. Regex Transformation Engine & ReDoS Protection (ADR-014)
- **NFA/DFA Execution Guarantees (`commands/transform.rs`)**:
  - Uses Rust `regex` crate (NFA/DFA based, linear time guarantee, no backtracking).
  - Backreferences and lookarounds are rejected at compile time as invalid syntax.
  - Pattern complexity heuristic (`is_structurally_complex`) dynamically reduces input limit from 1 MiB (`MAX_REGEX_INPUT_BYTES`) to 64 KiB (`MAX_REGEX_INPUT_BYTES_COMPLEX`).
  - Matching is bounded by `REGEX_TIMEOUT = Duration::from_secs(5)` inside `tokio::task::spawn_blocking`.
  - Tokio blocking pool protection enforced via `AppState::regex_semaphore` (`MAX_CONCURRENT_REGEX_OPS = 8`).

### 4. DAG Topology & Code Hygiene
- **Layer 4 DAG Compliance**:
  - Imports only Layer 0 (`memfuse-core`), Layer 2 (`memfuse-db`), Layer 3 (`memfuse-ollama`), Layer 1 (`memfuse-graph`), and external crates. No upward or cross-layer violations exist.
- **Unsafe Code Audit**:
  - `0` `unsafe` blocks in `crates/memfuse-tauri/src/`.
- **Error Handling Discipline**:
  - `0` `.unwrap()` or `.expect()` calls in non-test production code. All errors are properly propagated via `Result` or `MemFuseErrorDto`.

---

## Audit Verification Results

```text
cargo check   -p memfuse-tauri --all-features  => OK (0 errors, 0 warnings)
cargo clippy  -p memfuse-tauri -- -D warnings   => OK (0 findings)
cargo fmt     --check -p memfuse-tauri          => OK (0 diffs)
cargo test    -p memfuse-tauri --all-features   => OK (47 passed; 0 failed)
cargo check   --workspace --exclude memfuse-tauri => OK
```

---

## Conclusion & Status

`memfuse-tauri` is **fully verified, robust, and safe** for processing non-trusted external files and handling Tauri desktop IPC commands. All security and parser invariants are active and validated by unit and integration tests.
