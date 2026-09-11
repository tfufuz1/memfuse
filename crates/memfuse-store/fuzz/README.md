# Fuzzing infrastructure for `memfuse-store`

This directory contains `cargo-fuzz` targets for `memfuse-store`.

## Targets

- `wal_roundtrip`: Fuzzes raw byte streams against `Wal::open_with_config` and `wal.replay()`.

## Running locally

```bash
cargo fuzz run wal_roundtrip
```

To run with a time limit (e.g. 60 seconds):

```bash
cargo fuzz run wal_roundtrip -- -max_total_time=60
```
