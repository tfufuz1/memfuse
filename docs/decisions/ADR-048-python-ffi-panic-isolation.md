# ADR-048: Python FFI Panic Isolation

**Status:** Accepted
**Date:** 2026-09-03

## Decision

All `panic!()` calls in `memfuse-py` outside `#[cfg(test)]` are replaced with `Err(PyErr)`.

## Rationale

A Rust panic crossing the PyO3 FFI boundary crashes CPython. `catch_unwind` is not a substitute for correct error handling at call sites.
