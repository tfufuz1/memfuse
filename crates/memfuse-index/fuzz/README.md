# Fuzzing infrastructure for `memfuse-index`

This directory contains `cargo-fuzz` targets for `memfuse-index`.

## Targets

- `hnsw_insert_search`: Fuzzes sequences of `HnswIndex` operations (insert, search, delete) with arbitrary dimensions, vector values (including NaN/Inf), and `k` values.

## Running locally

```bash
cargo fuzz run hnsw_insert_search
```

To run with a time limit (e.g. 60 seconds):

```bash
cargo fuzz run hnsw_insert_search -- -max_total_time=60
```
